//! Score, point announcements and a controls reminder.

use bevy::prelude::*;
use volley_sim::court::TEAM_NAMES;
use volley_sim::{Ball, DT, Event, MoveId, Phase, PointReason, Sim};

use crate::input::{ActiveDevice, LOCAL_TEAM, LocalDriver};
use crate::scene::TEAM_COLORS;
use crate::{Match, SimEvent};

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud)
        .add_systems(Update, (update_score, announce_points, update_controls_help, prompt_saves));
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
             WASD move | Space jump | Q pass / serve (at the net: block) | E spike (in the air) | Shift dive | F foot save | P pause | 1 let a bot play",
        ),
        (LocalDriver::Human, ActiveDevice::Gamepad) => format!(
            "You are {you}. Right stick looks and aims: hits go where you look, the yellow ring shows\n\
             where (red = out). Look higher to hit farther.\n\
             Left stick move | A jump | RB pass / serve (at the net: block) | RT spike (in the air) | LT dive | LB foot save | Menu pause | View let a bot play",
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
    let feet = upcoming().any(|ball| player.reaches(MoveId::FootSave, ball));
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
