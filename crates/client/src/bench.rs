//! Bench mode: plays a bot match on its own and measures how well the
//! animation meets the ball and how fast frames come, then writes it all to a
//! JSON file and quits. For comparing animation techniques; see
//! `docs/animation/experiments.md`. Off unless `VOLLEY_BENCH` is set:
//!
//!     VOLLEY_BENCH=out.json [VOLLEY_BENCH_SECONDS=90] [VOLLEY_BENCH_ARENA=beach|neon] \
//!         [VOLLEY_BENCH_HERO=0] [VOLLEY_BENCH_SHOTS=dir] cargo run -p volley_client
//!
//! With `VOLLEY_BENCH_SHOTS`, it also saves a screenshot at the first few
//! passes, spikes and serves, named `NN_Kind_X_Y.png` with the hitter's spot
//! on screen (pixels), for cropping. Its window then stays on top of others:
//! macOS doesn't draw a covered window (nor any, with the screen locked; run
//! it under `caffeinate -d`), and screenshots of it come out black.
//!
//! The match is the same every run (bots and the simulation are
//! deterministic), so two runs differ only in how they're drawn and animated.
//! It runs for a set number of simulation seconds. Frame times are capped by
//! vsync (macOS keeps it on), so they show hitches, not spare headroom.

use std::fmt::Write as _;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use volley_sim::court::BALL_RADIUS;
use volley_sim::{Event, HitKind, MoveId, TICK_HZ};

use crate::arena::Arena;
use crate::characters::{PlayerBody, PoseLimbs, SwingReleased};
use crate::feel::{BaseSpeed, Froze};
use crate::flow::Screen;
use crate::input::LocalDriver;
use crate::scene::BallView;
use crate::{Match, SimEvent, heroes};

/// The bench settings, read from the environment.
#[derive(Resource, Clone)]
pub struct Bench {
    pub out: String,
    pub seconds: u32,
    pub arena: Arena,
    pub hero: usize,
    pub shots: Option<String>,
    /// Where to save a film: every other frame from `film_from` for
    /// `film_seconds` (simulation seconds), as numbered PNGs to stitch with
    /// `ffmpeg -framerate 30 -i DIR/%04d.png out.mp4`.
    pub film: Option<String>,
    pub film_from: f32,
    pub film_seconds: f32,
    /// `VOLLEY_BENCH_CAM=action`: a director's camera follows whoever plays
    /// the ball next, close and from the side, to review animation.
    pub director: bool,
}

impl Bench {
    /// Bench settings if `VOLLEY_BENCH` asks for a run.
    pub fn from_env() -> Option<Self> {
        let out = std::env::var("VOLLEY_BENCH").ok()?;
        let var = |name: &str| std::env::var(name).ok();
        Some(Self {
            out,
            seconds: var("VOLLEY_BENCH_SECONDS").and_then(|s| s.parse().ok()).unwrap_or(90),
            arena: if var("VOLLEY_BENCH_ARENA").as_deref() == Some("beach") { Arena::Beach } else { Arena::Neon },
            hero: var("VOLLEY_BENCH_HERO").and_then(|s| s.parse().ok()).unwrap_or(0),
            shots: var("VOLLEY_BENCH_SHOTS"),
            film: var("VOLLEY_BENCH_FILM"),
            film_from: var("VOLLEY_BENCH_FILM_FROM").and_then(|s| s.parse().ok()).unwrap_or(2.0),
            film_seconds: var("VOLLEY_BENCH_FILM_SECONDS").and_then(|s| s.parse().ok()).unwrap_or(8.0),
            director: var("VOLLEY_BENCH_CAM").as_deref() == Some("action"),
        })
    }
}

pub fn plugin(app: &mut App) {
    let Some(bench) = Bench::from_env() else { return };
    app.insert_resource(bench.arena)
        .insert_resource(bench)
        .init_resource::<Record>()
        .add_systems(Update, (start, frames, releases, freezes, film, finish))
        .add_systems(PostUpdate, (touches, hands, feet).after(PoseLimbs))
        .add_systems(PostUpdate, direct.before(bevy::transform::TransformSystems::Propagate));
}

