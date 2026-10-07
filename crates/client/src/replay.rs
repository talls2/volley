//! Instant replays: a point won with a big moment (a chain spike, a
//! posterizer, a stuff block, a kicked kill) plays again a beat after it's
//! scored, slowed down, from a camera beside whoever made it, the way
//! television replays a highlight. Any button skips it.
//!
//! Every simulation step is kept for a few seconds (the state after it and
//! its events). A replay feeds those back in as if the match were playing
//! them, so characters, the ball, trails, sounds and hit-stops all play again;
//! the live match waits, its pause between points simply lasting longer.

use std::collections::VecDeque;

use bevy::prelude::*;
use volley_sim::{Ball, Event, HitKind, Phase, Sim, TICK_HZ};

use crate::feel::BaseSpeed;
use crate::flow::Screen;
use crate::{Match, SimEvent};

pub fn plugin(app: &mut App) {
    app.init_resource::<Replay>()
        .add_systems(Update, (start, skip, show_banner).run_if(in_state(Screen::Playing)))
        .add_systems(PostUpdate, film.before(bevy::transform::TransformSystems::Propagate));
}

/// How much of the match is kept (ticks).
const KEEP_TICKS: usize = 4 * TICK_HZ as usize;
/// A replay starts this long (ticks) before the big moment and runs this long
/// past the point, which must follow the moment within `DECISIVE` ticks.
const REPLAY_BEFORE: u32 = 70;
const REPLAY_AFTER: u32 = 30;
const DECISIVE: u32 = 120;
/// A plain attack is replayed if it wins the point leaving this fast (m/s),
/// and no more often than every this many points; the special moments always.
const KILL_SPEED: f32 = 20.0;
const KILL_EVERY: u32 = 3;
/// How fast it plays, and how long after the point (real seconds) it starts.
const REPLAY_SPEED: f32 = 0.45;
const REPLAY_DELAY: f32 = 0.9;
/// The replay camera: how far from the player it films (m), how high, how
/// high it looks, and how quickly it follows (per second).
const CAMERA_DISTANCE: f32 = 5.0;
const CAMERA_HEIGHT: f32 = 1.9;
const CAMERA_LOOK: f32 = 1.6;
const CAMERA_FOLLOW: f32 = 4.0;
/// The bars across the top and bottom of the screen (fraction of its height).
const LETTERBOX: f32 = 0.13;

#[derive(Resource, Default)]
pub struct Replay {
    /// The latest steps: the state after each, and what happened in it.
    history: VecDeque<(Sim, Vec<Event>)>,
    /// Who made this rally's big moment, if it had one, and when (tick);
    /// whether it was special, not just a hard attack.
    star: Option<(usize, u32, bool)>,
    /// Points since the last replay.
    since: u32,
    /// A point worth replaying: when it was scored (tick), its star and
    /// their moment, and how long ago (real seconds).
    pending: Option<(u32, (usize, u32), f32)>,
    playing: Option<Playing>,
}

struct Playing {
    frames: Vec<(Sim, Vec<Event>)>,
    at: usize,
    /// Who made the moment, and when (tick).
    star: (usize, u32),
    /// The match as it was, to go back to.
    live: (Sim, Sim),
    /// How fast the game ran before.
    speed: f32,
}

impl Replay {
    pub fn playing(&self) -> bool {
        self.playing.is_some()
    }

    /// How much slower than usual the game runs for the replay.
    pub fn slowdown(&self) -> f32 {
        if self.playing() { REPLAY_SPEED } else { 1.0 }
    }

