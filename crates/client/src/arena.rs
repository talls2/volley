//! The places a match is played. The court is the same everywhere
//! (`volley_sim::court`): only what's around it changes. The beach, a walled
//! court on the sand by the sea; and the Neon Stadium, a night arena of light
//! with a glowing floor, energy walls and a crowd all around, the same on
//! both sides. Picked on the
//! hero select screen; the scene is rebuilt whenever the choice changes.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use volley_sim::court::{HALF_LENGTH, HALF_WIDTH, NET_HALF_WIDTH, NET_HEIGHT};

use crate::scene::{BallLook, TEAM_COLORS};

pub fn plugin(app: &mut App) {
    app.init_resource::<Arena>().add_systems(
        Update,
        (
            build_arena.run_if(resource_changed::<Arena>),
            generate_mipmaps.run_if(resource_exists::<NeedsMipmaps>),
            spin,
        ),
    );
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arena {
    Beach,
    Neon,
}

impl Default for Arena {
    // The newest, so it gets seen.
    fn default() -> Self {
        Arena::Neon
    }
}

impl Arena {
    pub const ALL: [Arena; 2] = [Arena::Beach, Arena::Neon];

    pub fn name(self) -> &'static str {
        match self {
            Arena::Beach => "Beach",
            Arena::Neon => "Neon Stadium",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Arena::Beach => "A glass cage on the sand, in the sun by the sea.",
            Arena::Neon => "A night stadium of light: a glowing floor, energy walls and a roaring crowd.",
        }
    }
}

/// Everything that belongs to the current arena, cleared when it changes.
#[derive(Component)]
struct ArenaPiece;

/// Turns slowly about the vertical, in radians per second.
#[derive(Component)]
struct Spin(f32);

/// The arenas' glass (or energy) walls: how tall they look (to the ball they
/// go up forever), and the spacing of their frame posts.
const WALL_HEIGHT: f32 = 8.0;
const WALL_THICKNESS: f32 = 0.1;
const POST_SPACING: f32 = 4.0;

/// Each wall as (center at the floor, length along it, whether it runs along x).
fn walls() -> [(Vec3, f32, bool); 4] {
    let t = WALL_THICKNESS / 2.0;
    [
        (Vec3::new(0.0, 0.0, -HALF_WIDTH - t), 2.0 * HALF_LENGTH, true),
        (Vec3::new(0.0, 0.0, HALF_WIDTH + t), 2.0 * HALF_LENGTH, true),
        (Vec3::new(-HALF_LENGTH - t, 0.0, 0.0), 2.0 * HALF_WIDTH, false),
        (Vec3::new(HALF_LENGTH + t, 0.0, 0.0), 2.0 * HALF_WIDTH, false),
    ]
}

/// A box's size given along a wall, up, and across it.
fn along(along_x: bool, long: f32, tall: f32, thick: f32) -> Vec3 {
    if along_x { Vec3::new(long, tall, thick) } else { Vec3::new(thick, tall, long) }
}

/// Spawns arena pieces.
struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
}

impl Builder<'_, '_, '_> {
    fn mesh(&mut self, mesh: impl Into<Mesh>) -> Handle<Mesh> {
        self.meshes.add(mesh)
    }

    fn material(&mut self, material: impl Into<StandardMaterial>) -> Handle<StandardMaterial> {
        self.materials.add(material)
    }

    /// Light itself: glows in `color` whatever lights it, `strength` times
    /// brighter than white so the bloom picks it up.
    fn glow(&mut self, color: Color, strength: f32) -> Handle<StandardMaterial> {
        self.materials.add(StandardMaterial { base_color: Color::BLACK, emissive: color.to_linear() * strength, ..default() })
    }

    fn put(&mut self, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, transform: Transform) -> EntityCommands<'_> {
        self.commands.spawn((ArenaPiece, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform))
    }

    fn block(&mut self, size: Vec3, material: &Handle<StandardMaterial>, at: Vec3) -> EntityCommands<'_> {
        let mesh = self.mesh(Cuboid::from_size(size));
        self.put(&mesh, material, Transform::from_translation(at))
    }

    fn light(&mut self, bundle: impl Bundle) {
        self.commands.spawn((ArenaPiece, bundle));
    }
}

