//! Player characters: Quaternius glTF models, animated from the match state.
//!
//! Animations follow the game, never drive it: the simulation decides where a
//! player is and when they hit the ball, and this module picks the animation
//! that shows it.
//!
//! The character files contain no animations. Those live in one shared library
//! file built on the same skeleton, and Bevy only wires animation targets into
//! models whose own file is animated, so `hook_up_skeleton` does that wiring.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::Duration;

use bevy::animation::{AnimatedBy, AnimationTargetId};
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use volley_sim::{DT, Event, HitKind, Phase, court};

use crate::input::LOCAL_TEAM;
use crate::scene::{TEAM_COLORS, player_feet};
use crate::{Match, SimEvent};

/// A body and its hairstyle, rigged to the same skeleton.
struct Look {
    body: &'static str,
    hair: &'static str,
    /// The pack's hair textures are grey, meant to be colored by a shader; we tint the material instead.
    hair_color: Color,
}

const LOOKS: [Look; 2] = [
    Look {
        body: "characters/Superhero_Male_FullBody.gltf",
        hair: "characters/Hair_SimpleParted.gltf",
        hair_color: Color::srgb(0.3, 0.18, 0.1),
    },
    Look {
        body: "characters/Superhero_Female_FullBody.gltf",
        hair: "characters/Hair_Buns.gltf",
        hair_color: Color::srgb(0.12, 0.1, 0.09),
    },
];
const ANIMATION_LIBRARY: &str = "animations/UAL1_Standard.glb";

const BLEND: Duration = Duration::from_millis(150);
/// Radians per second.
const TURN_SPEED: f32 = 10.0;
const JOG_SPEED: f32 = 0.4;
const SPRINT_SPEED: f32 = 4.0;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, (load_animation_library, spawn_characters))
        .add_systems(
            Update,
            (
                build_animation_graph.run_if(not(resource_exists::<Animations>)),
                (react_to_events, place_characters, animate_characters)
                    .chain()
                    .run_if(resource_exists::<Animations>),
                draw_team_markers,
            ),
        );
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Clip {
    Idle,
    Jog,
    Sprint,
    JumpStart,
    Airborne,
    Land,
    Pass,
    Spike,
    Serve,
    Celebrate,
}

impl Clip {
    const ALL: [Clip; 10] = [
        Clip::Idle,
        Clip::Jog,
        Clip::Sprint,
        Clip::JumpStart,
        Clip::Airborne,
        Clip::Land,
        Clip::Pass,
        Clip::Spike,
        Clip::Serve,
        Clip::Celebrate,
    ];

    /// The animation's name in the library. The free library has no volleyball
    /// moves, so hits borrow the closest motion it does have.
    fn source(self) -> &'static str {
        match self {
            Clip::Idle => "Idle_Loop",
            Clip::Jog => "Jog_Fwd_Loop",
            Clip::Sprint => "Sprint_Loop",
            Clip::JumpStart => "Jump_Start",
            Clip::Airborne => "Jump_Loop",
            Clip::Land => "Jump_Land",
            Clip::Pass => "Spell_Simple_Shoot",
            Clip::Spike => "Sword_Attack",
            Clip::Serve => "Punch_Cross",
            Clip::Celebrate => "Dance_Loop",
        }
    }

    fn looping(self) -> bool {
        matches!(self, Clip::Idle | Clip::Jog | Clip::Sprint | Clip::Airborne | Clip::Celebrate)
    }

    /// Moves started by the game rather than by running and jumping. Takeoffs and
    /// landings never cut these short.
    fn is_game_action(self) -> bool {
        matches!(self, Clip::Pass | Clip::Spike | Clip::Serve | Clip::Celebrate)
    }
}

#[derive(Resource)]
struct AnimationLibrary(Handle<Gltf>);

#[derive(Resource)]
struct Animations {
    graph: Handle<AnimationGraph>,
    nodes: HashMap<Clip, AnimationNodeIndex>,
}

/// The model showing player `index`.
#[derive(Component)]
struct Character {
    index: usize,
    /// The skeleton's top node, which holds the `AnimationPlayer`. Set once the model spawns.
    armature: Option<Entity>,
    playing: Option<Clip>,
    /// A one-off move (hit, jump, landing, celebration) that plays over running
    /// until it ends.
    action: Option<Clip>,
    /// Play `action` from the start even if it's already playing, e.g. two passes in a row.
    restart: bool,
    airborne: bool,
    /// Facing, as a rotation about the vertical axis. 0 faces +z.
    yaw: f32,
}

