//! Score, point announcements, hero names over the players, the local hero's
//! ability and ultimate, and a controls reminder.

use bevy::prelude::*;
use volley_sim::court::TEAM_NAMES;
use volley_sim::moves::Button;
use volley_sim::{Ball, DT, Event, HitKind, MoveId, Phase, PointReason, Sim};

use crate::heroes;
use crate::input::{ActiveDevice, LOCAL_TEAM, LocalDriver};
use crate::scene::TEAM_COLORS;
use crate::{Match, SimEvent};

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud)
        .add_systems(Update, (update_score, announce_points, update_controls_help, prompt_saves, show_abilities))
        // Once the camera has moved for the frame.
        .add_systems(PostUpdate, place_name_tags.after(bevy::transform::TransformSystems::Propagate));
}

/// Keeps text readable over bright sand and sky.
const SHADOW: TextShadow = TextShadow { offset: Vec2::new(2.0, 2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.7) };

#[derive(Component)]
struct ScoreText;

#[derive(Component)]
struct SetText;

#[derive(Component)]
struct MovePrompt;

#[derive(Component)]
struct Announcement;

#[derive(Component)]
struct ControlsHelp;

#[derive(Component)]
struct AbilityPanel;

/// The hero name floating over player `0`.
#[derive(Component)]
struct NameTag(usize);

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            ..default()
        },
        children![
            (ScoreText, Text::default(), TextFont { font_size: FontSize::Px(44.0), ..default() }, SHADOW),
            (SetText, Text::default(), TextFont { font_size: FontSize::Px(18.0), ..default() }, SHADOW),
            (Announcement, Text::default(), TextFont { font_size: FontSize::Px(22.0), ..default() }, SHADOW),
        ],
    ));
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            bottom: Val::Percent(26.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            MovePrompt,
            Text::default(),
            TextFont { font_size: FontSize::Px(32.0), ..default() },
            TextColor(Color::srgb(1.0, 0.55, 0.85)),
            SHADOW,
        )],
    ));
    commands.spawn((
        AbilityPanel,
        Text::default(),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        TextLayout::default().with_justify(Justify::Right),
        SHADOW,
        Node { position_type: PositionType::Absolute, right: Val::Px(18.0), bottom: Val::Px(14.0), ..default() },
    ));
    for player in 0..4 {
        commands.spawn((
            NameTag(player),
            Text::default(),
            TextFont { font_size: FontSize::Px(15.0), ..default() },
            SHADOW,
            Node { position_type: PositionType::Absolute, ..default() },
        ));
    }
    commands.spawn((
        ControlsHelp,
        Text::default(),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(0.95, 0.95, 0.97)),
        SHADOW,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(12.0),
            ..default()
        },
    ));
}

fn update_score(
    game: Res<Match>,
    mut score: Single<&mut Text, (With<ScoreText>, Without<SetText>)>,
    mut set: Single<&mut Text, (With<SetText>, Without<ScoreText>)>,
) {
    let sim = &game.current;
    let [red, blue] = sim.score;
    score.0 = format!("{}  {red} : {blue}  {}", TEAM_NAMES[0], TEAM_NAMES[1]);
    let [red_sets, blue_sets] = sim.sets;
    set.0 = format!("Set {} (to {})  |  sets {red_sets} : {blue_sets}", sim.set, sim.points_to_win_set());
}

/// How long a mid-rally callout, like a block, stays up.
const CALLOUT_SECONDS: f32 = 1.2;

