//! Third-person camera behind the local player. The mouse (or right stick) turns
//! it, and movement is relative to where it faces.

use std::f32::consts::PI;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use volley_sim::{Ball, court};

use crate::Match;
use crate::feel::Shake;
use crate::input::{self, LOCAL_TEAM};
use crate::scene::player_feet;

const DISTANCE: f32 = 7.0;
/// The camera looks at this point above the feet, so the player sits low in the frame.
const LOOK_HEIGHT: f32 = 2.6;
/// How much of a jump the camera follows; less keeps the view steady.
const JUMP_FOLLOW: f32 = 0.35;
const MOUSE_SENSITIVITY: f32 = 0.003;
/// Radians per second at full stick. Up and down is slower because it sets how
/// far hits go.
const STICK_YAW_SPEED: f32 = 3.0;
const STICK_PITCH_SPEED: f32 = 1.0;
/// Below zero the camera drops under the look point, to look up at high balls.
const MIN_PITCH: f32 = -0.35;
const MAX_PITCH: f32 = 1.2;
/// At full shake: how far the camera moves (m) and tilts (radians).
const SHAKE_OFFSET: f32 = 0.25;
const SHAKE_ROLL: f32 = 0.03;

pub fn plugin(app: &mut App) {
    // Red starts on the negative-x half.
    app.insert_resource(CameraRig::facing_net(-1.0))
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, (face_net_each_rally, grab_cursor, turn, follow).chain());
}

#[derive(Resource)]
pub struct CameraRig {
    /// Horizontal angle; 0 faces +x.
    pub yaw: f32,
    /// Angle of the camera above the look point.
    pub pitch: f32,
}

impl CameraRig {
    /// Facing the net from the half at `side`.
    fn facing_net(side: f32) -> Self {
        // Tilted so the aim starts about mid-way into the other court from home.
        Self { yaw: if side < 0.0 { 0.0 } else { PI }, pitch: 0.2 }
    }

    /// Horizontal facing as world (x, z).
    fn forward(&self) -> Vec2 {
        Vec2::new(self.yaw.cos(), self.yaw.sin())
    }

    /// Turns camera-relative input (x = right, y = forward) into world (x, z).
    pub fn to_world(&self, local: Vec2) -> Vec2 {
        let forward = self.forward();
        let right = Vec2::new(-forward.y, forward.x);
        right * local.x + forward * local.y
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // Positional sound is heard from here.
        SpatialListener::new(0.3),
        // Sea haze: the far ocean fades into the sky.
        DistanceFog {
            color: Color::srgb(0.75, 0.86, 0.96),
            falloff: FogFalloff::Linear { start: 60.0, end: 260.0 },
            ..default()
        },
    ));
}

/// Every rally starts facing the net. When serving, the camera also tilts so the
/// aim starts mid-way into the other court: the aim line passes through the
/// look point, so it meets the floor `LOOK_HEIGHT / tan(pitch)` beyond the player.
fn face_net_each_rally(game: Res<Match>, mut rig: ResMut<CameraRig>, mut rally: Local<u32>) {
    if *rally == game.current.rally {
        return;
    }
    *rally = game.current.rally;
    let side = game.current.side(LOCAL_TEAM);
    *rig = CameraRig::facing_net(side);
    let local = game.current.player_index(LOCAL_TEAM, 0);
    if game.current.ball == (Ball::Held { by: local }) {
        let target_x = -side * court::HALF_LENGTH * 0.5;
        let distance = (target_x - game.current.players[local].position.x).abs();
        rig.pitch = LOOK_HEIGHT.atan2(distance);
    }
}

/// Click to capture the mouse for looking around; Esc gives it back.
fn grab_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn turn(
    mut rig: ResMut<CameraRig>,
    mouse: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time>,
) {
    let mut delta = Vec2::ZERO;
    if cursor.grab_mode != CursorGrabMode::None {
        delta += mouse.delta * MOUSE_SENSITIVITY;
    }
    for pad in &gamepads {
        let tilt = input::stick(pad.right_stick());
        // Squaring the tilt keeps small movements slow, for fine aiming.
        let curved = tilt * tilt.length();
        delta += Vec2::new(curved.x * STICK_YAW_SPEED, -curved.y * STICK_PITCH_SPEED) * time.delta_secs();
    }
    rig.yaw = (rig.yaw + delta.x).rem_euclid(2.0 * PI);
    rig.pitch = (rig.pitch + delta.y).clamp(MIN_PITCH, MAX_PITCH);
}

fn follow(
    rig: Res<CameraRig>,
    game: Res<Match>,
    time: Res<Time<Fixed>>,
    real: Res<Time<Real>>,
    shake: Res<Shake>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let local = game.current.player_index(LOCAL_TEAM, 0);
    let feet = player_feet(&game, &time, local);
    let look_at = Vec3::new(feet.x, feet.y * JUMP_FOLLOW + LOOK_HEIGHT, feet.z);
    let forward = rig.forward();
    let back = -Vec3::new(forward.x, 0.0, forward.y) * rig.pitch.cos();
    let position = look_at + (back + Vec3::Y * rig.pitch.sin()) * DISTANCE;
    let mut transform = Transform::from_translation(position).looking_at(look_at, Vec3::Y);

    // Shake: smooth wobble from mixed sine waves, on real time so it keeps
    // moving through a hit-stop.
    let strength = shake.0 * shake.0;
    if strength > 0.0 {
        let t = real.elapsed_secs();
        let wobble = |a: f32, b: f32| (t * a).sin() * 0.6 + (t * b).sin() * 0.4;
        let offset = transform.rotation * Vec3::new(wobble(37.0, 53.0), wobble(41.0, 29.0), 0.0) * SHAKE_OFFSET;
        transform.translation += offset * strength;
        transform.rotate_local_z(wobble(23.0, 47.0) * SHAKE_ROLL * strength);
    }
    **camera = transform;
}
