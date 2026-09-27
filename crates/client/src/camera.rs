//! Third-person camera behind the local player. The mouse (or right stick) turns
//! it, and movement is relative to where it faces. Ball cam, on by default and
//! toggled with Tab or the right stick's click, keeps turning it toward the ball
//! instead; turning the camera yourself switches ball cam off.

use std::f32::consts::{PI, TAU};

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use volley_sim::Ball;

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
/// How quickly ball cam swings around to the ball (per second), and how much a
/// high ball tips the camera down to look up at it.
const BALL_CAM_TURN: f32 = 6.0;
const BALL_CAM_TILT: f32 = 0.6;
/// Ball cam ignores a ball this close (horizontally): right overhead, its
/// direction flips around at the slightest move.
const BALL_CAM_DEAD_ZONE: f32 = 1.5;
const BALL_CAM_KEY: KeyCode = KeyCode::Tab;
const BALL_CAM_BUTTON: GamepadButton = GamepadButton::RightThumb;
/// Turning the camera this much (radians in a frame) takes it off ball cam.
const MANUAL_TURN: f32 = 0.01;

pub fn plugin(app: &mut App) {
    // Red starts on the negative-x half.
    app.insert_resource(CameraRig::facing_net(-1.0))
        .init_resource::<BallCam>()
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, (face_net_each_rally, grab_cursor, toggle_ball_cam, turn, ball_cam, follow).chain());
}

/// Whether the camera keeps turning toward the ball.
#[derive(Resource)]
pub struct BallCam(pub bool);

impl Default for BallCam {
    fn default() -> Self {
        Self(true)
    }
}

fn toggle_ball_cam(keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, mut ball_cam: ResMut<BallCam>) {
    if keys.just_pressed(BALL_CAM_KEY) || gamepads.iter().any(|pad| pad.just_pressed(BALL_CAM_BUTTON)) {
        ball_cam.0 = !ball_cam.0;
    }
}

/// Swings the camera around behind the player, facing the ball, and tilts it
/// to keep a high ball in view.
fn ball_cam(
    ball_cam: Res<BallCam>,
    game: Res<Match>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    views: Query<&Transform, With<crate::scene::BallView>>,
    mut rig: ResMut<CameraRig>,
) {
    if !ball_cam.0 || !matches!(game.current.ball, Ball::InFlight(_) | Ball::Carried { .. }) {
        return;
    }
    let Ok(ball) = views.single().map(|view| view.translation) else { return };
    let feet = player_feet(&game, &fixed, game.current.player_index(LOCAL_TEAM, 0));
    let to_ball = Vec2::new(ball.x - feet.x, ball.z - feet.z);
    if to_ball.length() < BALL_CAM_DEAD_ZONE {
        return;
    }
    let blend = 1.0 - (-BALL_CAM_TURN * time.delta_secs()).exp();
    let turn = (to_ball.y.atan2(to_ball.x) - rig.yaw + PI).rem_euclid(TAU) - PI;
    rig.yaw = (rig.yaw + turn * blend).rem_euclid(TAU);
    // A ball above eye level tips the camera down to look up at it.
    let rise = (ball.y - LOOK_HEIGHT).atan2(to_ball.length());
    let pitch = (0.2 - BALL_CAM_TILT * rise.max(0.0)).clamp(MIN_PITCH, 0.4);
    rig.pitch += (pitch - rig.pitch) * blend;
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
        Self { yaw: if side < 0.0 { 0.0 } else { PI }, pitch: 0.2 }
    }

    /// Horizontal facing as world (x, z).
    pub fn forward(&self) -> Vec2 {
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

/// Each rally starts looking at the net from behind the player.
fn face_net_each_rally(game: Res<Match>, mut rig: ResMut<CameraRig>, mut rally: Local<u32>) {
    if *rally == game.current.rally {
        return;
    }
    *rally = game.current.rally;
    *rig = CameraRig::facing_net(game.current.side(LOCAL_TEAM));
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
    mut ball_cam: ResMut<BallCam>,
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
    if delta.length() > MANUAL_TURN {
        ball_cam.0 = false;
    }
    rig.yaw = (rig.yaw + delta.x).rem_euclid(TAU);
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