/// Points stay up until the next rally starts; blocks get a short callout.
fn announce_points(
    game: Res<Match>,
    time: Res<Time>,
    mut events: MessageReader<SimEvent>,
    mut announcement: Single<(&mut Text, &mut TextColor), With<Announcement>>,
    mut callout_until: Local<Option<f32>>,
) {
    let (text, color) = &mut *announcement;
    for SimEvent(event) in events.read() {
        match *event {
            Event::Point { team, reason } => {
                let why = match reason {
                    PointReason::LandedIn => "ball landed in",
                    PointReason::LandedOut => "ball out",
                    PointReason::TooManyTouches => "four touches",
                    PointReason::DoubleTouch => "double touch",
                };
                text.0 = format!("Point {}: {why}", TEAM_NAMES[team]);
                color.0 = TEAM_COLORS[team];
                *callout_until = None;
            }
            Event::SidesSwitched => text.0.push_str("  |  Switch sides"),
            Event::SetWon { team } => {
                let [red, blue] = game.current.sets;
                text.0 = format!("{} wins set {}!", TEAM_NAMES[team], red + blue);
                color.0 = TEAM_COLORS[team];
                *callout_until = None;
            }
            // Your attacks: how you hit it, and how cleanly.
            Event::Touched { player, kind, quality } if kind.is_attack() && player == game.current.player_index(LOCAL_TEAM, 0) => {
                let how = match kind {
                    HitKind::Dunk => "Slam dunk",
                    HitKind::Volley => "Volley kick",
                    HitKind::Bicycle => "Bicycle kick",
                    _ => "Spike",
                };
                let contact = match quality {
                    q if q >= 0.9 => "perfect!",
                    q if q >= 0.6 => "good",
                    q if q >= 0.3 => "off balance",
                    _ => "scrambled",
                };
                text.0 = format!("{how}: {contact}");
                color.0 = Color::srgb(1.0, 0.85 * quality + 0.15, 0.3);
                *callout_until = Some(time.elapsed_secs() + CALLOUT_SECONDS);
            }
            Event::Dribbled { player } | Event::Carried { player } | Event::Posterized { player } => {
                text.0 = match *event {
                    Event::Dribbled { .. } => "Dribble!",
                    Event::Carried { .. } => "Crossover!",
                    _ => "POSTERIZED!",
                }
                .to_string();
                // A knockdown is the other team's highlight.
                let team = game.current.players[player].team;
                let team = if matches!(event, Event::Posterized { .. }) { 1 - team } else { team };
                color.0 = TEAM_COLORS[team];
                *callout_until = Some(time.elapsed_secs() + CALLOUT_SECONDS);
            }
            Event::Blocked { player, stuffed } => {
                let team = game.current.players[player].team;
                text.0 = if stuffed { "Stuff block!" } else { "Block touch" }.to_string();
                color.0 = TEAM_COLORS[team];
                *callout_until = Some(time.elapsed_secs() + CALLOUT_SECONDS);
            }
            _ => {}
        }
    }
    let expired = match *callout_until {
        Some(until) => time.elapsed_secs() > until,
        None => game.current.phase == Phase::Rally,
    };
    if expired {
        text.0.clear();
    }
}

fn update_controls_help(
    driver: Res<LocalDriver>,
    device: Res<ActiveDevice>,
    mut help: Single<&mut Text, With<ControlsHelp>>,
) {
    if !driver.is_changed() && !device.is_changed() {
        return;
    }
    let you = TEAM_NAMES[LOCAL_TEAM];
    help.0 = match (*driver, *device) {
        (LocalDriver::Human, ActiveDevice::Keyboard) => format!(
            "You are {you}. Click to look with the mouse, Esc to release it. Hits go where you look:\n\
             the yellow ring shows where (red = out). Look higher to hit farther.\n\
             WASD move | C dash | Space jump (hold for full height; run in to jump higher) | Q pass / serve (at the net: block)\n\
             E attack (in the air: spike, volley or bicycle kick) | Shift dive | F foot save | R ability | G ultimate | P pause | 1 let a bot play",
        ),
        (LocalDriver::Human, ActiveDevice::Gamepad) => format!(
            "You are {you}. Right stick looks and aims: hits go where you look, the yellow ring shows\n\
             where (red = out). Look higher to hit farther.\n\
             Left stick move (click to dash) | A jump (hold for full height; run in to jump higher) | RB pass / serve (at the net: block)\n\
             RT attack (in the air: spike, volley or bicycle kick) | LT dive | LB foot save | X ability | Y ultimate | Menu pause | View let a bot play",
        ),
        (LocalDriver::Bot, ActiveDevice::Keyboard) => format!("A bot is playing {you}. Press 1 to take over."),
        (LocalDriver::Bot, ActiveDevice::Gamepad) => format!("A bot is playing {you}. Press View to take over."),
    };
}

/// How far ahead the save prompt looks.
const PROMPT_LOOKAHEAD_TICKS: u32 = 30;

/// Prompts a foot save when the ball is about to come by too low for your arms
/// but within a foot, and a dive when it will land too far to run to.
fn prompt_saves(
    game: Res<Match>,
    driver: Res<LocalDriver>,
    device: Res<ActiveDevice>,
    mut prompt: Single<&mut Text, With<MovePrompt>>,
) {
    prompt.0 = save_prompt(&game.current, *driver, *device).unwrap_or_default();
}