#[derive(Resource, Default)]
struct Record {
    started: bool,
    start_tick: u32,
    frame_ms: Vec<f32>,
    /// (hit kind, gap from the nearest palm or toes to the ball's surface, m).
    touches: Vec<(HitKind, f32)>,
    releases: Vec<SwingReleased>,
    freezes: Vec<Froze>,
    /// Every frame, every palm's speed (m/s), and where each palm and its
    /// body were; palms whose whole body jumped to a new spot.
    hand_speeds: Vec<f32>,
    palms: Vec<(Entity, Vec3, Vec3)>,
    teleports: u32,
    /// Every frame, every planted foot's speed along the ground (m/s), and
    /// where each foot was.
    foot_slides: Vec<f32>,
    feet: Vec<(Entity, Vec3)>,
    /// Frames a running body faced more than 30° away from where it ran, of
    /// all running frames; and the same for its hips (where the legs point).
    strafing: (u32, u32),
    hips_strafing: u32,
    /// Running frames' angles (degrees) between where the body, and its hips,
    /// face and where it runs.
    off_degrees: Vec<(f32, f32)>,
    /// Each standing player's torso lean from upright (degrees, hips to neck)
    /// every frame, outside dives and knockdowns.
    tilts: Vec<f32>,
    rally: u32,
    shots: u32,
    film_frame: u32,
    film_last: f32,
    film_log: String,
}

/// A body moving this far in one frame was put somewhere new, not animated.
const TELEPORT: f32 = 1.0;

/// Screenshots saved per kind of hit, at most.
const SHOTS_PER_KIND: usize = 6;

fn start(bench: Res<Bench>, mut record: ResMut<Record>, mut game: ResMut<Match>, mut driver: ResMut<LocalDriver>, mut next: ResMut<NextState<Screen>>) {
    if record.started {
        return;
    }
    let sim = heroes::new_match(bench.hero);
    record.start_tick = sim.tick;
    *game = Match { previous: sim.clone(), current: sim };
    *driver = LocalDriver::Bot;
    next.set(Screen::Playing);
    record.started = true;
}

fn frames(time: Res<Time<Real>>, screen: Res<State<Screen>>, mut record: ResMut<Record>) {
    if *screen.get() == Screen::Playing {
        record.frame_ms.push(time.delta_secs() * 1000.0);
    }
}