    /// Keeps a step, and notes big moments and points worth replaying.
    pub fn record(&mut self, sim: &Sim, events: &[Event]) {
        for event in events {
            // A special moment noted this tick, which its own touch keeps.
            let special = self.star.is_some_and(|(_, tick, special)| special && tick == sim.tick);
            match *event {
                Event::Chained { player } | Event::Posterized { player } => self.star = Some((player, sim.tick, true)),
                Event::Blocked { player, stuffed: true } => self.star = Some((player, sim.tick, true)),
                Event::Touched { player, kind: HitKind::Bicycle | HitKind::Volley, .. } => self.star = Some((player, sim.tick, true)),
                // Any other hard attack, unless it's the special one just noted.
                Event::Touched { player, kind: HitKind::Spike, .. } if !special => {
                    let fast = match sim.ball {
                        Ball::InFlight(flight) => flight.velocity.length() > KILL_SPEED,
                        _ => false,
                    };
                    self.star = fast.then_some((player, sim.tick, false));
                }
                // Any other touch moves the rally on.
                Event::Touched { .. } => {
                    if self.star.is_some_and(|(_, tick, _)| tick != sim.tick) {
                        self.star = None;
                    }
                }
                // A rally won by its big moment, for the side that made it:
                // not the last point of the match, which has its own ending.
                Event::Point { team, .. } => {
                    self.since += 1;
                    if let Some((player, moment, special)) = self.star.take()
                        && sim.players[player].team == team
                        && sim.tick - moment <= DECISIVE
                        && (special || self.since >= KILL_EVERY)
                        && matches!(sim.phase, Phase::PointScored { .. })
                    {
                        self.since = 0;
                        self.pending = Some((sim.tick, (player, moment), 0.0));
                    }
                }
                _ => {}
            }
        }
        self.history.push_back((sim.clone(), events.to_vec()));
        while self.history.len() > KEEP_TICKS {
            self.history.pop_front();
        }
    }

    /// Plays the next step of the replay into the match, if one is playing.
    /// Returns whether it did (and the live match should wait).
    pub fn step(&mut self, game: &mut Match, events: &mut MessageWriter<SimEvent>, clock: &mut Time<Virtual>, base: &mut BaseSpeed) -> bool {
        let Some(playing) = self.playing.as_mut() else { return false };
        if let Some((sim, happened)) = playing.frames.get(playing.at) {
            // The first step is a cut back in time, not movement.
            game.previous.clone_from(if playing.at == 0 { sim } else { &game.current });
            game.current.clone_from(sim);
            // The point and what follows it already happened once.
            for event in happened {
                if !matches!(event, Event::Point { .. } | Event::SetWon { .. } | Event::MatchWon { .. } | Event::SidesSwitched) {
                    events.write(SimEvent(*event));
                }
            }
            playing.at += 1;
            return true;
        }
        self.finish(game, clock, base);
        true
    }

    fn finish(&mut self, game: &mut Match, clock: &mut Time<Virtual>, base: &mut BaseSpeed) {
        if let Some(playing) = self.playing.take() {
            (game.previous, game.current) = playing.live;
            base.0 = playing.speed;
            clock.set_relative_speed(playing.speed);
        }
    }
}

/// Starts a pending replay once the point has sunk in.
fn start(
    time: Res<Time<Real>>,
    mut replay: ResMut<Replay>,
    game: Res<Match>,
    mut clock: ResMut<Time<Virtual>>,
    mut base: ResMut<BaseSpeed>,
    bench: Option<Res<crate::bench::Bench>>,
) {
    let Some((tick, star, waited)) = replay.pending else { return };
    // The bench measures a plain match, unless it's filming replays.
    if bench.is_some_and(|bench| !bench.replays) {
        replay.pending = None;
        return;
    }
    let waited = waited + time.delta_secs();
    replay.pending = Some((tick, star, waited));
    if waited < REPLAY_DELAY {
        return;
    }
    replay.pending = None;
    let from = star.1.saturating_sub(REPLAY_BEFORE);
    let frames: Vec<_> = replay.history.iter().filter(|(sim, _)| sim.tick >= from && sim.tick <= tick + REPLAY_AFTER).cloned().collect();
    // Only within one rally: a replay doesn't start in the last one.
    let rally = frames.last().map(|(sim, _)| sim.rally);
    let frames: Vec<_> = frames.into_iter().filter(|(sim, _)| Some(sim.rally) == rally).collect();
    if frames.len() < 30 {
        return;
    }
    let speed = base.0;
    base.0 = speed * REPLAY_SPEED;
    clock.set_relative_speed(base.0);
    debug!("replay: {} ticks up to the point at tick {tick}, filming player {} from tick {}", frames.len(), star.0, star.1);
    replay.playing = Some(Playing { frames, at: 0, star, live: (game.previous.clone(), game.current.clone()), speed });
}

