//! Third-person camera behind the local player. The mouse (or right stick) turns
//! it, and movement is relative to where it faces.

use std::f32::consts::PI;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::Match;
use crate::input::LOCAL_TEAM;
use crate::scene::player_feet;

const DISTANCE: f32 = 6.0;
/// The camera looks at this point above the feet, so the player sits low in the frame.
const LOOK_HEIGHT: f32 = 2.6;
/// How much of a jump the camera follows; less keeps the view steady.
const JUMP_FOLLOW: f32 = 0.35;
const MOUSE_SENSITIVITY: f32 = 0.003;
/// Radians per second at full stick.
const STICK_SPEED: f32 = 3.0;
/// Below zero the camera drops under the look point, to look up at high balls.
const MIN_PITCH: f32 = -0.35;
const MAX_PITCH: f32 = 1.2;

pub fn plugin(app: &mut App) {
    app.insert_resource(CameraRig::facing_net(LOCAL_TEAM))
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, (grab_cursor, turn, follow).chain());
}

#[derive(Resource)]
pub struct CameraRig {
    /// Horizontal angle; 0 faces +x.
    pub yaw: f32,
    /// Angle of the camera above the look point.
    pub pitch: f32,
}

impl CameraRig {
    fn facing_net(team: usize) -> Self {
        Self { yaw: if team == 0 { 0.0 } else { PI }, pitch: 0.25 }
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
    commands.spawn(Camera3d::default());
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
    if let Some(pad) = gamepads.iter().next() {
        let stick = pad.right_stick();
        if stick.length() > 0.2 {
            delta += Vec2::new(stick.x, -stick.y) * STICK_SPEED * time.delta_secs();
        }
    }
    rig.yaw = (rig.yaw + delta.x).rem_euclid(2.0 * PI);
    rig.pitch = (rig.pitch + delta.y).clamp(MIN_PITCH, MAX_PITCH);
}

fn follow(
    rig: Res<CameraRig>,
    game: Res<Match>,
    time: Res<Time<Fixed>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let local = game.current.player_index(LOCAL_TEAM, 0);
    let feet = player_feet(&game, &time, local);
    let look_at = Vec3::new(feet.x, feet.y * JUMP_FOLLOW + LOOK_HEIGHT, feet.z);
    let forward = rig.forward();
    let back = -Vec3::new(forward.x, 0.0, forward.y) * rig.pitch.cos();
    let position = look_at + (back + Vec3::Y * rig.pitch.sin()) * DISTANCE;
    **camera = Transform::from_translation(position).looking_at(look_at, Vec3::Y);
}
