//! Keyboard and gamepad → `PlayerInput` for the local player. Everyone else is a bot.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use volley_sim::{PlayerInput, Sim, bot};

use crate::camera::CameraRig;

/// The local player is this team's first player.
pub const LOCAL_TEAM: usize = 0;

pub fn plugin(app: &mut App) {
    app.init_resource::<Presses>()
        .init_resource::<LocalDriver>()
        .add_systems(
            RunFixedMainLoop,
            record_presses.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        )
        .add_systems(Update, toggle_driver);
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocalDriver {
    #[default]
    Human,
    /// A bot plays for you, so you can watch.
    Bot,
}

pub struct Keys {
    pub forward: KeyCode,
    pub back: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub jump: KeyCode,
    pub pass: KeyCode,
    pub spike: KeyCode,
    pub dive: KeyCode,
    pub toggle_bot: KeyCode,
}

pub const KEYS: Keys = Keys {
    forward: KeyCode::KeyW,
    back: KeyCode::KeyS,
    left: KeyCode::KeyA,
    right: KeyCode::KeyD,
    jump: KeyCode::Space,
    pass: KeyCode::KeyQ,
    spike: KeyCode::KeyE,
    dive: KeyCode::ShiftLeft,
    toggle_bot: KeyCode::Digit1,
};

const JUMP_BUTTON: GamepadButton = GamepadButton::South;
const PASS_BUTTON: GamepadButton = GamepadButton::West;
const SPIKE_BUTTON: GamepadButton = GamepadButton::East;
const DIVE_BUTTON: GamepadButton = GamepadButton::North;

/// Button presses seen since the last simulation tick.
///
/// Input updates once per frame but the simulation ticks at its own fixed rate,
/// so a frame can run zero ticks (a press would be lost) or two (a press would
/// count twice). Collecting presses here hands each one to exactly one tick.
#[derive(Resource, Default)]
struct Presses {
    jump: bool,
    pass: bool,
    spike: bool,
    dive: bool,
}

fn record_presses(keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut presses: ResMut<Presses>) {
    let pad = gamepads.iter().next();
    let pressed = |key, button| keys.just_pressed(key) || pad.is_some_and(|pad| pad.just_pressed(button));
    presses.jump |= pressed(KEYS.jump, JUMP_BUTTON);
    presses.pass |= pressed(KEYS.pass, PASS_BUTTON);
    presses.spike |= pressed(KEYS.spike, SPIKE_BUTTON);
    presses.dive |= pressed(KEYS.dive, DIVE_BUTTON);
}

fn toggle_driver(keys: Res<ButtonInput<KeyCode>>, mut driver: ResMut<LocalDriver>) {
    if keys.just_pressed(KEYS.toggle_bot) {
        *driver = if *driver == LocalDriver::Human { LocalDriver::Bot } else { LocalDriver::Human };
    }
}

#[derive(SystemParam)]
pub struct Controls<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
    presses: ResMut<'w, Presses>,
    driver: Res<'w, LocalDriver>,
    camera: Res<'w, CameraRig>,
}

impl Controls<'_, '_> {
    /// This tick's input for every player. Consumes the recorded presses.
    pub fn take_inputs(&mut self, sim: &Sim) -> Vec<PlayerInput> {
        let mut inputs: Vec<_> = (0..sim.players.len()).map(|i| bot::input_for(sim, i)).collect();
        let presses = std::mem::take(&mut *self.presses);
        if *self.driver == LocalDriver::Human {
            inputs[sim.player_index(LOCAL_TEAM, 0)] = PlayerInput {
                movement: self.camera.to_world(self.movement()),
                jump: presses.jump,
                pass: presses.pass,
                spike: presses.spike,
                dive: presses.dive,
            };
        }
        inputs
    }

    /// Camera-relative movement: x = right, y = forward.
    fn movement(&self) -> Vec2 {
        let axis = |negative, positive| {
            f32::from(u8::from(self.keys.pressed(positive))) - f32::from(u8::from(self.keys.pressed(negative)))
        };
        let mut movement = Vec2::new(axis(KEYS.left, KEYS.right), axis(KEYS.back, KEYS.forward));
        if let Some(pad) = self.gamepads.iter().next() {
            let stick = pad.left_stick();
            if stick.length() > 0.2 {
                movement += stick;
            }
        }
        movement.clamp_length_max(1.0)
    }
}
