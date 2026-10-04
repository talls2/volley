//! The ball, its shadow and where it will land, placed from the simulation
//! each frame. The arena around the court is in `arena`; players are in
//! `characters`.

use std::f32::consts::FRAC_PI_2;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use volley_sim::court::{self, BALL_RADIUS};
use volley_sim::{Ball, Event, Flight, Sim};

use crate::{Match, SimEvent};

pub const TEAM_COLORS: [Color; 2] = [Color::srgb(0.9, 0.3, 0.3), Color::srgb(0.3, 0.5, 0.95)];

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_ball)
        .add_systems(Update, ((start_bounce, place_ball, place_decoy, place_shadows).chain(), draw_ball_guides));
}

#[derive(Component)]
pub struct BallView;

/// The balls' material, which an arena may make glow.
#[derive(Resource)]
pub struct BallLook(pub Handle<StandardMaterial>);

/// A decoy ball from a split hit (Golazo's Chilena): drawn exactly like the
/// real one, shadow and landing rings too, so nothing gives away which is which.
#[derive(Component)]
struct DecoyView;

/// A soft dark spot on the floor right under a ball, so its height reads at a
/// glance: big and dark when it's low, small and faint when it's high. The
/// flag says whether it's the decoy's.
#[derive(Component)]
struct BallShadow(bool);

const SHADOW_RADIUS: f32 = 0.35;
/// The shadow is smallest and faintest with the ball this high.
const SHADOW_FADE_HEIGHT: f32 = 8.0;
/// A landing ring starts this big and closes to the landing spot as the ball
/// comes down, over this many seconds.
const LANDING_RING: f32 = 2.5;
const LANDING_WARNING: f32 = 1.5;

fn spawn_ball(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let shadow_mesh = meshes.add(Circle::new(SHADOW_RADIUS));
    for decoy in [false, true] {
        commands.spawn((
            BallShadow(decoy),
            Mesh3d(shadow_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.45),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            NotShadowCaster,
        ));
    }
    let ball_mesh = meshes.add(Sphere::new(BALL_RADIUS));
    let ball_material = materials.add(Color::srgb(1.0, 0.92, 0.45));
    commands.insert_resource(BallLook(ball_material.clone()));
    commands.spawn((BallView, Mesh3d(ball_mesh.clone()), MeshMaterial3d(ball_material.clone()), Transform::default()));
    commands.spawn((DecoyView, Mesh3d(ball_mesh), MeshMaterial3d(ball_material), Transform::default(), Visibility::Hidden));
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
/// Places the decoy ball while there is one, blending between ticks like the real one.
fn place_decoy(game: Res<Match>, fixed: Res<Time<Fixed>>, decoy: Single<(&mut Transform, &mut Visibility), With<DecoyView>>) {
    let (mut transform, mut visibility) = decoy.into_inner();
    let Some(flight) = game.current.decoy() else {
        *visibility = Visibility::Hidden;
        return;
    };
    *visibility = Visibility::Visible;
    let now = flight.position_at(game.current.tick);
    let before = flight.position_at(game.current.tick.saturating_sub(1).max(flight.start_tick));
    transform.translation = before.lerp(now, fixed.overstep_fraction());
}

fn place_shadows(
    balls: Query<(&Transform, &Visibility, Has<DecoyView>), (Or<(With<BallView>, With<DecoyView>)>, Without<BallShadow>)>,
    mut shadows: Query<(&BallShadow, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), Without<BallView>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (ball, ball_visibility, decoy) in &balls {
        let Some((_, mut transform, mut visibility, material)) = shadows.iter_mut().find(|(shadow, ..)| shadow.0 == decoy) else {
            continue;
        };
        *visibility = if *ball_visibility == Visibility::Hidden { Visibility::Hidden } else { Visibility::Visible };
        let height = (ball.translation.y / SHADOW_FADE_HEIGHT).clamp(0.0, 1.0);
        transform.translation = ball.translation.with_y(0.015);
        transform.scale = Vec3::splat(1.0 - 0.6 * height);
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base_color = Color::srgba(0.0, 0.0, 0.0, 0.5 - 0.35 * height);
        }
    }
}

/// Where the ball will come down, in the color of the team whose side it's
/// falling on: a target, and a ring closing in on it as the ball drops.
fn draw_ball_guides(game: Res<Match>, mut gizmos: Gizmos) {
    let Ball::InFlight(flight) = game.current.ball else { return };
    for flight in std::iter::once(flight).chain(game.current.decoy()) {
        draw_landing(&game.current, flight, &mut gizmos);
    }
}

fn draw_landing(sim: &Sim, flight: Flight, gizmos: &mut Gizmos) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let landing = flight.landing_point();
    let at = landing.with_y(0.02);
    let color = TEAM_COLORS[sim.team_on(landing.x)].lighter(0.2);
    gizmos.circle(Isometry3d::new(at, flat), 0.5, color);
    gizmos.circle(Isometry3d::new(at, flat), 0.25, color);
    let left = flight.landing_time() - flight.elapsed(sim.tick);
    let closing = (left / LANDING_WARNING).clamp(0.0, 1.0);
    gizmos.circle(Isometry3d::new(at, flat), 0.5 + (LANDING_RING - 0.5) * closing, color.with_alpha(0.7));
}
