//! Score, point announcements and a controls reminder.

use bevy::prelude::*;
use volley_sim::court::TEAM_NAMES;
use volley_sim::{Event, Phase, PointReason};

use crate::input::{LOCAL_TEAM, LocalDriver};
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

fn announce_points(
    game: Res<Match>,
    mut events: MessageReader<SimEvent>,
    mut announcement: Single<(&mut Text, &mut TextColor), With<Announcement>>,
) {
    let (text, color) = &mut *announcement;
    for SimEvent(event) in events.read() {
        if let Event::Point { team, reason } = *event {
            let why = match reason {
                PointReason::LandedIn => "ball landed in",
                PointReason::LandedOut => "ball out",
                PointReason::TooManyTouches => "four touches",
                PointReason::DoubleTouch => "double touch",
            };
            text.0 = format!("Point {}: {why}", TEAM_NAMES[team]);
            color.0 = TEAM_COLORS[team];
        }
    }
    if game.current.phase == Phase::Rally {
        text.0.clear();
    }
}

fn update_controls_help(driver: Res<LocalDriver>, mut help: Single<&mut Text, With<ControlsHelp>>) {
    if !driver.is_changed() {
        return;
    }
    let you = TEAM_NAMES[LOCAL_TEAM];
    help.0 = match *driver {
        LocalDriver::Human => format!(
            "You are {you}. Click to look with the mouse, Esc to release it.\n\
             WASD move | Space jump | Q pass / serve | E spike (in the air) | Shift dive | 1 let a bot play",
        ),
        LocalDriver::Bot => format!("A bot is playing {you}. Press 1 to take over."),
    };
}
