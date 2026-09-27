//! Keyboard, mouse and gamepad → `PlayerInput` for the local player. Everyone
//! else is a bot.

use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use volley_sim::{Aim, Ball, PlayerInput, Sim, bot};

use crate::camera::CameraRig;
use crate::flow::Screen;

/// The local player is this team's first player.
pub const LOCAL_TEAM: usize = 0;

pub fn plugin(app: &mut App) {
    app.init_resource::<Presses>()
        .init_resource::<Charge>()
        .init_resource::<LocalDriver>()
        .init_resource::<ActiveDevice>()
        .add_systems(
            RunFixedMainLoop,
            record_presses.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        )
        .add_systems(Update, (toggle_driver, track_active_device))
        // Presses made on the menus (like the one that started the match) don't count.
        .add_systems(OnEnter(Screen::Playing), |mut presses: ResMut<Presses>, mut charge: ResMut<Charge>| {
            *presses = Presses::default();
            *charge = Charge::default();
        });
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
    pub ability: KeyCode,
    pub ultimate: KeyCode,
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
    ability: KeyCode::KeyR,
    ultimate: KeyCode::KeyG,
    toggle_bot: KeyCode::Digit1,
};

pub struct Buttons {
    pub jump: &'static [GamepadButton],
    pub pass: &'static [GamepadButton],
    pub spike: &'static [GamepadButton],
    pub dive: &'static [GamepadButton],
    pub kick: &'static [GamepadButton],
    pub dash: &'static [GamepadButton],
    pub ability: &'static [GamepadButton],
    pub ultimate: &'static [GamepadButton],
    pub toggle_bot: GamepadButton,
}

/// Hits are on the triggers and bumpers, pressed with index fingers, so the
/// right thumb can stay on the stick and keep aiming. The face buttons are the
/// hero's ability and ultimate.
pub const BUTTONS: Buttons = Buttons {
    jump: &[GamepadButton::South],
    // RB
    pass: &[GamepadButton::RightTrigger],
    // RT
    spike: &[GamepadButton::RightTrigger2],
    // LT, B
    dive: &[GamepadButton::LeftTrigger2, GamepadButton::East],
    // LB
    kick: &[GamepadButton::LeftTrigger],
    // Left stick click: the thumb is already there, moving.
    dash: &[GamepadButton::LeftThumb],
    // X, Y
    ability: &[GamepadButton::West],
    ultimate: &[GamepadButton::North],
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
    /// Pass let go: serves go on release, after charging.
    pass_released: bool,
    spike: bool,
    dive: bool,
    kick: bool,
    dash: bool,
    ability: bool,
    ultimate: bool,
}

/// Holding pass or attack this long gives a hit its full (small) boost.
const CHARGE_SECONDS: f32 = 0.5;

/// Boosting hits: a hit charges while its button is held, and uses whatever
/// charge it has when it meets the ball: a little farther and faster.
#[derive(Resource, Default)]
pub struct Charge {
    /// When pass or attack went down, while held.
    pass_since: Option<f32>,
    spike_since: Option<f32>,
    /// The power when the button was let go, kept while its move waits for the ball.
    released: f32,
    /// The power right now, from 0 to 1: charging, or the last one released.
    pub power: f32,
    pub charging: bool,
}

fn charge_after(seconds: f32) -> f32 {
    (seconds / CHARGE_SECONDS).min(1.0)
}

/// Every connected gamepad controls the local player. The Mac can report extra
/// devices as gamepads, and listening to all of them means the real one always works.
fn record_presses(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    mut presses: ResMut<Presses>,
    mut charge: ResMut<Charge>,
) {
    let pressed = |key, buttons: &[GamepadButton]| {
        keys.just_pressed(key) || gamepads.iter().any(|pad| pad.any_just_pressed(buttons.iter().copied()))
    };
    let released = |key, buttons: &[GamepadButton]| {
        keys.just_released(key) || gamepads.iter().any(|pad| pad.any_just_released(buttons.iter().copied()))
    };
    let now = time.elapsed_secs();
    // Pass and attack charge while held; letting go keeps the power reached.
    let (charge, presses) = (&mut *charge, &mut *presses);
    for (key, buttons, since, press, release) in [
        (KEYS.pass, BUTTONS.pass, &mut charge.pass_since, &mut presses.pass, Some(&mut presses.pass_released)),
        (KEYS.spike, BUTTONS.spike, &mut charge.spike_since, &mut presses.spike, None),
    ] {
        if pressed(key, buttons) {
            *since = Some(now);
            *press = true;
        }
        if released(key, buttons)
            && let Some(start) = since.take()
        {
            charge.released = charge_after(now - start);
            if let Some(release) = release {
                *release = true;
            }
        }
    }
    let held = [charge.pass_since, charge.spike_since].into_iter().flatten().min_by(f32::total_cmp);
    charge.charging = held.is_some();
    charge.power = held.map_or(charge.released, |start| charge_after(now - start));
    presses.jump |= pressed(KEYS.jump, BUTTONS.jump);
    presses.jump_released |= keys.just_released(KEYS.jump)
        || gamepads.iter().any(|pad| pad.any_just_released(BUTTONS.jump.iter().copied()));
    presses.dive |= pressed(KEYS.dive, BUTTONS.dive);
    presses.kick |= pressed(KEYS.kick, BUTTONS.kick);
    presses.dash |= pressed(KEYS.dash, BUTTONS.dash);
    presses.ability |= pressed(KEYS.ability, BUTTONS.ability);
    presses.ultimate |= pressed(KEYS.ultimate, BUTTONS.ultimate);
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
    charge: ResMut<'w, Charge>,
    driver: Res<'w, LocalDriver>,
    camera: Res<'w, CameraRig>,
}

impl Controls<'_, '_> {
    /// This tick's input for every player. Consumes the recorded presses.
    pub fn take_inputs(&mut self, sim: &Sim) -> Vec<PlayerInput> {
        let mut inputs: Vec<_> = (0..sim.players.len()).map(|i| bot::input_for(sim, i)).collect();
        let presses = std::mem::take(&mut *self.presses);
        let me = sim.player_index(LOCAL_TEAM, 0);
        // A released hit keeps its power while its move waits for the ball; after that, a tap's.
        if !self.charge.charging && !presses.pass && !presses.spike && sim.players[me].action.is_none() {
            self.charge.released = 0.0;
            self.charge.power = 0.0;
        }
        // A serve has no ball coming to time: hold to charge it, let go to serve.
        let serving = sim.ball == (Ball::Held { by: me });
        let pass_down = self.keys.pressed(KEYS.pass) || self.gamepads.iter().any(|pad| pad.any_pressed(BUTTONS.pass.iter().copied()));
        if *self.driver == LocalDriver::Human {
            let movement = self.camera.to_world(self.movement());
            // Hits go the way you're moving, or the way you're looking if you aren't.
            let direction = if movement.length() > 0.2 { movement } else { self.camera.forward() };
            inputs[me] = PlayerInput {
                movement,
                aim: Some(Aim::Toward { direction, power: self.charge.power }),
                jump: presses.jump,
                jump_released: presses.jump_released,
                pass: if serving { presses.pass_released } else { presses.pass },
                pass_held: pass_down && !serving,
                spike: presses.spike,
                dive: presses.dive,
                kick: presses.kick,
                dash: presses.dash,
                ability: presses.ability,
                ultimate: presses.ultimate,
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
