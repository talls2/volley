//! A glowing streak behind the ball, like Mario Tennis. Its color says what the
//! last hit was: fiery spikes, gold serves, cool passes.
//!
//! The streak is a ribbon of triangles through the ball's recent positions,
//! rebuilt each frame to face the camera, widest at the ball and fading to
//! nothing at the tail.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use volley_sim::court::BALL_RADIUS;
use volley_sim::{Ball, Event, HitKind};

use crate::scene::BallView;
use crate::{Match, SimEvent};

/// How long each point of the trail lasts.
const TRAIL_SECONDS: f32 = 0.35;
/// Half the ribbon's width at the ball, relative to the ball.
const HEAD_WIDTH: f32 = 0.9;

const BLOCK_COLOR: Color = Color::srgb(0.85, 0.5, 1.0);

pub fn plugin(app: &mut App) {
    app.init_resource::<Trail>()
        .add_systems(Startup, spawn_trail)
        // After the ball has been placed for this frame.
        .add_systems(PostUpdate, (record_trail, draw_trail).chain());
}

fn color_for(kind: HitKind) -> Color {
    match kind {
        HitKind::Spike => Color::srgb(1.0, 0.35, 0.05),
        HitKind::Serve => Color::srgb(1.0, 0.8, 0.2),
        HitKind::Pass | HitKind::Dig => Color::srgb(0.55, 0.85, 1.0),
        HitKind::Lob => Color::srgb(0.6, 1.0, 0.7),
    }
}

#[derive(Resource)]
struct Trail {
    /// Oldest first: where the ball was, and when.
    points: VecDeque<(Vec3, f32)>,
    color: Color,
    rally: u32,
}

impl Default for Trail {
    fn default() -> Self {
        Self { points: VecDeque::new(), color: color_for(HitKind::Serve), rally: 0 }
    }
}

#[derive(Component)]
struct TrailView;

fn spawn_trail(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Starts as one invisible triangle so the mesh always has its attributes.
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    commands.spawn((
        TrailView,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            // Glows: adds light instead of covering what's behind.
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            double_sided: true,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        // Its bounds change every frame; never skip drawing it.
        NoFrustumCulling,
    ));
}

fn record_trail(
    game: Res<Match>,
    time: Res<Time>,
    mut events: MessageReader<SimEvent>,
    ball: Single<&Transform, With<BallView>>,
    mut trail: ResMut<Trail>,
) {
    // Every hit starts a fresh streak, so it never bends back through the contact.
    for SimEvent(event) in events.read() {
        match *event {
            Event::Touched { kind, .. } => {
                trail.color = color_for(kind);
                trail.points.clear();
            }
            Event::Blocked { .. } => {
                trail.color = BLOCK_COLOR;
                trail.points.clear();
            }
            Event::HitNet { .. } => trail.points.clear(),
            _ => {}
        }
    }
    if trail.rally != game.current.rally {
        trail.rally = game.current.rally;
        trail.points.clear();
    }
    let now = time.elapsed_secs();
    if matches!(game.current.ball, Ball::InFlight(_)) {
        trail.points.push_back((ball.translation, now));
    }
    while trail.points.front().is_some_and(|&(_, t)| now - t > TRAIL_SECONDS) {
        trail.points.pop_front();
    }
}

fn draw_trail(
    trail: Res<Trail>,
    time: Res<Time>,
    camera: Single<&Transform, With<Camera3d>>,
    view: Single<(&Mesh3d, &mut Visibility), With<TrailView>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (mesh, mut visibility) = view.into_inner();
    let points = &trail.points;
    if points.len() < 2 {
        *visibility = Visibility::Hidden;
        return;
    }
    *visibility = Visibility::Visible;

    let now = time.elapsed_secs();
    let count = points.len();
    let mut positions = Vec::with_capacity(2 * count);
    let mut normals = Vec::with_capacity(2 * count);
    let mut colors = Vec::with_capacity(2 * count);
    for (i, &(point, born)) in points.iter().enumerate() {
        let before = points[i.saturating_sub(1)].0;
        let after = points[(i + 1).min(count - 1)].0;
        let to_eye = (camera.translation - point).normalize_or_zero();
        let side = (after - before).cross(to_eye).normalize_or_zero();
        // 1 at the ball, 0 at the tail.
        let life = (1.0 - (now - born) / TRAIL_SECONDS).clamp(0.0, 1.0);
        let half_width = BALL_RADIUS * HEAD_WIDTH * life;
        let color = trail.color.to_linear().with_alpha(life * life).to_f32_array();
        positions.extend([(point + side * half_width).to_array(), (point - side * half_width).to_array()]);
        normals.extend([to_eye.to_array(); 2]);
        colors.extend([color; 2]);
    }
    let indices = (0..count as u32 - 1)
        .flat_map(|i| {
            let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
            [a, b, c, b, d, c]
        })
        .collect();

    if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
    }
}