/// Any key or button ends a replay.
fn skip(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut replay: ResMut<Replay>,
    mut game: ResMut<Match>,
    mut clock: ResMut<Time<Virtual>>,
    mut base: ResMut<BaseSpeed>,
) {
    if !replay.playing() {
        return;
    }
    let pressed = keys.get_just_pressed().next().is_some()
        || mouse.get_just_pressed().next().is_some()
        || gamepads.iter().any(|pad| pad.get_just_pressed().next().is_some());
    if pressed {
        replay.finish(&mut game, &mut clock, &mut base);
    }
}

/// Films the replay from beside the player who made it, side on to the line
/// from them to the ball, steady at a height that frames a jump; once
/// they've made their moment, it stays put and turns to follow the ball.
fn film(
    replay: Res<Replay>,
    game: Res<Match>,
    time: Res<Time<Real>>,
    mut aim: Local<Option<(Vec3, Vec3)>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let Some(playing) = &replay.playing else {
        *aim = None;
        return;
    };
    let sim = &game.current;
    let (star, moment) = playing.star;
    let body = sim.players[star].position;
    let ball = sim.ball_position();
    if sim.tick > moment
        && let Some((eye_now, look_now)) = aim.as_mut()
    {
        let follow = 1.0 - (-CAMERA_FOLLOW * time.delta_secs()).exp();
        *look_now = look_now.lerp(ball, follow);
        **camera = Transform::from_translation(*eye_now).looking_at(*look_now, Vec3::Y);
        return;
    }
    let toward = Vec2::new(ball.x - body.x, ball.z - body.z).normalize_or(Vec2::X);
    let side = Vec3::new(-toward.y, 0.0, toward.x);
    // Keep the court's middle behind the player where it can.
    let side = if side.dot(-body.with_y(0.0)) > 0.0 { -side } else { side };
    let look = Vec3::new(body.x, CAMERA_LOOK, body.z);
    let eye = look + side * CAMERA_DISTANCE + Vec3::Y * (CAMERA_HEIGHT - CAMERA_LOOK);
    let (eye_now, look_now) = aim.get_or_insert((eye, look));
    let follow = 1.0 - (-CAMERA_FOLLOW * time.delta_secs()).exp();
    *eye_now = eye_now.lerp(eye, follow);
    *look_now = look_now.lerp(look, follow);
    **camera = Transform::from_translation(*eye_now).looking_at(*look_now, Vec3::Y);
}

#[derive(Component)]
struct Banner;

/// The recording light beside "Replay", which blinks.
#[derive(Component)]
struct Tag;

const RED: Color = Color::srgb(1.0, 0.2, 0.2);

/// Letterbox bars and a "Replay" tag while a replay plays.
fn show_banner(mut commands: Commands, replay: Res<Replay>, banner: Query<Entity, With<Banner>>, time: Res<Time<Real>>, mut tags: Query<&mut BackgroundColor, With<Tag>>) {
    match (replay.playing(), banner.is_empty()) {
        (true, true) => {
            let bar = |top: bool| {
                (
                    Banner,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(LETTERBOX * 100.0),
                        top: if top { Val::Px(0.0) } else { Val::Auto },
                        bottom: if top { Val::Auto } else { Val::Px(0.0) },
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                )
            };
            commands.spawn(bar(true));
            commands.spawn(bar(false)).with_children(|bar| {
                bar.spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(28.0),
                    top: Val::Percent(30.0),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|tag| {
                    tag.spawn((
                        Tag,
                        Node { width: Val::Px(14.0), height: Val::Px(14.0), border_radius: BorderRadius::MAX, ..default() },
                        BackgroundColor(RED),
                    ));
                    tag.spawn((
                        Text::new("REPLAY"),
                        TextFont { font_size: FontSize::Px(26.0), ..default() },
                        TextColor(Color::WHITE),
                    ));
                });
            });
        }
        (false, false) => {
            for entity in &banner {
                commands.entity(entity).despawn();
            }
        }
        _ => {}
    }
    // The dot blinks.
    let on = (time.elapsed_secs() * 2.0).fract() < 0.6;
    for mut color in &mut tags {
        color.0 = color.0.with_alpha(if on { 1.0 } else { 0.35 });
    }
}
