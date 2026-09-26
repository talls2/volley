//! Score, point announcements and a controls reminder.

use bevy::prelude::*;
use volley_sim::court::TEAM_NAMES;
use volley_sim::{Event, Phase, PointReason};

use crate::input::{ActiveDevice, LOCAL_TEAM, LocalDriver};
use crate::scene::TEAM_COLORS;
use crate::{Match, SimEvent};

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud)
        .add_systems(Update, (update_score, announce_points, update_controls_help));
}

#[derive(Component)]
struct ScoreText;

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
            (ScoreText, Text::default(), TextFont { font_size: FontSize::Px(44.0), ..default() }),
            (Announcement, Text::default(), TextFont { font_size: FontSize::Px(22.0), ..default() }),
        ],
    ));
    commands.spawn((
        ControlsHelp,
        Text::default(),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(0.8, 0.8, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(12.0),
            ..default()
        },
    ));
}

fn update_score(game: Res<Match>, mut text: Single<&mut Text, With<ScoreText>>) {
    let [red, blue] = game.current.score;
    text.0 = format!("{}  {red} : {blue}  {}", TEAM_NAMES[0], TEAM_NAMES[1]);
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
             WASD move | Space jump | Q pass / serve (at the net: block) | E spike (in the air) | Shift dive | 1 let a bot play",
        ),
        (LocalDriver::Human, ActiveDevice::Gamepad) => format!(
            "You are {you}. Right stick looks and aims: hits go where you look, the yellow ring shows\n\
             where (red = out). Look higher to hit farther.\n\
             Left stick move | A jump | RB pass / serve (at the net: block) | RT spike (in the air) | LT dive | View let a bot play",
        ),
        (LocalDriver::Bot, ActiveDevice::Keyboard) => format!("A bot is playing {you}. Press 1 to take over."),
        (LocalDriver::Bot, ActiveDevice::Gamepad) => format!("A bot is playing {you}. Press View to take over."),
    };
}
