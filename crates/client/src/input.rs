//! Keyboard, mouse and gamepad → `PlayerInput` for the local player. Everyone
//! else is a bot.

use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use volley_sim::{PlayerInput, Sim, bot};

use crate::aim;
use crate::camera::CameraRig;

/// The local player is this team's first player.
pub const LOCAL_TEAM: usize = 0;

pub fn plugin(app: &mut App) {
    app.init_resource::<Presses>()
        .init_resource::<LocalDriver>()
        .init_resource::<ActiveDevice>()
        .add_systems(
            RunFixedMainLoop,
            record_presses.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        )
        .add_systems(Update, (toggle_driver, track_active_device));
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocalDriver {
    #[default]
    Human,
    /// A bot plays for you, so you can watch.
    Bot,
}

/// What the player last used, so help text can show the right buttons.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveDevice {
    #[default]
    Keyboard,
    Gamepad,
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
    pub kick: KeyCode,
    pub dash: KeyCode,
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
    kick: KeyCode::KeyF,
    dash: KeyCode::KeyC,
    toggle_bot: KeyCode::Digit1,
};

pub struct Buttons {
    pub jump: &'static [GamepadButton],
    pub pass: &'static [GamepadButton],
    pub spike: &'static [GamepadButton],
    pub dive: &'static [GamepadButton],
    pub kick: &'static [GamepadButton],
    pub dash: &'static [GamepadButton],
    pub toggle_bot: GamepadButton,
}

/// Hits are on the triggers and bumpers, pressed with index fingers, so the
/// right thumb can stay on the stick and keep aiming. Face buttons do the same
/// for anyone who prefers them.
pub const BUTTONS: Buttons = Buttons {
    jump: &[GamepadButton::South],
    // RB, X
    pass: &[GamepadButton::RightTrigger, GamepadButton::West],
    // RT, Y
    spike: &[GamepadButton::RightTrigger2, GamepadButton::North],
    // LT, B
    dive: &[GamepadButton::LeftTrigger2, GamepadButton::East],
    // LB
    kick: &[GamepadButton::LeftTrigger],
    // Left stick click: the thumb is already there, moving.
    dash: &[GamepadButton::LeftThumb],
    // View / Back
    toggle_bot: GamepadButton::Select,
};

/// Stick tilt below this is ignored, so a resting or worn stick doesn't drift.
const STICK_DEADZONE: f32 = 0.2;

/// A stick's tilt with the deadzone removed, rescaled so it still reaches 1.
pub fn stick(raw: Vec2) -> Vec2 {
    let tilt = raw.length();
    if tilt < STICK_DEADZONE {
        return Vec2::ZERO;
    }
    raw / tilt * ((tilt - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0)
}

/// Button presses seen since the last simulation tick.
///
/// Input updates once per frame but the simulation ticks at its own fixed rate,
/// so a frame can run zero ticks (a press would be lost) or two (a press would
/// count twice). Collecting presses here hands each one to exactly one tick.
#[derive(Resource, Default)]
struct Presses {
    jump: bool,
    jump_released: bool,
    pass: bool,
    spike: bool,
    dive: bool,
    kick: bool,
    dash: bool,
}

/// Every connected gamepad controls the local player. The Mac can report extra
/// devices as gamepads, and listening to all of them means the real one always works.
fn record_presses(keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut presses: ResMut<Presses>) {
    let pressed = |key, buttons: &[GamepadButton]| {
        keys.just_pressed(key) || gamepads.iter().any(|pad| pad.any_just_pressed(buttons.iter().copied()))
    };
    presses.jump |= pressed(KEYS.jump, BUTTONS.jump);
    presses.jump_released |= keys.just_released(KEYS.jump)
        || gamepads.iter().any(|pad| pad.any_just_released(BUTTONS.jump.iter().copied()));
    presses.pass |= pressed(KEYS.pass, BUTTONS.pass);
    presses.spike |= pressed(KEYS.spike, BUTTONS.spike);
    presses.dive |= pressed(KEYS.dive, BUTTONS.dive);
    presses.kick |= pressed(KEYS.kick, BUTTONS.kick);
    presses.dash |= pressed(KEYS.dash, BUTTONS.dash);
}

fn toggle_driver(keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut driver: ResMut<LocalDriver>) {
    if keys.just_pressed(KEYS.toggle_bot) || gamepads.iter().any(|pad| pad.just_pressed(BUTTONS.toggle_bot)) {
        *driver = if *driver == LocalDriver::Human { LocalDriver::Bot } else { LocalDriver::Human };
    }
}

fn track_active_device(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    mut device: ResMut<ActiveDevice>,
) {
    let pad_used = gamepads.iter().any(|pad| {
        pad.get_just_pressed().next().is_some()
            || stick(pad.left_stick()) != Vec2::ZERO
            || stick(pad.right_stick()) != Vec2::ZERO
    });
    if pad_used {
        device.set_if_neq(ActiveDevice::Gamepad);
    } else if keys.get_just_pressed().next().is_some() || mouse.delta != Vec2::ZERO {
        device.set_if_neq(ActiveDevice::Keyboard);
    }
}

#[derive(SystemParam)]
pub struct Controls<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
    presses: ResMut<'w, Presses>,
    driver: Res<'w, LocalDriver>,
    camera: Res<'w, CameraRig>,
    camera_transform: Query<'w, 's, &'static Transform, With<Camera3d>>,
}

impl Controls<'_, '_> {
    /// This tick's input for every player. Consumes the recorded presses.
    pub fn take_inputs(&mut self, sim: &Sim) -> Vec<PlayerInput> {
        let mut inputs: Vec<_> = (0..sim.players.len()).map(|i| bot::input_for(sim, i)).collect();
        let presses = std::mem::take(&mut *self.presses);
        if *self.driver == LocalDriver::Human {
            inputs[sim.player_index(LOCAL_TEAM, 0)] = PlayerInput {
                movement: self.camera.to_world(self.movement()),
                aim: self.camera_transform.single().ok().map(aim::floor_point),
                jump: presses.jump,
                jump_released: presses.jump_released,
                pass: presses.pass,
                spike: presses.spike,
                dive: presses.dive,
                kick: presses.kick,
                dash: presses.dash,
            };
        }
        inputs
    }

    /// Camera-relative movement: x = right, y = forward.
    fn movement(&self) -> Vec2 {
        let axis = |negative, positive| {
            f32::from(u8::from(self.keys.pressed(positive))) - f32::from(u8::from(self.keys.pressed(negative)))
        };
        let keys = Vec2::new(axis(KEYS.left, KEYS.right), axis(KEYS.back, KEYS.forward));
        let sticks: Vec2 = self.gamepads.iter().map(|pad| stick(pad.left_stick())).sum();
        (keys + sticks).clamp_length_max(1.0)
    }
}
