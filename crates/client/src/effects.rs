//! Sand kicked up where the ball lands, where a dive hits the floor, where a
//! dash pushes off, and where players land from jumps.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use volley_sim::{Event, MoveId};

use crate::{Match, SimEvent};

/// When a diving player's body hits the sand, after the lunge starts.
const DIVE_IMPACT_SECONDS: f32 = 0.4;
/// When a foot save's leg sweeps through the sand.
const KICK_IMPACT_SECONDS: f32 = 0.1;
const GRAIN_GRAVITY: f32 = 9.8;

pub fn plugin(app: &mut App) {
    app.init_resource::<PendingPuffs>()
        .add_systems(Startup, load_grain)
        .add_systems(Update, (kick_up_sand, fly_grains));
}

#[derive(Resource)]
struct GrainLook {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Grain {
    velocity: Vec3,
    age: f32,
    life: f32,
    size: f32,
}

/// Dive impacts waiting to happen: (when, which player).
#[derive(Resource, Default)]
struct PendingPuffs(Vec<(f32, usize)>);

fn load_grain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(GrainLook {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        material: materials.add(StandardMaterial {
            // Lighter than the sand, so the dust stands out against it.
            base_color: Color::srgba(0.97, 0.9, 0.74, 0.8),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
    });
}

fn kick_up_sand(
    mut commands: Commands,
    game: Res<Match>,
    time: Res<Time>,
    look: Res<GrainLook>,
    mut events: MessageReader<SimEvent>,
    mut pending: ResMut<PendingPuffs>,
    mut seed: Local<u32>,
) {
    let now = time.elapsed_secs();
    let mut puff = |at: Vec3, grains: u32, speed: f32| {
        for _ in 0..grains {
            *seed = seed.wrapping_add(1);
            let [a, b, c] = [random(*seed, 0), random(*seed, 1), random(*seed, 2)];
            let angle = a * std::f32::consts::TAU;
            let out = speed * (0.4 + 0.6 * b);
            commands.spawn((
                Grain {
                    velocity: Vec3::new(angle.cos() * out, speed * (0.8 + c), angle.sin() * out),
                    age: 0.0,
                    life: 0.5 + 0.4 * c,
                    size: 0.025 + 0.04 * b,
                },
                Mesh3d(look.mesh.clone()),
                MeshMaterial3d(look.material.clone()),
                Transform::from_translation(at.with_y(0.05)).with_scale(Vec3::ZERO),
                NotShadowCaster,
            ));
        }
    };

    for SimEvent(event) in events.read() {
        match *event {
            Event::Landed { at, velocity, .. } => puff(at, 40, (velocity.length() * 0.15).clamp(1.5, 3.5)),
            Event::MoveStarted { player, id: MoveId::Dive } => pending.0.push((now + DIVE_IMPACT_SECONDS, player)),
            Event::MoveStarted { player, id: MoveId::FootSave } => pending.0.push((now + KICK_IMPACT_SECONDS, player)),
            // Sand sprays back off the pushing foot.
            Event::Dashed { player } => puff(game.current.players[player].position, 30, 1.9),
            Event::MoveStarted { player, id: MoveId::Posterizer } => puff(game.current.players[player].position, 60, 3.0),
            Event::Posterized { player } => pending.0.push((now + 0.35, player)),
            _ => {}
        }
    }
    pending.0.retain(|&(when, player)| {
        if now < when {
            return true;
        }
        puff(game.current.players[player].position, 55, 2.6);
        false
    });
    // Landing from a jump.
    for (before, after) in game.previous.players.iter().zip(&game.current.players) {
        if !before.grounded() && after.grounded() && game.previous.rally == game.current.rally {
            puff(after.position, 20, 1.4);
        }
    }
}

fn fly_grains(mut commands: Commands, time: Res<Time>, mut grains: Query<(Entity, &mut Grain, &mut Transform)>) {
    let dt = time.delta_secs();
    for (entity, mut grain, mut transform) in &mut grains {
        grain.age += dt;
        if grain.age >= grain.life {
            commands.entity(entity).despawn();
            continue;
        }
        grain.velocity.y -= GRAIN_GRAVITY * dt;
        grain.velocity *= 1.0 - 1.5 * dt;
        transform.translation += grain.velocity * dt;
        transform.translation.y = transform.translation.y.max(0.02);
        transform.scale = Vec3::splat(grain.size * (1.0 - grain.age / grain.life));
    }
}

/// A repeatable pseudo-random number in 0..1.
fn random(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 13;
    (x & 0xFFFF) as f32 / 65535.0
}