fn load_animation_library(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(AnimationLibrary(assets.load(ANIMATION_LIBRARY)));
}

fn build_animation_graph(
    mut commands: Commands,
    library: Res<AnimationLibrary>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Some(gltf) = gltfs.get(&library.0) else {
        return;
    };
    let mut graph = AnimationGraph::new();
    let nodes = Clip::ALL
        .into_iter()
        .map(|clip| {
            let handle = gltf.named_animations.get(clip.source()).unwrap_or_else(|| {
                panic!("{ANIMATION_LIBRARY} has no animation named {}", clip.source())
            });
            (clip, graph.add_clip(handle.clone(), 1.0, graph.root))
        })
        .collect();
    commands.insert_resource(Animations { graph: graphs.add(graph), nodes });
}

fn spawn_characters(mut commands: Commands, assets: Res<AssetServer>, game: Res<Match>) {
    for (index, player) in game.current.players.iter().enumerate() {
        let model = assets.load(GltfAssetLabel::Scene(0).from_asset(LOOKS[index % LOOKS.len()].body));
        // Start out facing the net.
        let yaw = -court::side(player.team) * FRAC_PI_2;
        commands
            .spawn((
                Character { index, armature: None, playing: None, action: None, restart: false, airborne: false, yaw },
                WorldAssetRoot(model),
                Transform::default(),
            ))
            .observe(hook_up_skeleton);
    }
}

/// Once a body has spawned: puts an `AnimationPlayer` on its armature, wires
/// the skeleton to it, and adds the hairstyle, whose skeleton the same player drives.
fn hook_up_skeleton(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut characters: Query<&mut Character>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    let Ok(mut character) = characters.get_mut(ready.entity) else {
        return;
    };
    let Some(armature) = find_armature(ready.entity, &children, &names) else {
        warn!("character model has no Armature node");
        return;
    };
    commands.entity(armature).insert((AnimationPlayer::default(), AnimationTransitions::new()));
    add_animation_targets(&mut commands, armature, armature, &children, &names);
    character.armature = Some(armature);

    let look = &LOOKS[character.index % LOOKS.len()];
    let hair_color = look.hair_color;
    let hair = assets.load(GltfAssetLabel::Scene(0).from_asset(look.hair));
    commands.spawn((WorldAssetRoot(hair), Transform::default(), ChildOf(ready.entity))).observe(
        move |ready: On<WorldInstanceReady>,
              mut commands: Commands,
              children: Query<&Children>,
              names: Query<&Name>,
              meshes: Query<&MeshMaterial3d<StandardMaterial>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            if let Some(hair_armature) = find_armature(ready.entity, &children, &names) {
                add_animation_targets(&mut commands, hair_armature, armature, &children, &names);
            }
            for mesh in meshes.iter_many(children.iter_descendants(ready.entity)) {
                if let Some(mut material) = materials.get_mut(&mesh.0) {
                    material.base_color = hair_color;
                }
            }
        },
    );
}

fn find_armature(root: Entity, children: &Query<&Children>, names: &Query<&Name>) -> Option<Entity> {
    children
        .iter_descendants(root)
        .find(|&entity| names.get(entity).is_ok_and(|name| name.as_str() == "Armature"))
}

/// Gives every node from `armature` down the animation ID Bevy's glTF loader
/// would: one built from the node names on the path down from the armature.
/// The library's animations use the same paths, so each finds its bone.
/// `player` is the entity whose `AnimationPlayer` drives them.
fn add_animation_targets(
    commands: &mut Commands,
    armature: Entity,
    player: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
) {
    let mut pending = vec![(armature, vec![names.get(armature).unwrap().clone()])];
    while let Some((entity, path)) = pending.pop() {
        commands.entity(entity).insert((AnimationTargetId::from_names(path.iter()), AnimatedBy(player)));
        for &child in children.get(entity).into_iter().flatten() {
            if let Ok(name) = names.get(child) {
                let mut child_path = path.clone();
                child_path.push(name.clone());
                pending.push((child, child_path));
            }
        }
    }
}