fn build_arena(
    mut commands: Commands,
    arena: Res<Arena>,
    pieces: Query<Entity, With<ArenaPiece>>,
    camera: Single<Entity, With<Camera3d>>,
    ball: Res<BallLook>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    for piece in &pieces {
        commands.entity(piece).despawn();
    }
    // The ball glows a little in the dark, so it reads against the night.
    if let Some(mut material) = materials.get_mut(&ball.0) {
        material.emissive = if *arena == Arena::Neon { Color::srgb(1.0, 0.85, 0.4).to_linear() * 0.8 } else { LinearRgba::BLACK };
    }
    let mut build = Builder { commands: &mut commands, meshes: &mut meshes, materials: &mut materials };
    match *arena {
        Arena::Beach => beach(&mut build, &assets),
        Arena::Neon => neon(&mut build, &mut images),
    }
    let mut camera = commands.entity(*camera);
    match *arena {
        Arena::Beach => {
            // Sea haze: the far ocean fades into the sky.
            camera.insert(DistanceFog {
                color: Color::srgb(0.75, 0.86, 0.96),
                falloff: FogFalloff::Linear { start: 60.0, end: 260.0 },
                ..default()
            });
            camera.remove::<Bloom>();
        }
        Arena::Neon => {
            camera.insert((
                DistanceFog { color: Color::srgb(0.02, 0.025, 0.06), falloff: FogFalloff::Linear { start: 70.0, end: 420.0 }, ..default() },
                Bloom { intensity: 0.22, ..Bloom::NATURAL },
            ));
        }
    }
}

// The beach -------------------------------------------------------------------

const SAND_SIZE: f32 = 160.0;
/// Real-world size of one tile of the sand texture.
const SAND_TILE: f32 = 1.5;
const PAD_HEIGHT: f32 = 1.0;
const LINE_COLOR: Color = Color::srgb(0.1, 0.35, 0.85);
const SKY: Color = Color::srgb(0.55, 0.78, 0.97);

