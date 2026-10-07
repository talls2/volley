//! The ball, its shadow and where it will land, placed from the simulation
//! each frame, and the players' contact shadows. The arena around the court
//! is in `arena`; players are in `characters`.

use std::f32::consts::FRAC_PI_2;

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use volley_sim::court::{self, BALL_RADIUS};
use volley_sim::{Ball, Event, Flight, Sim};

use crate::characters::PlayerBody;
use crate::{Match, SimEvent};

pub const TEAM_COLORS: [Color; 2] = [Color::srgb(0.9, 0.3, 0.3), Color::srgb(0.3, 0.5, 0.95)];

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, (spawn_ball, spawn_player_shadows))
        .add_systems(Update, ((start_bounce, place_ball, place_decoy, shape_balls, place_shadows).chain(), draw_ball_guides, place_player_shadows));
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
/// A soft dark patch on the floor under each player, where the light from
/// above is blocked: what makes a body look like it stands on the floor
/// rather than over it. It shrinks and fades as the player jumps.
#[derive(Component)]
struct PlayerShadow(usize);

/// The patch's size (m across), darkness, and the height (m) a jump takes
/// it to its smallest and faintest.
const PLAYER_SHADOW_SIZE: f32 = 1.3;
const PLAYER_SHADOW_ALPHA: f32 = 0.6;
const PLAYER_SHADOW_FADE: f32 = 2.5;

/// A landing ring starts this big and closes to the landing spot as the ball
/// comes down, over this many seconds.
const LANDING_RING: f32 = 2.5;
const LANDING_WARNING: f32 = 1.5;

fn spawn_ball(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
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
    let ball_mesh = meshes.add(Sphere::new(BALL_RADIUS).mesh().uv(32, 18));
    let ball_material = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(ball_texture())),
        perceptual_roughness: 0.55,
        ..default()
    });
    commands.insert_resource(BallLook(ball_material.clone()));
    // Each ball: placed, and stretched along its flight, by the parent; the
    // panels spin in the child.
    let skin = |commands: &mut Commands, parent: Entity| {
        commands.spawn((BallSkin, Mesh3d(ball_mesh.clone()), MeshMaterial3d(ball_material.clone()), Transform::default(), ChildOf(parent)));
    };
    let ball = commands.spawn((BallView, BallMotion::default(), Transform::default(), Visibility::Visible)).id();
    skin(&mut commands, ball);
    let decoy = commands.spawn((DecoyView, BallMotion::default(), Transform::default(), Visibility::Hidden)).id();
    skin(&mut commands, decoy);
}

/// A ball's mesh, spinning inside the ball that places and stretches it.
#[derive(Component)]
struct BallSkin;

/// How a ball has been moving, for its spin, stretch and squash.
#[derive(Component)]
struct BallMotion {
    last: Vec3,
    spin: Quat,
    /// A hit's squash, from 0 (none) toward 1, fading fast.
    squash: f32,
}

impl Default for BallMotion {
    fn default() -> Self {
        Self { last: Vec3::ZERO, spin: Quat::IDENTITY, squash: 0.0 }
    }
}

/// Stretch along the flight per m/s, at most `MAX_STRETCH`; a hit's squash and
/// how fast it springs back; the fastest the panels visibly spin (rad/s).
const STRETCH_PER_SPEED: f32 = 0.012;
const MAX_STRETCH: f32 = 0.3;
const HIT_SQUASH: f32 = 0.35;
const SQUASH_RECOVERY: f32 = 18.0;
const MAX_SPIN: f32 = 28.0;