fn save_prompt(sim: &Sim, driver: LocalDriver, device: ActiveDevice) -> Option<String> {
    let me = sim.player_index(LOCAL_TEAM, 0);
    let player = &sim.players[me];
    let Ball::InFlight(flight) = sim.ball else { return None };
    let busy = player.action.is_some_and(|action| action.id.spec().lunge.is_some());
    if driver != LocalDriver::Human || sim.must_not_touch(me) || busy || !player.grounded() {
        return None;
    }
    let (foot_key, dive_key) = match device {
        ActiveDevice::Keyboard => ("F", "Shift"),
        ActiveDevice::Gamepad => ("LB", "LT"),
    };
    let upcoming = || (1..=PROMPT_LOOKAHEAD_TICKS).map(|ahead| flight.position_at(sim.tick + ahead));
    let arms = upcoming().any(|ball| player.reaches(MoveId::Pass, ball));
    let feet = player.kit.has_move(MoveId::FootSave) && upcoming().any(|ball| player.reaches(MoveId::FootSave, ball));
    if feet && !arms {
        return Some(format!("Foot save!  [{foot_key}]"));
    }
    // Landing on our side, too far to run to in time but within a dive.
    let landing = flight.landing_point();
    let seconds_left = flight.landing_time() - flight.elapsed(sim.tick);
    let distance = Vec2::new(landing.x - player.position.x, landing.z - player.position.z).length();
    let dive = MoveId::Dive.spec();
    let dive_reach = dive.lunge.unwrap_or_default() * dive.active as f32 * DT + dive.reach;
    let ours = sim.team_on(landing.x) == LOCAL_TEAM;
    (ours && seconds_left < 0.6 && distance > player.kit.run_speed * seconds_left + 1.0 && distance < dive_reach)
        .then(|| format!("Dive!  [{dive_key}]"))
}

/// The local hero's ability and ultimate: ready, cooling down, or charging.
fn show_abilities(
    game: Res<Match>,
    driver: Res<LocalDriver>,
    device: Res<ActiveDevice>,
    mut panel: Single<(&mut Text, &mut TextColor), With<AbilityPanel>>,
) {
    let (text, color) = &mut *panel;
    let sim = &game.current;
    let me = &sim.players[sim.player_index(LOCAL_TEAM, 0)];
    let (ability_key, ultimate_key) = heroes::buttons(*device);
    let mut lines = Vec::new();
    let mut ultimate_ready = false;
    for (button, key) in [(Button::Ability, ability_key), (Button::Ultimate, ultimate_key)] {
        let Some(id) = me.kit.move_on(button) else { continue };
        let spec = id.spec();
        let state = if spec.ultimate {
            ultimate_ready = me.charge >= 1.0;
            if ultimate_ready { "READY".to_string() } else { format!("{:.0}%", me.charge * 100.0) }
        } else if me.ready(id, sim.tick) {
            "ready".to_string()
        } else {
            format!("{:.1}s", me.cooldown_left(id, sim.tick))
        };
        lines.push(format!("{} {key}  {state}", spec.name));
    }
    text.0 = if *driver == LocalDriver::Human { lines.join("\n") } else { String::new() };
    color.0 = if ultimate_ready { Color::srgb(1.0, 0.8, 0.2) } else { Color::WHITE };
}

/// Hero names float over everyone, in team colors.
fn place_name_tags(
    game: Res<Match>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    fixed: Res<Time<Fixed>>,
    mut tags: Query<(&NameTag, &mut Node, &mut Text, &mut TextColor, &mut Visibility)>,
) {
    let (camera, camera_transform) = *camera;
    let sim = &game.current;
    for (tag, mut node, mut text, mut color, mut visibility) in &mut tags {
        let Some(player) = sim.players.get(tag.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let head = crate::scene::player_feet(&game, &fixed, tag.0) + Vec3::Y * 2.15;
        let Ok(at) = camera.world_to_viewport(camera_transform, head) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        text.0 = player.kit.name.to_string();
        color.0 = TEAM_COLORS[player.team];
        // Centered over the head; names are short, so an estimate of the width will do.
        node.left = Val::Px(at.x - 4.0 * text.0.len() as f32);
        node.top = Val::Px(at.y - 10.0);
    }
}