fn react_to_events(mut events: MessageReader<SimEvent>, game: Res<Match>, mut characters: Query<&mut Character>) {
    for SimEvent(event) in events.read() {
        for mut character in &mut characters {
            let team = game.current.players[character.index].team;
            let clip = match *event {
                Event::Touched { player, kind } if player == character.index => Some(match kind {
                    HitKind::Serve => Clip::Serve,
                    HitKind::Pass | HitKind::Lob => Clip::Pass,
                    HitKind::Spike => Clip::Spike,
                }),
                Event::Point { team: winner, .. } if winner == team => Some(Clip::Celebrate),
                _ => None,
            };
            if let Some(clip) = clip {
                character.action = Some(clip);
                character.restart = true;
            }
        }
    }
}

/// Ground speed over the last tick, in meters per second.
fn ground_velocity(game: &Match, index: usize) -> Vec2 {
    if game.previous.rally != game.current.rally {
        return Vec2::ZERO;
    }
    let delta = game.current.players[index].position - game.previous.players[index].position;
    Vec2::new(delta.x, delta.z) / DT
}

/// Moves each model to its player and turns it: toward where it's running, or
/// toward the ball when standing still.
fn place_characters(
    game: Res<Match>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut characters: Query<(&mut Character, &mut Transform)>,
) {
    let ball = game.current.ball_position();
    for (mut character, mut transform) in &mut characters {
        let feet = player_feet(&game, &fixed, character.index);
        let velocity = ground_velocity(&game, character.index);
        let facing = if velocity.length() > JOG_SPEED {
            velocity
        } else {
            Vec2::new(ball.x - feet.x, ball.z - feet.z)
        };
        if facing.length() > 0.1 {
            let wanted = facing.x.atan2(facing.y);
            let turn = (wanted - character.yaw + PI).rem_euclid(TAU) - PI;
            let max_turn = TURN_SPEED * time.delta_secs();
            character.yaw += turn.clamp(-max_turn, max_turn);
        }
        *transform = Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(character.yaw));
    }
}

fn animate_characters(
    game: Res<Match>,
    animations: Res<Animations>,
    mut commands: Commands,
    mut characters: Query<&mut Character>,
    mut rigs: Query<(&mut AnimationPlayer, &mut AnimationTransitions, Has<AnimationGraphHandle>)>,
) {
    for mut character in &mut characters {
        let Some(armature) = character.armature else {
            continue;
        };
        let Ok((mut player, mut transitions, has_graph)) = rigs.get_mut(armature) else {
            continue;
        };
        if !has_graph {
            commands.entity(armature).insert(AnimationGraphHandle(animations.graph.clone()));
        }

        let airborne = !game.current.players[character.index].grounded();
        if airborne != character.airborne {
            character.airborne = airborne;
            if !character.action.is_some_and(Clip::is_game_action) {
                character.action = Some(if airborne { Clip::JumpStart } else { Clip::Land });
                character.restart = true;
            }
        }

        if let Some(action) = character.action
            && !character.restart
        {
            let finished = if action == Clip::Celebrate {
                game.current.phase == Phase::Rally
            } else {
                player.animation(animations.nodes[&action]).is_none_or(|active| active.is_finished())
            };
            if finished {
                character.action = None;
            }
        }

        let speed = ground_velocity(&game, character.index).length();
        let movement = if airborne {
            Clip::Airborne
        } else if speed > SPRINT_SPEED {
            Clip::Sprint
        } else if speed > JOG_SPEED {
            Clip::Jog
        } else {
            Clip::Idle
        };
        let clip = character.action.unwrap_or(movement);
        if character.playing != Some(clip) || character.restart {
            let active = transitions.play(&mut player, animations.nodes[&clip], BLEND);
            if clip.looping() {
                active.repeat();
            }
            character.playing = Some(clip);
            character.restart = false;
        }
    }
}

/// A ring in team color under every player, doubled under your own.
fn draw_team_markers(game: Res<Match>, characters: Query<(&Character, &Transform)>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let local = game.current.player_index(LOCAL_TEAM, 0);
    for (character, transform) in &characters {
        let color = TEAM_COLORS[game.current.players[character.index].team];
        let at = transform.translation.with_y(0.03);
        gizmos.circle(Isometry3d::new(at, flat), 0.55, color);
        if character.index == local {
            gizmos.circle(Isometry3d::new(at, flat), 0.65, color);
        }
    }
}
