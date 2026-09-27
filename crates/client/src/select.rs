//! Hero select, before every match: a card per hero, left and right to choose,
//! confirm to play.

use bevy::prelude::*;
use volley_sim::moves::HEROES;

use crate::Match;
use crate::flow::Screen;
use crate::heroes::{self, INFO};
use crate::input::{ActiveDevice, stick};

pub fn plugin(app: &mut App) {
    app.init_resource::<HeroChoice>()
        .add_systems(Startup, spawn_select)
        .add_systems(Update, (choose.run_if(in_state(Screen::HeroSelect)), show_select));
}

/// The hero you picked, by index into `HEROES`.
#[derive(Resource)]
pub struct HeroChoice(pub usize);

impl Default for HeroChoice {
    fn default() -> Self {
        // The newest hero, so it gets tried.
        Self(HEROES.len() - 1)
    }
}

const LEFT_KEYS: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];
const RIGHT_KEYS: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];
const CONFIRM_KEYS: [KeyCode; 2] = [KeyCode::Enter, KeyCode::Space];
const CONFIRM_BUTTONS: [GamepadButton; 2] = [GamepadButton::South, GamepadButton::Start];

#[derive(Component)]
struct SelectRoot;

#[derive(Component)]
struct Card(usize);

#[derive(Component)]
struct CardLines(usize);

#[derive(Component)]
struct Hint;

fn spawn_select(mut commands: Commands) {
    let shadow = TextShadow { offset: Vec2::new(2.0, 2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.7) };
    let root = commands
        .spawn((
            SelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.05, 0.1, 0.45)),
            Visibility::Hidden,
        ))
        .id();
    commands.spawn((Text::new("Choose your hero"), TextFont { font_size: FontSize::Px(56.0), ..default() }, shadow, ChildOf(root)));
    let row = commands
        .spawn((
            Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(24.0), align_items: AlignItems::Stretch, ..default() },
            ChildOf(root),
        ))
        .id();
    for (index, (kit, info)) in HEROES.iter().zip(&INFO).enumerate() {
        let card = commands
            .spawn((
                Card(index),
                Node {
                    width: Val::Px(400.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(14.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.08, 0.12, 0.85)),
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                ChildOf(row),
            ))
            .id();
        commands.spawn((
            Text::new(kit.name),
            TextFont { font_size: FontSize::Px(40.0), ..default() },
            TextColor(info.color),
            shadow,
            ChildOf(card),
        ));
        commands.spawn((
            Text::new(info.title),
            TextFont { font_size: FontSize::Px(20.0), ..default() },
            TextColor(Color::srgb(0.8, 0.85, 0.9)),
            ChildOf(card),
        ));
        commands.spawn((Text::new(info.blurb), TextFont { font_size: FontSize::Px(16.0), ..default() }, ChildOf(card)));
        commands.spawn((
            CardLines(index),
            Text::default(),
            TextFont { font_size: FontSize::Px(15.0), ..default() },
            TextColor(Color::srgb(0.9, 0.9, 0.8)),
            ChildOf(card),
        ));
    }
    commands.spawn((Hint, Text::default(), TextFont { font_size: FontSize::Px(22.0), ..default() }, shadow, ChildOf(root)));
}

fn show_select(
    screen: Res<State<Screen>>,
    choice: Res<HeroChoice>,
    device: Res<ActiveDevice>,
    mut root: Single<&mut Visibility, With<SelectRoot>>,
    mut cards: Query<(&Card, &mut BorderColor, &mut BackgroundColor)>,
    mut lines: Query<(&CardLines, &mut Text), Without<Hint>>,
    mut hint: Single<&mut Text, With<Hint>>,
) {
    let showing = *screen.get() == Screen::HeroSelect;
    **root = if showing { Visibility::Visible } else { Visibility::Hidden };
    if !showing {
        return;
    }
    for (card, mut border, mut background) in &mut cards {
        let chosen = card.0 == choice.0;
        *border = BorderColor::all(if chosen { INFO[card.0].color } else { Color::srgba(1.0, 1.0, 1.0, 0.2) });
        background.0 = if chosen { Color::srgba(0.1, 0.14, 0.2, 0.95) } else { Color::srgba(0.05, 0.08, 0.12, 0.7) };
    }
    for (card, mut text) in &mut lines {
        text.0 = heroes::lines(&INFO[card.0], *device).iter().map(|line| format!("- {line}")).collect::<Vec<_>>().join("\n");
    }
    hint.0 = match *device {
        ActiveDevice::Keyboard => "A / D to choose, Enter to play".to_string(),
        ActiveDevice::Gamepad => "Left stick or d-pad to choose, A to play".to_string(),
    };
}

fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut choice: ResMut<HeroChoice>,
    mut game: ResMut<Match>,
    mut next: ResMut<NextState<Screen>>,
    mut stick_held: Local<bool>,
) {
    let tilt = gamepads.iter().map(|pad| stick(pad.left_stick()).x).sum::<f32>();
    let flicked = if tilt.abs() > 0.6 && !*stick_held { tilt.signum() as i32 } else { 0 };
    *stick_held = tilt.abs() > 0.3;
    let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
    let step = if keys.any_just_pressed(LEFT_KEYS) || pad(GamepadButton::DPadLeft) {
        -1
    } else if keys.any_just_pressed(RIGHT_KEYS) || pad(GamepadButton::DPadRight) {
        1
    } else {
        flicked
    };
    choice.0 = (choice.0 as i32 + step).rem_euclid(HEROES.len() as i32) as usize;

    if keys.any_just_pressed(CONFIRM_KEYS) || gamepads.iter().any(|pad| pad.any_just_pressed(CONFIRM_BUTTONS)) {
        let sim = heroes::new_match(choice.0);
        *game = Match { previous: sim.clone(), current: sim };
        next.set(Screen::Playing);
    }
}
