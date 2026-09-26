//! The 3D view: court, net and ball, placed from the simulation each frame.
//! Players are in `characters`.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use volley_sim::{Ball, Event, Sim};
use volley_sim::court::{self, ATTACK_LINE, BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HALF_WIDTH, NET_HEIGHT, RUNOFF};

use crate::{Match, SimEvent};

pub const TEAM_COLORS: [Color; 2] = [Color::srgb(0.9, 0.3, 0.3), Color::srgb(0.3, 0.5, 0.95)];

pub fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(Color::srgb(0.07, 0.08, 0.11)))
        .add_systems(Startup, spawn_scene)
        .add_systems(Update, ((start_bounce, place_ball).chain(), draw_ball_guides));
}

#[derive(Component)]
struct BallView;

fn spawn_scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Nearly overhead, so shadows land close to what casts them.
    commands.spawn((
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(2.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let mut flat_box = |size: Vec3, at: Vec3, color: Color| {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(at),
        ));
    };

    // Free zone, court and lines, each a little above the last.
    // Arena floor, wide enough that bouncing balls stay on it.
    let free_zone = Vec3::new(2.0 * (HALF_LENGTH + RUNOFF + 15.0), 0.02, 2.0 * (HALF_WIDTH + RUNOFF + 15.0));
    flat_box(free_zone, Vec3::new(0.0, -0.03, 0.0), Color::srgb(0.18, 0.36, 0.5));
    flat_box(Vec3::new(2.0 * HALF_LENGTH, 0.02, 2.0 * HALF_WIDTH), Vec3::new(0.0, -0.01, 0.0), Color::srgb(0.85, 0.52, 0.28));
    let line = 0.08;
    let white = Color::WHITE;
    for z in [-HALF_WIDTH, HALF_WIDTH] {
        flat_box(Vec3::new(2.0 * HALF_LENGTH + line, 0.01, line), Vec3::new(0.0, 0.005, z), white);
    }
    for x in [-HALF_LENGTH, -ATTACK_LINE, 0.0, ATTACK_LINE, HALF_LENGTH] {
        flat_box(Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(x, 0.005, 0.0), white);
    }

    // Net: a translucent mesh with a white top band, between two posts.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.02, 1.0, 2.0 * NET_HALF_WIDTH))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.05, 0.05, 0.05, 0.55),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(0.0, NET_HEIGHT - 0.5, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.04, 0.07, 2.0 * NET_HALF_WIDTH))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_xyz(0.0, NET_HEIGHT - 0.035, 0.0),
    ));
    let post_height = NET_HEIGHT + 0.1;
    let post = meshes.add(Cylinder::new(0.06, post_height));
    let post_material = materials.add(Color::srgb(0.75, 0.75, 0.8));
    for z in [-NET_HALF_WIDTH - 0.1, NET_HALF_WIDTH + 0.1] {
        commands.spawn((
            Mesh3d(post.clone()),
            MeshMaterial3d(post_material.clone()),
            Transform::from_xyz(0.0, post_height / 2.0, z),
        ));
    }

    commands.spawn((
        BallView,
        Mesh3d(meshes.add(Sphere::new(BALL_RADIUS))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.92, 0.45))),
        Transform::default(),
    ));
}

/// How far between the previous and current tick to draw, or `None` right
/// after a reset, when blending would slide things across the court.
fn blend(game: &Match, time: &Time<Fixed>) -> Option<f32> {
    (game.previous.rally == game.current.rally).then(|| time.overstep_fraction())
}

/// Where to draw a player's feet this frame.
pub fn player_feet(game: &Match, time: &Time<Fixed>, player: usize) -> Vec3 {
    let position = |sim: &Sim| sim.players[player].position;
    match blend(game, time) {
        Some(alpha) => position(&game.previous).lerp(position(&game.current), alpha),
        None => position(&game.current),
    }
}

/// A landed ball bouncing and rolling to a stop. Only for show: the rally
/// ended when it first touched the floor.
#[derive(Resource)]
struct Bounce {
    rally: u32,
    position: Vec3,
    velocity: Vec3,
}

/// Fraction of vertical speed a bounce keeps.
const BOUNCE_RESTITUTION: f32 = 0.6;
/// Fraction of horizontal speed a bounce keeps.
const BOUNCE_GRIP: f32 = 0.55;
/// How quickly a rolling ball slows, per second.
const ROLLING_FRICTION: f32 = 1.5;

fn start_bounce(mut commands: Commands, game: Res<Match>, mut events: MessageReader<SimEvent>) {
    for SimEvent(event) in events.read() {
        if let Event::Landed { at, velocity, .. } = *event {
            let velocity = bounced(velocity);
            commands.insert_resource(Bounce { rally: game.current.rally, position: at, velocity });
        }
    }
}

fn bounced(velocity: Vec3) -> Vec3 {
    Vec3::new(velocity.x * BOUNCE_GRIP, -velocity.y * BOUNCE_RESTITUTION, velocity.z * BOUNCE_GRIP)
}

fn place_ball(
    game: Res<Match>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    bounce: Option<ResMut<Bounce>>,
    mut ball: Single<&mut Transform, With<BallView>>,
) {
    if let Some(mut bounce) = bounce
        && bounce.rally == game.current.rally
        && matches!(game.current.ball, Ball::Dead { .. })
    {
        let dt = time.delta_secs();
        bounce.velocity.y -= court::BALL_GRAVITY * dt;
        let step = bounce.velocity * dt;
        bounce.position += step;
        if bounce.position.y < BALL_RADIUS {
            bounce.position.y = BALL_RADIUS;
            bounce.velocity = if bounce.velocity.y < -0.5 { bounced(bounce.velocity) } else { bounce.velocity.with_y(0.0) };
        }
        if bounce.velocity.y == 0.0 {
            let slow = (1.0 - ROLLING_FRICTION * dt).max(0.0);
            bounce.velocity.x *= slow;
            bounce.velocity.z *= slow;
        }
        ball.translation = bounce.position;
        return;
    }
    ball.translation = match blend(&game, &fixed) {
        Some(alpha) => game.previous.ball_position().lerp(game.current.ball_position(), alpha),
        None => game.current.ball_position(),
    };
}

/// A ring under the ball and a marker where it will land: the depth cues that
/// make a ball in 3D readable.
fn draw_ball_guides(game: Res<Match>, ball: Single<&Transform, With<BallView>>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let below = ball.translation.with_y(0.02);
    gizmos.circle(Isometry3d::new(below, flat), BALL_RADIUS, Color::srgba(0.0, 0.0, 0.0, 0.8));
    if let Some(landing) = game.current.landing_point() {
        let at = landing.with_y(0.02);
        gizmos.circle(Isometry3d::new(at, flat), 0.5, Color::WHITE);
        gizmos.circle(Isometry3d::new(at, flat), 0.25, Color::WHITE);
    }
}