fn beach(build: &mut Builder, assets: &AssetServer) {
    build.commands.insert_resource(ClearColor(SKY));
    // Warm sun, nearly overhead so shadows land close to what casts them, and
    // bluish sky light filling the shadows.
    build.light((
        DirectionalLight { color: Color::srgb(1.0, 0.95, 0.85), illuminance: 11_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(2.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    build.commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.8, 1.0), brightness: 600.0, ..default() });

    let repeating = |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            anisotropy_clamp: 16,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let sand_color = assets.load_builder().with_settings(repeating).load("textures/sand_01_diff_2k.jpg");
    let sand_normal = assets
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            repeating(settings);
            settings.is_srgb = false;
        })
        .load("textures/sand_01_nor_gl_2k.jpg");
    build.commands.insert_resource(NeedsMipmaps(vec![sand_color.id(), sand_normal.id()]));
    let sand = build.material(StandardMaterial {
        base_color_texture: Some(sand_color),
        normal_map_texture: Some(sand_normal),
        perceptual_roughness: 0.95,
        reflectance: 0.2,
        uv_transform: Affine2::from_scale(Vec2::splat(SAND_SIZE / SAND_TILE)),
        ..default()
    });
    let floor = build.mesh(Plane3d::default().mesh().size(SAND_SIZE, SAND_SIZE));
    build.put(&floor, &sand, Transform::default());
    let ocean = build.material(StandardMaterial {
        base_color: Color::srgb(0.05, 0.33, 0.42),
        perceptual_roughness: 0.2,
        reflectance: 0.6,
        ..default()
    });
    let sea = build.mesh(Plane3d::default().mesh().size(800.0, 800.0));
    build.put(&sea, &ocean, Transform::from_xyz(0.0, -0.25, 0.0));

    glass_walls(build);

    // Straps along the walls, and one under the net.
    let line = 0.08;
    let strap = build.material(LINE_COLOR);
    for (size, at) in [
        (Vec3::new(2.0 * HALF_LENGTH + line, 0.01, line), Vec3::new(0.0, 0.005, -HALF_WIDTH)),
        (Vec3::new(2.0 * HALF_LENGTH + line, 0.01, line), Vec3::new(0.0, 0.005, HALF_WIDTH)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(-HALF_LENGTH, 0.005, 0.0)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(HALF_LENGTH, 0.005, 0.0)),
        (Vec3::new(line, 0.01, 2.0 * HALF_WIDTH), Vec3::new(0.0, 0.005, 0.0)),
    ] {
        build.block(size, &strap, at);
    }

    // Net: a translucent mesh with a blue top band, between padded posts.
    let mesh = build.material(StandardMaterial {
        base_color: Color::srgba(0.05, 0.05, 0.05, 0.55),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    build.block(Vec3::new(0.02, 1.0, 2.0 * NET_HALF_WIDTH), &mesh, Vec3::new(0.0, NET_HEIGHT - 0.5, 0.0));
    let band = build.material(LINE_COLOR);
    build.block(Vec3::new(0.04, 0.07, 2.0 * NET_HALF_WIDTH), &band, Vec3::new(0.0, NET_HEIGHT - 0.035, 0.0));
    let post_height = NET_HEIGHT + 0.1;
    let post = build.mesh(Cylinder::new(0.05, post_height));
    let post_material = build.material(Color::srgb(0.85, 0.85, 0.88));
    let pad = build.mesh(Cylinder::new(0.12, 1.8));
    let pad_material = build.material(LINE_COLOR);
    for z in [-NET_HALF_WIDTH - 0.1, NET_HALF_WIDTH + 0.1] {
        build.put(&post, &post_material, Transform::from_xyz(0.0, post_height / 2.0, z));
        build.put(&pad, &pad_material, Transform::from_xyz(0.0, 0.9, z));
    }
}

/// Glass walls around the arena, padded along the base in each half's team
/// color, with frame posts and a top rail.
fn glass_walls(build: &mut Builder) {
    let glass = build.material(StandardMaterial {
        base_color: Color::srgba(0.75, 0.9, 1.0, 0.12),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.05,
        reflectance: 0.8,
        ..default()
    });
    let frame = build.material(Color::srgb(0.92, 0.93, 0.95));
    let pads = [build.material(TEAM_COLORS[0].darker(0.1)), build.material(TEAM_COLORS[1].darker(0.1))];
    let post = build.mesh(Cuboid::new(0.12, WALL_HEIGHT, 0.12));
    for (center, length, along_x) in walls() {
        build.block(along(along_x, length, WALL_HEIGHT, WALL_THICKNESS), &glass, center + Vec3::Y * WALL_HEIGHT / 2.0).insert(NotShadowCaster);
        build.block(along(along_x, length, 0.12, 0.16), &frame, center + Vec3::Y * WALL_HEIGHT);
        // Pads: a side wall spans both halves, so it gets one per half.
        let halves: &[(f32, f32)] = if along_x { &[(-HALF_LENGTH / 2.0, HALF_LENGTH), (HALF_LENGTH / 2.0, HALF_LENGTH)] } else { &[(0.0, 2.0 * HALF_WIDTH)] };
        for &(offset, long) in halves {
            let at = center + if along_x { Vec3::new(offset, 0.0, 0.0) } else { Vec3::ZERO };
            let pad = pads[usize::from(at.x > 0.0)].clone();
            build.block(along(along_x, long, PAD_HEIGHT, 0.25), &pad, at + Vec3::Y * PAD_HEIGHT / 2.0);
        }
        for offset in posts(length) {
            let offset = if along_x { Vec3::new(offset, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, offset) };
            build.put(&post, &frame, Transform::from_translation(center + offset + Vec3::Y * WALL_HEIGHT / 2.0));
        }
    }
}

/// Offsets along a wall of `length` for its frame posts, ends included.
fn posts(length: f32) -> impl Iterator<Item = f32> {
    let count = (length / POST_SPACING).round() as i32;
    (0..=count).map(move |i| -length / 2.0 + length * i as f32 / count as f32)
}

// The Neon Stadium ------------------------------------------------------------

const NIGHT: Color = Color::srgb(0.008, 0.01, 0.025);
/// The cool white of the arena's lights.
const NEON_WHITE: Color = Color::srgb(0.75, 0.9, 1.0);
const NEON_CYAN: Color = Color::srgb(0.2, 0.85, 1.0);
/// Floor grid squares, meters.
const GRID_TILE: f32 = 2.0;
/// Stands: rows of seats rising away from the walls.
const STAND_GAP: f32 = 5.0;
const STAND_ROWS: usize = 7;
const ROW_DEPTH: f32 = 2.0;
const ROW_RISE: f32 = 1.4;
const SEAT_SPACING: f32 = 0.75;

fn neon(build: &mut Builder, images: &mut Assets<Image>) {
    build.commands.insert_resource(ClearColor(NIGHT));
    build.commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.5, 0.6, 1.0), brightness: 220.0, ..default() });
    // Banks of stadium lights high above, casting the shadows.
    build.light((
        DirectionalLight { color: Color::srgb(0.85, 0.9, 1.0), illuminance: 3_500.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-3.0, 10.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Both halves washed in the same cool light from their ends.
    for side in [-1.0, 1.0] {
        build.light((
            PointLight { color: NEON_WHITE, intensity: 2_000_000.0, range: 60.0, ..default() },
            Transform::from_xyz(side * (HALF_LENGTH - 3.0), 7.0, 0.0),
        ));
    }

    neon_floor(build, images);
    energy_walls(build);
    energy_net(build);
    stands(build);
    sky(build);
}

/// A dark, glossy floor with a glowing grid, bright court lines and a ring in
/// the middle of each half, the same on both sides.
fn neon_floor(build: &mut Builder, images: &mut Assets<Image>) {
    let grid = images.add(grid_texture());
    let outside = build.material(StandardMaterial { base_color: Color::srgb(0.012, 0.014, 0.022), perceptual_roughness: 0.6, ..default() });
    let ground = build.mesh(Plane3d::default().mesh().size(600.0, 600.0));
    build.put(&ground, &outside, Transform::from_xyz(0.0, -0.02, 0.0));

    let court = build.mesh(Plane3d::default().mesh().size(2.0 * HALF_LENGTH, 2.0 * HALF_WIDTH));
    let floor = build.material(StandardMaterial {
        base_color: Color::srgb(0.02, 0.024, 0.04),
        perceptual_roughness: 0.15,
        metallic: 0.2,
        reflectance: 0.7,
        emissive: NEON_CYAN.to_linear() * 0.6,
        emissive_texture: Some(grid),
        uv_transform: Affine2::from_scale(Vec2::new(2.0 * HALF_LENGTH, 2.0 * HALF_WIDTH) / GRID_TILE),
        ..default()
    });
    build.put(&court, &floor, Transform::default());

    // A ring in the middle of each half, like a kickoff circle.
    let ring = build.mesh(Annulus::new(2.8, 3.0));
    let glow = build.glow(NEON_CYAN, 5.0);
    for side in [-1.0, 1.0] {
        build.put(&ring, &glow, Transform::from_xyz(side * HALF_LENGTH / 2.0, 0.01, 0.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)));
    }

    let line = build.glow(NEON_WHITE, 6.0);
    let width = 0.1;
    for (size, at) in [
        (Vec3::new(2.0 * HALF_LENGTH, 0.01, width), Vec3::new(0.0, 0.006, -HALF_WIDTH + width)),
        (Vec3::new(2.0 * HALF_LENGTH, 0.01, width), Vec3::new(0.0, 0.006, HALF_WIDTH - width)),
        (Vec3::new(width, 0.01, 2.0 * HALF_WIDTH), Vec3::new(-HALF_LENGTH + width, 0.006, 0.0)),
        (Vec3::new(width, 0.01, 2.0 * HALF_WIDTH), Vec3::new(HALF_LENGTH - width, 0.006, 0.0)),
        (Vec3::new(2.0 * width, 0.01, 2.0 * HALF_WIDTH), Vec3::new(0.0, 0.006, 0.0)),
    ] {
        build.block(size, &line, at).insert(NotShadowCaster);
    }
}

/// One square of the floor grid, for the emissive map: bright edges with a
/// soft falloff, and a faint cross through the middle.
fn grid_texture() -> Image {
    const SIZE: usize = 128;
    let mut data = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = |v: usize| (v.min(SIZE - 1 - v)) as f32;
            let middle = |v: usize| (v as f32 - SIZE as f32 / 2.0).abs();
            let from_edge = edge(x).min(edge(y));
            let from_middle = middle(x).min(middle(y));
            let line = (1.0 - from_edge / 3.0).max(0.0) + 0.35 * (1.0 - from_edge / 10.0).max(0.0);
            let cross = 0.25 * (1.0 - from_middle / 1.5).max(0.0);
            let value = (255.0 * (line + cross).min(1.0)) as u8;
            data.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 16,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

/// Walls of faint blue energy: a bright rail on top, ribs between, and a big
/// glowing frame on each end wall.
fn energy_walls(build: &mut Builder) {
    let field = build.material(StandardMaterial {
        base_color: Color::srgba(0.3, 0.6, 1.0, 0.06),
        emissive: NEON_CYAN.to_linear() * 0.03,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.05,
        reflectance: 0.9,
        ..default()
    });
    let rail = build.glow(NEON_WHITE, 5.0);
    let rib = build.glow(NEON_CYAN, 1.6);
    let rib_mesh = build.mesh(Cuboid::new(0.06, WALL_HEIGHT, 0.06));
    for (center, length, along_x) in walls() {
        build.block(along(along_x, length, WALL_HEIGHT, WALL_THICKNESS), &field, center + Vec3::Y * WALL_HEIGHT / 2.0).insert(NotShadowCaster);
        build.block(along(along_x, length, 0.08, 0.12), &rail, center + Vec3::Y * WALL_HEIGHT).insert(NotShadowCaster);
        for offset in posts(length) {
            let offset = if along_x { Vec3::new(offset, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, offset) };
            build.put(&rib_mesh, &rib, Transform::from_translation(center + offset + Vec3::Y * WALL_HEIGHT / 2.0)).insert(NotShadowCaster);
        }
    }
    // The end walls: a frame of light, like a goal.
    let (width, height) = (12.0, 4.5);
    let frame = build.glow(NEON_WHITE, 9.0);
    let fill = build.material(StandardMaterial {
        base_color: NEON_CYAN.with_alpha(0.06),
        emissive: NEON_CYAN.to_linear() * 0.2,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    for side in [-1.0, 1.0] {
        let x = side * (HALF_LENGTH + WALL_THICKNESS);
        let bar = 0.18;
        for (size, at) in [
            (Vec3::new(0.1, bar, width), Vec3::new(x, height, 0.0)),
            (Vec3::new(0.1, height, bar), Vec3::new(x, height / 2.0, -width / 2.0)),
            (Vec3::new(0.1, height, bar), Vec3::new(x, height / 2.0, width / 2.0)),
        ] {
            build.block(size, &frame, at).insert(NotShadowCaster);
        }
        build.block(Vec3::new(0.05, height, width), &fill, Vec3::new(x, height / 2.0, 0.0)).insert(NotShadowCaster);
    }
    // Bright pillars at the corners.
    let pillar = build.glow(NEON_WHITE, 7.0);
    let pillar_mesh = build.mesh(Cuboid::new(0.25, WALL_HEIGHT, 0.25));
    for x in [-HALF_LENGTH, HALF_LENGTH] {
        for z in [-HALF_WIDTH, HALF_WIDTH] {
            build.put(&pillar_mesh, &pillar, Transform::from_xyz(x, WALL_HEIGHT / 2.0, z)).insert(NotShadowCaster);
        }
    }
}

/// A net of light: a faint field, a bright band along the top and glowing posts.
fn energy_net(build: &mut Builder) {
    let field = build.material(StandardMaterial {
        base_color: Color::srgba(0.3, 0.8, 1.0, 0.22),
        emissive: NEON_CYAN.to_linear() * 0.6,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    build.block(Vec3::new(0.02, 1.0, 2.0 * NET_HALF_WIDTH), &field, Vec3::new(0.0, NET_HEIGHT - 0.5, 0.0)).insert(NotShadowCaster);
    let band = build.glow(NEON_WHITE, 14.0);
    build.block(Vec3::new(0.06, 0.09, 2.0 * NET_HALF_WIDTH), &band, Vec3::new(0.0, NET_HEIGHT - 0.035, 0.0)).insert(NotShadowCaster);
    let low = build.glow(NEON_CYAN, 3.0);
    build.block(Vec3::new(0.04, 0.04, 2.0 * NET_HALF_WIDTH), &low, Vec3::new(0.0, NET_HEIGHT - 1.0, 0.0)).insert(NotShadowCaster);
    let post_height = NET_HEIGHT + 0.1;
    let post = build.mesh(Cylinder::new(0.07, post_height));
    let post_glow = build.glow(NEON_CYAN, 4.0);
    for z in [-NET_HALF_WIDTH - 0.1, NET_HALF_WIDTH + 0.1] {
        build.put(&post, &post_glow, Transform::from_xyz(0.0, post_height / 2.0, z));
    }
}

/// Rows of stands on all four sides with a crowd in the seats.
fn stands(build: &mut Builder) {
    let concrete = build.material(StandardMaterial { base_color: Color::srgb(0.035, 0.04, 0.06), perceptual_roughness: 0.85, ..default() });
    // Fans: a body and a head, in dark shirts.
    let body_mesh = build.mesh(Capsule3d::new(0.21, 0.3));
    let head_mesh = build.mesh(Sphere::new(0.12).mesh().ico(2).unwrap());
    let fans: Vec<_> = [
        Color::srgb(0.75, 0.2, 0.2),
        Color::srgb(0.2, 0.35, 0.8),
        Color::srgb(0.85, 0.85, 0.9),
        Color::srgb(0.12, 0.12, 0.14),
        Color::srgb(0.9, 0.6, 0.2),
        Color::srgb(0.3, 0.7, 0.4),
    ]
    .into_iter()
    .map(|color| build.material(StandardMaterial { base_color: color.darker(0.55), perceptual_roughness: 0.95, ..default() }))
    .collect();
    let skin = build.material(StandardMaterial { base_color: Color::srgb(0.18, 0.13, 0.1), perceptual_roughness: 0.9, ..default() });
    let phone = build.glow(NEON_WHITE, 12.0);
    let phone_mesh = build.mesh(Cuboid::new(0.05, 0.08, 0.05));

    let mut seed = 0u32;
    // Each side as (where its first row starts, outward direction, length along it).
    let sides = [
        (Vec3::new(0.0, 0.0, -HALF_WIDTH - STAND_GAP), Vec3::NEG_Z, 2.0 * HALF_LENGTH + 2.0 * STAND_GAP),
        (Vec3::new(0.0, 0.0, HALF_WIDTH + STAND_GAP), Vec3::Z, 2.0 * HALF_LENGTH + 2.0 * STAND_GAP),
        (Vec3::new(-HALF_LENGTH - STAND_GAP, 0.0, 0.0), Vec3::NEG_X, 2.0 * HALF_WIDTH + 2.0 * STAND_GAP),
        (Vec3::new(HALF_LENGTH + STAND_GAP, 0.0, 0.0), Vec3::X, 2.0 * HALF_WIDTH + 2.0 * STAND_GAP),
    ];
    for (start, out, length) in sides {
        let along_x = out.x == 0.0;
        // Each row is wider than the one in front, so the corners close up.
        for row in 0..STAND_ROWS {
            let long = length + 2.0 * ROW_DEPTH * row as f32;
            let top = ROW_RISE * (row + 1) as f32;
            let middle = start + out * (ROW_DEPTH * (row as f32 + 0.5));
            build.block(along(along_x, long, top, ROW_DEPTH), &concrete, middle + Vec3::Y * top / 2.0);
            // The crowd: most seats taken, a few phone lights up.
            let seats = (long / SEAT_SPACING) as i32;
            for seat in 0..seats {
                seed = seed.wrapping_add(1);
                if hash(seed, 0) < 0.3 {
                    continue;
                }
                let offset = -long / 2.0 + (seat as f32 + 0.5) * SEAT_SPACING;
                let sideways = if along_x { Vec3::X } else { Vec3::Z };
                // Some standing, some sitting; never quite in line.
                let height = 0.35 + 0.25 * hash(seed, 4);
                let at = middle + sideways * (offset + (hash(seed, 5) - 0.5) * 0.25) + out * (hash(seed, 1) - 0.5) * 0.4 + Vec3::Y * (top + height);
                let fan = fans[(hash(seed, 2) * fans.len() as f32) as usize % fans.len()].clone();
                build.put(&body_mesh, &fan, Transform::from_translation(at)).insert(NotShadowCaster);
                build.put(&head_mesh, &skin, Transform::from_translation(at + Vec3::Y * 0.42)).insert(NotShadowCaster);
                if hash(seed, 3) < 0.06 {
                    build.put(&phone_mesh, &phone, Transform::from_translation(at + Vec3::Y * 0.75 - out * 0.15)).insert(NotShadowCaster);
                }
            }
        }
    }

    // Light towers at the corners, beyond the stands.
    let tower = build.material(StandardMaterial { base_color: Color::srgb(0.05, 0.055, 0.07), ..default() });
    let lamps = build.glow(NEON_WHITE, 20.0);
    let reach = STAND_GAP + ROW_DEPTH * STAND_ROWS as f32 + 2.0;
    for x in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            let foot = Vec3::new(x * (HALF_LENGTH + reach), 0.0, z * (HALF_WIDTH + reach));
            build.block(Vec3::new(1.0, 30.0, 1.0), &tower, foot + Vec3::Y * 15.0);
            let facing = Transform::from_translation(foot + Vec3::Y * 30.5).looking_at(Vec3::new(0.0, 30.5, 0.0), Vec3::Y);
            let panel = build.mesh(Cuboid::new(5.0, 3.0, 0.3));
            build.put(&panel, &lamps, facing).insert(NotShadowCaster);
        }
    }
}

/// Stars, and a great ring of light hanging over the net, turning slowly.
fn sky(build: &mut Builder) {
    let star = build.mesh(Sphere::new(1.0).mesh().ico(1).unwrap());
    let stars = [build.glow(Color::WHITE, 3.0), build.glow(Color::srgb(0.7, 0.8, 1.0), 6.0)];
    for i in 0..220u32 {
        let (a, b, c) = (hash(i, 10), hash(i, 11), hash(i, 12));
        let around = a * TAU;
        let up = 0.12 + 0.8 * b;
        let direction = Vec3::new(around.cos() * (1.0 - up * up).sqrt(), up, around.sin() * (1.0 - up * up).sqrt());
        let size = 0.5 + 1.2 * c;
        let material = stars[usize::from(c > 0.7)].clone();
        build.put(&star, &material, Transform::from_translation(direction * 380.0).with_scale(Vec3::splat(size))).insert(NotShadowCaster);
    }
    let ring = build.mesh(Torus::new(13.5, 14.0));
    let glow = build.glow(NEON_CYAN, 4.0);
    build.put(&ring, &glow, Transform::from_xyz(0.0, 24.0, 0.0)).insert((Spin(0.08), NotShadowCaster));
    let inner = build.mesh(Torus::new(9.8, 10.0));
    let white = build.glow(NEON_WHITE, 3.0);
    build
        .put(&inner, &white, Transform::from_xyz(0.0, 23.0, 0.0).with_rotation(Quat::from_rotation_x(0.12)))
        .insert((Spin(-0.13), NotShadowCaster));
}

/// A repeatable random number in [0, 1).
fn hash(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn spin(time: Res<Time>, mut spinning: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut spinning {
        transform.rotate_y(spin.0 * time.delta_secs());
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
        // Already done, when an arena is built again.
        if image.texture_descriptor.mip_level_count > 1 {
            return false;
        }
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
