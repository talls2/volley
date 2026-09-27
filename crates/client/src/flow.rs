//! Match flow: the title screen, hero select, pausing, and the match-over
//! screen with a rematch.

use bevy::prelude::*;
use volley_sim::Phase;
use volley_sim::court::TEAM_NAMES;

use crate::Match;
use crate::scene::TEAM_COLORS;

pub fn plugin(app: &mut App) {
    app.init_state::<Screen>()
        .add_systems(Startup, spawn_overlay)
        .add_systems(Update, (start_or_rematch, toggle_pause, notice_match_over, update_overlay));
}

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Screen {
    #[default]
    Title,
    /// Picking a hero before each match; see `select.rs`.
    HeroSelect,
    Playing,
    MatchOver,
}

const START_KEYS: [KeyCode; 2] = [KeyCode::Enter, KeyCode::Space];
const START_BUTTONS: [GamepadButton; 2] = [GamepadButton::South, GamepadButton::Start];
const PAUSE_KEY: KeyCode = KeyCode::KeyP;
const PAUSE_BUTTON: GamepadButton = GamepadButton::Start;

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct Headline;

#[derive(Component)]
struct Subline;

fn spawn_overlay(mut commands: Commands) {
    let shadow = TextShadow { offset: Vec2::new(3.0, 3.0), color: Color::srgba(0.0, 0.0, 0.0, 0.75) };
    commands.spawn((
        Overlay,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(14.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.05, 0.1, 0.35)),
        children![
            (Headline, Text::default(), TextFont { font_size: FontSize::Px(84.0), ..default() }, shadow),
            (Subline, Text::default(), TextFont { font_size: FontSize::Px(26.0), ..default() }, TextLayout::default().with_justify(Justify::Center), shadow),
        ],
    ));
}

fn pressed(keys: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>, key_list: &[KeyCode], buttons: &[GamepadButton]) -> bool {
    keys.any_just_pressed(key_list.iter().copied())
        || gamepads.iter().any(|pad| pad.any_just_pressed(buttons.iter().copied()))
}

/// From the title or a finished match, on to picking heroes.
fn start_or_rematch(
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    if matches!(screen.get(), Screen::Title | Screen::MatchOver) && pressed(&keys, &gamepads, &START_KEYS, &START_BUTTONS) {
        next.set(Screen::HeroSelect);
    }
}

fn toggle_pause(
    screen: Res<State<Screen>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut clock: ResMut<Time<Virtual>>,
) {
    if *screen.get() != Screen::Playing || !pressed(&keys, &gamepads, &[PAUSE_KEY], &[PAUSE_BUTTON]) {
        return;
    }
    if clock.is_paused() {
        clock.unpause();
    } else {
        clock.pause();
    }
}

fn notice_match_over(game: Res<Match>, screen: Res<State<Screen>>, mut next: ResMut<NextState<Screen>>) {
    if *screen.get() == Screen::Playing && matches!(game.current.phase, Phase::MatchOver { .. }) {
        next.set(Screen::MatchOver);
    }
}

fn update_overlay(
    screen: Res<State<Screen>>,
    clock: Res<Time<Virtual>>,
    game: Res<Match>,
    mut overlay: Single<&mut Visibility, With<Overlay>>,
    mut headline: Single<(&mut Text, &mut TextColor), (With<Headline>, Without<Subline>)>,
    mut subline: Single<&mut Text, (With<Subline>, Without<Headline>)>,
) {
    let (headline_text, headline_color) = &mut *headline;
    let (title, color, detail) = match *screen.get() {
        Screen::Title => (
            "VOLLEY".to_string(),
            Color::WHITE,
            "Arena beach volley, 3v3: sets to 21, best of 3\nPress Enter or A to play".to_string(),
        ),
        Screen::Playing if clock.is_paused() => ("Paused".to_string(), Color::WHITE, "Press P or Menu to resume".to_string()),
        Screen::Playing | Screen::HeroSelect => {
            **overlay = Visibility::Hidden;
            return;
        }
        Screen::MatchOver => {
            let Phase::MatchOver { winner } = game.current.phase else { return };
            let [red, blue] = game.current.sets;
            (
                format!("{} wins!", TEAM_NAMES[winner]),
                TEAM_COLORS[winner],
                format!("Sets {red} : {blue}\nPress Enter or A for a rematch"),
            )
        }
    };
    **overlay = Visibility::Visible;
    headline_text.0 = title;
    headline_color.0 = color;
    subline.0 = detail;
}