/// A volleyball's panels, for the sphere's (u, v): three-panel sections in
/// yellow, blue and white, curving, with thin dark seams, so spin shows.
fn ball_texture() -> Image {
    const W: usize = 256;
    const H: usize = 128;
    let colors = [[255u8, 214, 64], [38, 92, 214], [242, 242, 236]];
    let mut data = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        let v = y as f32 / H as f32;
        for x in 0..W {
            let u = x as f32 / W as f32;
            let band = u * 6.0 + 0.35 * (v * std::f32::consts::TAU).sin();
            let fraction = band - band.floor();
            let seam = fraction < 0.03 || fraction > 0.97;
            let [r, g, b] = if seam { [40, 40, 50] } else { colors[(band.floor() as i32).rem_euclid(3) as usize] };
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Image::new(
        Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Spins each ball's panels the way it rolls through the air, stretches it
/// along its flight the faster it goes, and squashes the real one for an
/// instant when it's hit.
fn shape_balls(
    time: Res<Time>,
    mut events: MessageReader<SimEvent>,
    mut balls: Query<(&mut Transform, &mut BallMotion, &Children, Has<BallView>), Or<(With<BallView>, With<DecoyView>)>>,
    mut skins: Query<&mut Transform, (With<BallSkin>, Without<BallMotion>)>,
) {
    let hit = events.read().any(|SimEvent(event)| matches!(event, Event::Touched { .. }));
    let dt = time.delta_secs();
    for (mut transform, mut motion, children, real) in &mut balls {
        let at = transform.translation;
        let moved = at - motion.last;
        motion.last = at;
        if real && hit {
            motion.squash = 1.0;
        }
        motion.squash *= (-SQUASH_RECOVERY * dt).exp();
        let velocity = if dt > 0.0 && moved.length() < 3.0 { moved / dt } else { Vec3::ZERO };
        let speed = velocity.length();
        let along = if speed > 0.5 { velocity / speed } else { Vec3::Z };
        if speed > 0.5 {
            let axis = along.cross(Vec3::Y).normalize_or_zero();
            if axis != Vec3::ZERO {
                let spin = (speed / BALL_RADIUS * 0.15).min(MAX_SPIN);
                motion.spin = (Quat::from_axis_angle(axis, -spin * dt) * motion.spin).normalize();
            }
        }
        let stretch = 1.0 + (speed * STRETCH_PER_SPEED).min(MAX_STRETCH) - HIT_SQUASH * motion.squash;
        let across = 1.0 / stretch.max(0.3).sqrt();
        let facing = Quat::from_rotation_arc(Vec3::Z, along);
        transform.rotation = facing;
        transform.scale = Vec3::new(across, across, stretch);
        // The panels turn in the world, whatever way the stretch points.
        for &child in children {
            if let Ok(mut skin) = skins.get_mut(child) {
                skin.rotation = facing.inverse() * motion.spin;
            }
        }
    }
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

fn spawn_player_shadows(
    mut commands: Commands,
    game: Res<Match>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mesh = meshes.add(Rectangle::new(PLAYER_SHADOW_SIZE, PLAYER_SHADOW_SIZE));
    let blob = images.add(blob_texture());
    for index in 0..game.current.players.len() {
        commands.spawn((
            PlayerShadow(index),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, PLAYER_SHADOW_ALPHA),
                base_color_texture: Some(blob.clone()),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
}

/// White with alpha falling off smoothly from the middle: a soft round spot.
fn blob_texture() -> Image {
    const N: usize = 64;
    let mut data = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / N as f32 * 2.0 - Vec2::ONE;
            let fall = (1.0 - d.length_squared()).max(0.0);
            data.extend_from_slice(&[255, 255, 255, (fall * fall * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn place_player_shadows(
    bodies: Query<(&PlayerBody, &Transform), Without<PlayerShadow>>,
    mut shadows: Query<(&PlayerShadow, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (shadow, mut transform, mut visibility, material) in &mut shadows {
        let Some((_, body)) = bodies.iter().find(|(body, _)| body.0 == shadow.0) else { continue };
        *visibility = Visibility::Visible;
        let height = (body.translation.y / PLAYER_SHADOW_FADE).clamp(0.0, 1.0);
        transform.translation = body.translation.with_y(0.012);
        transform.scale = Vec3::splat(1.0 - 0.45 * height);
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base_color = Color::srgba(0.0, 0.0, 0.0, PLAYER_SHADOW_ALPHA * (1.0 - 0.75 * height));
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
