//! The 3D view: the beach, the walled arena, the net and the ball, placed from
//! the simulation each frame. Players are in `characters`.

use std::f32::consts::FRAC_PI_2;

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::prelude::*;
use volley_sim::{Ball, Event, Sim};
use volley_sim::court::{self, BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HALF_WIDTH, NET_HEIGHT};

use crate::{Match, SimEvent};

pub const TEAM_COLORS: [Color; 2] = [Color::srgb(0.9, 0.3, 0.3), Color::srgb(0.3, 0.5, 0.95)];

pub fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(SKY))
        .add_systems(Startup, spawn_scene)
        .add_systems(Update, generate_mipmaps.run_if(resource_exists::<NeedsMipmaps>))
        .add_systems(Update, ((start_bounce, place_ball).chain(), draw_ball_guides));
}

#[derive(Component)]
pub struct BallView;

/// The beach: a sand floor, an ocean around it, sky and haze.
const SAND_SIZE: f32 = 160.0;
/// The arena's glass walls: how tall they look (to the ball they go up forever),
/// the padded band along their base, and the frame posts' spacing.
const WALL_HEIGHT: f32 = 8.0;
const PAD_HEIGHT: f32 = 1.0;
const POST_SPACING: f32 = 4.0;
/// Real-world size of one tile of the sand texture.
const SAND_TILE: f32 = 1.5;
const LINE_COLOR: Color = Color::srgb(0.1, 0.35, 0.85);
const SKY: Color = Color::srgb(0.55, 0.78, 0.97);

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    // Warm sun, nearly overhead so shadows land close to what casts them, and
    // bluish sky light filling the shadows.
    commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.95, 0.85), illuminance: 11_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(2.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.8, 1.0), brightness: 600.0, ..default() });

    let repeating = |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            anisotropy_clamp: 16,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let sand_color = assets.load_builder().with_settings(repeating).load("textures/sand_01_diff_2k.jpg");
    let sand_normal = assets.load_builder().with_settings(move |settings: &mut ImageLoaderSettings| {
        repeating(settings);
        settings.is_srgb = false;
    }).load("textures/sand_01_nor_gl_2k.jpg");
    commands.insert_resource(NeedsMipmaps(vec![sand_color.id(), sand_normal.id()]));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(SAND_SIZE, SAND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(sand_color),
            normal_map_texture: Some(sand_normal),
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            uv_transform: Affine2::from_scale(Vec2::splat(SAND_SIZE / SAND_TILE)),
            ..default()
        })),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(800.0, 800.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.33, 0.42),
            perceptual_roughness: 0.2,
            reflectance: 0.6,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.25, 0.0),
    ));

    spawn_walls(&mut commands, &mut meshes, &mut materials);

    // Straps along the walls, and one under the net.
    let line = 0.08;
    let strap = materials.add(LINE_COLOR);
    for (size, at) in [
        (Vec3::new(2.0 * HALF_LENGTH + line, 0.01, line), Vec3::new(0.0, 0.005, -HALF_WIDTH)),
        (Vec3::new(2.0 * HALF_LENGTH + line, 0.01, line), Vec3::new(0.0, 0.005, HALF_WIDTH)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(-HALF_LENGTH, 0.005, 0.0)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(HALF_LENGTH, 0.005, 0.0)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(0.0, 0.005, 0.0)),
    ] {
        commands.spawn((Mesh3d(meshes.add(Cuboid::from_size(size))), MeshMaterial3d(strap.clone()), Transform::from_translation(at)));
    }

    // Net: a translucent mesh with a blue top band, between padded posts.
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
        MeshMaterial3d(materials.add(LINE_COLOR)),
        Transform::from_xyz(0.0, NET_HEIGHT - 0.035, 0.0),
    ));
    let post_height = NET_HEIGHT + 0.1;
    let post = meshes.add(Cylinder::new(0.05, post_height));
    let post_material = materials.add(Color::srgb(0.85, 0.85, 0.88));
    let pad = meshes.add(Cylinder::new(0.12, 1.8));
    let pad_material = materials.add(LINE_COLOR);
    for z in [-NET_HALF_WIDTH - 0.1, NET_HALF_WIDTH + 0.1] {
        commands.spawn((Mesh3d(post.clone()), MeshMaterial3d(post_material.clone()), Transform::from_xyz(0.0, post_height / 2.0, z)));
        commands.spawn((Mesh3d(pad.clone()), MeshMaterial3d(pad_material.clone()), Transform::from_xyz(0.0, 0.9, z)));
    }

    commands.spawn((
        BallView,
        Mesh3d(meshes.add(Sphere::new(BALL_RADIUS))),
        MeshMaterial3d(materials.add(Color::srgb(1.0, 0.92, 0.45))),
        Transform::default(),
    ));
}