/// How fast every palm moves each frame, as drawn: a pop (a bone jumping
/// between poses) shows up as an impossible speed. A whole body moving to a
/// new spot (a match starting) counts as a teleport instead.
fn hands(
    time: Res<Time<Real>>,
    game: Res<Match>,
    screen: Res<State<Screen>>,
    palms: Query<(Entity, &Name, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    bodies: Query<&GlobalTransform, With<PlayerBody>>,
    mut record: ResMut<Record>,
) {
    let dt = time.delta_secs();
    if *screen.get() != Screen::Playing || dt <= 0.0 {
        return;
    }
    // Each palm, with where its body is.
    let now: Vec<(Entity, Vec3, Vec3)> = palms
        .iter()
        .filter(|(_, name, _)| matches!(name.as_str(), "middle_01_l" | "middle_01_r"))
        .filter_map(|(entity, _, at)| {
            let body = parents.iter_ancestors(entity).find_map(|a| bodies.get(a).ok())?;
            Some((entity, at.translation(), body.translation()))
        })
        .collect();
    // Skip the jump when a rally resets everyone's position, and the first
    // second, while models load and take their first pose.
    if game.current.rally == record.rally && game.current.tick > record.start_tick + TICK_HZ {
        for &(entity, at, body) in &now {
            let Some(&(_, before, body_before)) = record.palms.iter().find(|(e, ..)| *e == entity) else { continue };
            if body.distance(body_before) > TELEPORT {
                record.teleports += 1;
            } else {
                record.hand_speeds.push(at.distance(before) / dt);
            }
        }
    }
    record.rally = game.current.rally;
    record.palms = now;
}

/// A foot this low (the ankle, m) is on the ground.
const PLANTED: f32 = 0.14;
/// A standing torso leaning further than this from upright (degrees) is folded over.
const FOLDED: f32 = 60.0;
/// Running, for counting how often the body faces away from where it runs.
const RUNNING: f32 = 1.5;

/// How much planted feet slide (a foot on the ground should stay put), and how
/// often a running body faces away from where it runs.
fn feet(
    time: Res<Time<Real>>,
    game: Res<Match>,
    screen: Res<State<Screen>>,
    bones: Query<(Entity, &Name, &GlobalTransform)>,
    bodies: Query<(Entity, &PlayerBody, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    mut record: ResMut<Record>,
) {
    let dt = time.delta_secs();
    if *screen.get() != Screen::Playing || dt <= 0.0 {
        return;
    }
    let now: Vec<(Entity, Vec3)> = bones
        .iter()
        .filter(|(_, name, _)| matches!(name.as_str(), "foot_l" | "foot_r"))
        .map(|(entity, _, at)| (entity, at.translation()))
        .collect();
    for (entity, body, _) in &bodies {
        let player = &game.current.players[body.0];
        let diving = player.action.is_some_and(|action| matches!(action.id, MoveId::Dive | MoveId::FootSave));
        if !player.grounded() || player.stunned(game.current.tick) || diving {
            continue;
        }
        let joint = |name: &str| {
            bones.iter().find(|(e, n, _)| n.as_str() == name && parents.iter_ancestors(*e).any(|a| a == entity)).map(|(.., at)| at.translation())
        };
        if let (Some(hips), Some(neck)) = (joint("pelvis"), joint("neck_01")) {
            record.tilts.push((neck - hips).angle_between(Vec3::Y).to_degrees());
        }
    }
    if game.current.rally == record.rally {
        for &(entity, at) in &now {
            let Some(&(_, before)) = record.feet.iter().find(|(e, _)| *e == entity) else { continue };
            let slide = Vec2::new(at.x - before.x, at.z - before.z).length() / dt;
            if at.y < PLANTED && before.y < PLANTED && slide < 20.0 {
                record.foot_slides.push(slide);
            }
        }
        for (entity, body, transform) in &bodies {
            let player = &game.current.players[body.0];
            if !player.grounded() || player.velocity.length() < RUNNING {
                continue;
            }
            let facing = transform.rotation() * Vec3::Z;
            let angle = Vec2::new(facing.x, facing.z).angle_to(player.velocity).abs();
            record.strafing.1 += 1;
            if angle > 30f32.to_radians() {
                record.strafing.0 += 1;
            }
            // The hips face square to the line between the hip joints.
            let joint = |name: &str| {
                bones.iter().find(|(e, n, _)| n.as_str() == name && parents.iter_ancestors(*e).any(|a| a == entity)).map(|(.., at)| at.translation())
            };
            if let (Some(left), Some(right)) = (joint("thigh_l"), joint("thigh_r")) {
                let hips = Vec3::Y.cross(right - left);
                let hips_angle = Vec2::new(hips.x, hips.z).angle_to(player.velocity).abs();
                if hips_angle > 30f32.to_radians() {
                    record.hips_strafing += 1;
                }
                record.off_degrees.push((angle.to_degrees(), hips_angle.to_degrees()));
            }
        }
    }
    record.feet = now;
}

fn freezes(mut froze: MessageReader<Froze>, mut record: ResMut<Record>) {
    record.freezes.extend(froze.read().copied());
}

fn releases(mut released: MessageReader<SwingReleased>, mut record: ResMut<Record>) {
    for release in released.read() {
        record.releases.push(*release);
    }
}

/// At each touch, how far the toucher's nearest palm (the middle knuckle) or
/// toes are from the ball's surface as drawn: what you'd see on screen. 0 is
/// a hand on the ball.
fn touches(
    mut commands: Commands,
    bench: Res<Bench>,
    game: Res<Match>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    window: Single<&Window>,
    mut events: MessageReader<SimEvent>,
    ball: Single<&GlobalTransform, With<BallView>>,
    bodies: Query<(Entity, &PlayerBody)>,
    bones: Query<(Entity, &Name, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    mut record: ResMut<Record>,
) {
    for SimEvent(event) in events.read() {
        let Event::Touched { player, kind, .. } = *event else { continue };
        let Some((body, _)) = bodies.iter().find(|(_, b)| b.0 == player) else { continue };
        let at = ball.translation();
        let gap = bones
            .iter()
            .filter(|(_, name, _)| matches!(name.as_str(), "middle_01_l" | "middle_01_r" | "ball_l" | "ball_r"))
            .filter(|(entity, ..)| parents.iter_ancestors(*entity).any(|a| a == body))
            .map(|(_, _, bone)| (bone.translation().distance(at) - BALL_RADIUS).max(0.0))
            .fold(f32::MAX, f32::min);
        if gap < f32::MAX {
            record.touches.push((kind, gap));
        }
        if bench.film.is_some() {
            record.film_log.push_str(&format!("touch tick {} player {player} {kind:?}\n", game.current.tick));
        }
        let shown = record.touches.iter().filter(|(k, _)| *k == kind).count();
        if let Some(dir) = &bench.shots
            && matches!(kind, HitKind::Pass | HitKind::Spike | HitKind::Serve)
            && shown <= SHOTS_PER_KIND
            && let Ok(spot) = camera.0.world_to_viewport(camera.1, game.current.players[player].position + Vec3::Y * 1.2)
        {
            record.shots += 1;
            let scale = window.scale_factor();
            let path = format!("{dir}/{:02}_{kind:?}_{:.0}_{:.0}.png", record.shots, spot.x * scale, spot.y * scale);
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
    }
}

/// The director's camera: how far from its subject (m), how high, how high
/// it looks, and how
/// quickly it glides to a new one (per second).
const DIRECTOR_DISTANCE: f32 = 4.5;
const DIRECTOR_HEIGHT: f32 = 1.7;
const DIRECTOR_LOOK: f32 = 1.5;
const DIRECTOR_GLIDE: f32 = 3.0;

/// Films whoever plays the ball next, side on, close: the player with the
/// soonest contact coming, else the last to touch it.
fn direct(
    bench: Res<Bench>,
    game: Res<Match>,
    time: Res<Time<Real>>,
    mut subject: Local<Option<usize>>,
    mut aim: Local<Option<(Vec3, Vec3)>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    if !bench.director {
        return;
    }
    let sim = &game.current;
    let next = (0..sim.players.len())
        .filter_map(|i| sim.predicted_contact(i, 90).map(|(ticks, _)| (ticks, i)))
        .min()
        .map(|(_, i)| i);
    if next.is_some() {
        *subject = next;
    }
    let Some(who) = *subject else { return };
    let body = sim.players[who].position;
    let ball = sim.ball_position();
    // Side on to the line from the player to the ball.
    let toward = Vec2::new(ball.x - body.x, ball.z - body.z).normalize_or(Vec2::X);
    let side = Vec3::new(-toward.y, 0.0, toward.x);
    // Steady at a height that frames the floor and a spike's reach, so jumps
    // read as jumps.
    let look = Vec3::new(body.x, DIRECTOR_LOOK, body.z);
    let eye = look + side * DIRECTOR_DISTANCE + Vec3::Y * (DIRECTOR_HEIGHT - DIRECTOR_LOOK);
    let (eye_now, look_now) = aim.get_or_insert((eye, look));
    let glide = 1.0 - (-DIRECTOR_GLIDE * time.delta_secs()).exp();
    *eye_now = eye_now.lerp(eye, glide);
    *look_now = look_now.lerp(look, glide);
    **camera = Transform::from_translation(*eye_now).looking_at(*look_now, Vec3::Y);
}

/// The game runs this fast while filming, so every saved frame covers a
/// sliver of game time and the film plays back smooth and at real speed.
const FILM_SPEED: f32 = 0.2;

/// Saves a frame each time the game has moved on a thirtieth of a second
/// while filming, slowing the game down meanwhile; the film plays at 30 fps.
fn film(
    mut commands: Commands,
    bench: Res<Bench>,
    game: Res<Match>,
    mut base: ResMut<BaseSpeed>,
    mut clock: ResMut<Time<Virtual>>,
    mut record: ResMut<Record>,
) {
    let Some(dir) = &bench.film else { return };
    let seconds = game.current.tick.saturating_sub(record.start_tick) as f32 / TICK_HZ as f32;
    let filming = record.started && seconds >= bench.film_from && seconds <= bench.film_from + bench.film_seconds;
    let speed = if filming { FILM_SPEED } else { 1.0 };
    if base.0 != speed {
        base.0 = speed;
        clock.set_relative_speed(speed);
    }
    if !filming {
        return;
    }
    let now = clock.elapsed_secs();
    if now - record.film_last >= 1.0 / 30.0 {
        record.film_last = now;
        record.film_frame += 1;
        let path = format!("{dir}/{:04}.png", record.film_frame);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        // Which tick each frame shows, to find moments in the film.
        let line = format!("frame {} tick {}\n", record.film_frame, game.current.tick);
        record.film_log.push_str(&line);
    }
}

fn finish(bench: Res<Bench>, game: Res<Match>, record: Res<Record>) {
    if !record.started || game.current.tick < record.start_tick + bench.seconds * TICK_HZ {
        return;
    }
    let json = report(&bench, &record);
    if let Some(dir) = &bench.film {
        let _ = std::fs::write(format!("{dir}/log.txt"), &record.film_log);
    }
    if let Err(error) = std::fs::write(&bench.out, json) {
        eprintln!("bench: couldn't write {}: {error}", bench.out);
    } else {
        println!("bench: wrote {}", bench.out);
    }
    std::process::exit(0);
}

/// The value at fraction `q` (0 to 1) through sorted `values`.
fn quantile(values: &[f32], q: f32) -> f32 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted.get(((sorted.len() as f32 - 1.0) * q).round() as usize).copied().unwrap_or(0.0)
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() { 0.0 } else { values.iter().sum::<f32>() / values.len() as f32 }
}

fn report(bench: &Bench, record: &Record) -> String {
    let mut out = String::new();
    let frames = &record.frame_ms;
    let _ = write!(
        out,
        "{{\n  \"seconds\": {},\n  \"arena\": \"{:?}\",\n  \"hero\": {},\n  \"frames\": {},\n  \"frame_ms\": {{ \"mean\": {:.2}, \"p50\": {:.2}, \"p95\": {:.2}, \"p99\": {:.2} }},\n",
        bench.seconds,
        bench.arena,
        bench.hero,
        frames.len(),
        mean(frames),
        quantile(frames, 0.5),
        quantile(frames, 0.95),
        quantile(frames, 0.99),
    );
    let speeds = &record.hand_speeds;
    let _ = write!(
        out,
        "  \"hand_speed\": {{ \"p99\": {:.2}, \"p999\": {:.2}, \"max\": {:.2}, \"over_30\": {}, \"over_40\": {}, \"teleports\": {} }},\n",
        quantile(speeds, 0.99),
        quantile(speeds, 0.999),
        quantile(speeds, 1.0),
        speeds.iter().filter(|&&v| v > 30.0).count(),
        speeds.iter().filter(|&&v| v > 40.0).count(),
        record.teleports,
    );
    let tilts = &record.tilts;
    let _ = write!(
        out,
        "  \"posture\": {{ \"tilt_mean\": {:.1}, \"tilt_p99\": {:.1}, \"folded\": {} }},\n",
        mean(tilts),
        quantile(tilts, 0.99),
        tilts.iter().filter(|&&v| v > FOLDED).count(),
    );
    let slides = &record.foot_slides;
    let _ = write!(
        out,
        "  \"feet\": {{ \"slide_mean\": {:.3}, \"slide_p90\": {:.3}, \"sliding_share\": {:.3}, \"strafing_share\": {:.3}, \"hips_strafing_share\": {:.3}, \"body_off_deg\": {:.1}, \"hips_off_deg\": {:.1} }},\n",
        mean(slides),
        quantile(slides, 0.9),
        slides.iter().filter(|&&v| v > 0.5).count() as f32 / slides.len().max(1) as f32,
        record.strafing.0 as f32 / record.strafing.1.max(1) as f32,
        record.hips_strafing as f32 / record.strafing.1.max(1) as f32,
        mean(&record.off_degrees.iter().map(|(body, _)| *body).collect::<Vec<_>>()),
        mean(&record.off_degrees.iter().map(|(_, hips)| *hips).collect::<Vec<_>>()),
    );

    // Contact gaps by kind of hit.
    let mut kinds: Vec<HitKind> = record.touches.iter().map(|(kind, _)| *kind).collect();
    kinds.dedup();
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    kinds.dedup();
    let all: Vec<f32> = record.touches.iter().map(|(_, gap)| *gap).collect();
    let _ = writeln!(out, "  \"gap_m\": {{\n    \"all\": {},", stats(&all));
    let rows: Vec<String> = kinds
        .iter()
        .map(|kind| {
            let gaps: Vec<f32> = record.touches.iter().filter(|(k, _)| k == kind).map(|(_, gap)| *gap).collect();
            format!("    \"{kind:?}\": {}", stats(&gaps))
        })
        .collect();
    let _ = writeln!(out, "{}\n  }},", rows.join(",\n"));

    // Swing timing by clip: how far from its contact frame the swing was when
    // the ball was touched (clip seconds; 0 is perfect), and how often the
    // swing had been timed to the touch.
    let mut clips: Vec<&str> = record.releases.iter().map(|r| r.clip).collect();
    clips.sort_unstable();
    clips.dedup();
    let rows: Vec<String> = clips
        .iter()
        .map(|clip| {
            let of: Vec<&SwingReleased> = record.releases.iter().filter(|r| r.clip == *clip).collect();
            let off: Vec<f32> = of.iter().map(|r| (r.contact - r.at).abs()).collect();
            let timed = of.iter().filter(|r| r.timed).count() as f32 / of.len() as f32;
            let warp: Vec<f32> = of.iter().map(|r| r.warp).collect();
            format!(
                "    \"{clip}\": {{ \"n\": {}, \"timed\": {:.2}, \"off_s_mean\": {:.3}, \"off_s_p90\": {:.3}, \"warp_m_mean\": {:.2} }}",
                of.len(),
                timed,
                mean(&off),
                quantile(&off, 0.9),
                mean(&warp)
            )
        })
        .collect();
    let _ = writeln!(out, "  \"swings\": {{\n{}\n  }},", rows.join(",\n"));

    // Hit-stop by kind of hit: how fast the ball left (m/s) and how long the
    // game froze (ms).
    let mut kinds: Vec<String> = record.freezes.iter().map(|f| format!("{:?}", f.kind)).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let rows: Vec<String> = kinds
        .iter()
        .map(|kind| {
            let of: Vec<&Froze> = record.freezes.iter().filter(|f| format!("{:?}", f.kind) == *kind).collect();
            let speed: Vec<f32> = of.iter().map(|f| f.speed).collect();
            let ms: Vec<f32> = of.iter().map(|f| f.seconds * 1000.0).collect();
            format!(
                "    \"{kind}\": {{ \"n\": {}, \"speed_mean\": {:.1}, \"speed_max\": {:.1}, \"freeze_ms_mean\": {:.0}, \"freeze_ms_max\": {:.0} }}",
                of.len(),
                mean(&speed),
                quantile(&speed, 1.0),
                mean(&ms),
                quantile(&ms, 1.0)
            )
        })
        .collect();
    let _ = writeln!(out, "  \"hit_stop\": {{\n{}\n  }}\n}}", rows.join(",\n"));
    out
}

fn stats(values: &[f32]) -> String {
    format!(
        "{{ \"n\": {}, \"mean\": {:.3}, \"p50\": {:.3}, \"p90\": {:.3} }}",
        values.len(),
        mean(values),
        quantile(values, 0.5),
        quantile(values, 0.9)
    )
}