/// Glass walls around the arena, padded along the base in each half's team
/// color, with frame posts and a top rail.
fn spawn_walls(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.9, 1.0, 0.12),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.05,
        reflectance: 0.8,
        ..default()
    });
    let frame = materials.add(Color::srgb(0.92, 0.93, 0.95));
    let pads = [materials.add(TEAM_COLORS[0].darker(0.1)), materials.add(TEAM_COLORS[1].darker(0.1))];
    let thickness = 0.1;
    // Each wall as (center, length along it, whether it runs along x).
    let walls = [
        (Vec3::new(0.0, 0.0, -HALF_WIDTH - thickness / 2.0), 2.0 * HALF_LENGTH, true),
        (Vec3::new(0.0, 0.0, HALF_WIDTH + thickness / 2.0), 2.0 * HALF_LENGTH, true),
        (Vec3::new(-HALF_LENGTH - thickness / 2.0, 0.0, 0.0), 2.0 * HALF_WIDTH, false),
        (Vec3::new(HALF_LENGTH + thickness / 2.0, 0.0, 0.0), 2.0 * HALF_WIDTH, false),
    ];
    for (center, length, along_x) in walls {
        let size = |long: f32, tall: f32, thick: f32| if along_x { Vec3::new(long, tall, thick) } else { Vec3::new(thick, tall, long) };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size(length, WALL_HEIGHT, thickness)))),
            MeshMaterial3d(glass.clone()),
            Transform::from_translation(center + Vec3::Y * WALL_HEIGHT / 2.0),
            NotShadowCaster,
        ));
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size(length, 0.12, 0.16)))),
            MeshMaterial3d(frame.clone()),
            Transform::from_translation(center + Vec3::Y * WALL_HEIGHT),
        ));
        // Pads: a side wall spans both halves, so it gets one per half.
        let halves: &[(f32, f32)] = if along_x { &[(-HALF_LENGTH / 2.0, HALF_LENGTH), (HALF_LENGTH / 2.0, HALF_LENGTH)] } else { &[(0.0, 2.0 * HALF_WIDTH)] };
        for &(offset, long) in halves {
            let at = center + if along_x { Vec3::new(offset, 0.0, 0.0) } else { Vec3::ZERO };
            let pad = &pads[usize::from(at.x > 0.0)];
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::from_size(size(long, PAD_HEIGHT, 0.25)))),
                MeshMaterial3d(pad.clone()),
                Transform::from_translation(at + Vec3::Y * PAD_HEIGHT / 2.0),
            ));
        }
        let posts = (length / POST_SPACING).round() as i32;
        for i in 0..=posts {
            let along = -length / 2.0 + length * i as f32 / posts as f32;
            let offset = if along_x { Vec3::new(along, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, along) };
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.12, WALL_HEIGHT, 0.12))),
                MeshMaterial3d(frame.clone()),
                Transform::from_translation(center + offset + Vec3::Y * WALL_HEIGHT / 2.0),
            ));
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

/// Textures that should get mipmaps once loaded.
#[derive(Resource)]
struct NeedsMipmaps(Vec<AssetId<Image>>);

/// JPGs load without mipmaps, the smaller copies a GPU samples when a texture
/// is far away. Without them fine repeating detail like sand shimmers into
/// noise at a distance, so they're built here by averaging 2x2 blocks.
fn generate_mipmaps(mut commands: Commands, mut pending: ResMut<NeedsMipmaps>, mut images: ResMut<Assets<Image>>) {
    pending.0.retain(|&id| {
        let Some(mut image) = images.get_mut(id) else {
            return true;
        };
        let (mut width, mut height) = (image.width() as usize, image.height() as usize);
        let Some(data) = image.data.as_mut() else {
            return false;
        };
        let bytes_per_pixel = data.len() / (width * height);
        let mut level = data.clone();
        let mut levels = 1;
        while width > 1 && height > 1 {
            let (next_width, next_height) = (width / 2, height / 2);
            let mut next = vec![0u8; next_width * next_height * bytes_per_pixel];
            for y in 0..next_height {
                for x in 0..next_width {
                    for c in 0..bytes_per_pixel {
                        let at = |dx: usize, dy: usize| level[((2 * y + dy) * width + 2 * x + dx) * bytes_per_pixel + c] as u32;
                        next[(y * next_width + x) * bytes_per_pixel + c] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4) as u8;
                    }
                }
            }
            data.extend_from_slice(&next);
            (level, width, height) = (next, next_width, next_height);
            levels += 1;
        }
        image.texture_descriptor.mip_level_count = levels;
        false
    });
    if pending.0.is_empty() {
        commands.remove_resource::<NeedsMipmaps>();
    }
}
