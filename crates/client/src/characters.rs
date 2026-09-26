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
use bevy::transform::TransformSystems;
use bevy::world_serialization::WorldInstanceReady;
use volley_sim::{Ball, DT, Event, HitKind, HitRequest, Phase, court};

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
        )
        // Overrides the animated arms, so it runs once the animation has been applied.
        .add_systems(PostUpdate, raise_arms_to_block.after(TransformSystems::Propagate));
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
    Dive,
    Celebrate,
}

/// Timing inside a hit animation, in seconds of the clip.
struct Swing {
    /// The end of the wind-up. A pressed hit holds here until the ball arrives.
    wind_up: f32,
    /// Where the hand meets the ball. A hit jumps here the moment it happens.
    contact: f32,
}

impl Clip {
    const ALL: [Clip; 11] = [
        Clip::Idle,
        Clip::Jog,
        Clip::Sprint,
        Clip::JumpStart,
        Clip::Airborne,
        Clip::Land,
        Clip::Pass,
        Clip::Spike,
        Clip::Serve,
        Clip::Dive,
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
            // One arm sweeps up and forward from the waist.
            Clip::Pass => "Spell_Simple_Enter",
            // Hand high and back, then down across the body.
            Clip::Spike => "Sword_Attack",
            Clip::Serve => "Punch_Cross",
            // Launches forward, hits the floor, rolls back up.
            Clip::Dive => "Roll",
            Clip::Celebrate => "Dance_Loop",
        }
    }

    fn looping(self) -> bool {
        matches!(self, Clip::Idle | Clip::Jog | Clip::Sprint | Clip::Airborne | Clip::Celebrate)
    }

    /// Playback speed. The landing and roll are sped up to fit the game's quicker jumps and dives.
    fn speed(self) -> f32 {
        match self {
            Clip::Land | Clip::Dive => 1.6,
            Clip::Spike => 1.2,
            _ => 1.0,
        }
    }

    /// Timings measured from the clips' hand positions.
    fn swing(self) -> Option<Swing> {
        match self {
            Clip::Pass => Some(Swing { wind_up: 0.2, contact: 0.4 }),
            Clip::Spike => Some(Swing { wind_up: 0.26, contact: 0.35 }),
            Clip::Serve => Some(Swing { wind_up: 0.17, contact: 0.25 }),
            _ => None,
        }
    }

    /// Moves started by the game rather than by running and jumping. Takeoffs and
    /// landings never cut these short.
    fn is_game_action(self) -> bool {
        matches!(self, Clip::Pass | Clip::Spike | Clip::Serve | Clip::Dive | Clip::Celebrate)
    }
}

/// How fast a wind-up plays on its way to the held pose.
const WIND_UP_SPEED: f32 = 2.0;
/// How long a player keeps facing where they sent the ball, or where they dove.
const FACE_SECONDS: f32 = 0.6;

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
    /// Where to start `action` when it next plays, in clip seconds.
    seek: Option<f32>,
    /// A pressed hit is winding up or holding, waiting for the ball.
    winding_up: bool,
    /// The held hit connected: jump to contact and follow through.
    release: bool,
    airborne: bool,
    /// Facing, as a rotation about the vertical axis. 0 faces +z.
    yaw: f32,
    /// A direction to face instead of the usual, until a time (`Time::elapsed_secs`).
    face: Option<(Vec2, f32)>,
    /// Each arm's upper arm, forearm and hand bones, for posing arms in code.
    arms: Vec<[Entity; 3]>,
    /// How far the arms are raised for a block, 0 to 1.
    arms_up: f32,
}

impl Character {
    fn new(index: usize, yaw: f32) -> Self {
        Self {
            index,
            armature: None,
            playing: None,
            action: None,
            restart: false,
            seek: None,
            winding_up: false,
            release: false,
            airborne: false,
            yaw,
            face: None,
            arms: Vec::new(),
            arms_up: 0.0,
        }
    }

    fn start(&mut self, clip: Clip, seek: Option<f32>) {
        self.action = Some(clip);
        self.restart = true;
        self.seek = seek;
        self.winding_up = false;
        self.release = false;
    }
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
                Character::new(index, yaw),
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
    let bone = |name: &str| {
        children.iter_descendants(armature).find(|&entity| names.get(entity).is_ok_and(|n| n.as_str() == name))
    };
    character.arms = [["upperarm_l", "lowerarm_l", "hand_l"], ["upperarm_r", "lowerarm_r", "hand_r"]]
        .into_iter()
        .filter_map(|[upper, lower, hand]| Some([bone(upper)?, bone(lower)?, bone(hand)?]))
        .collect();

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

fn react_to_events(
    mut events: MessageReader<SimEvent>,
    game: Res<Match>,
    time: Res<Time>,
    mut characters: Query<&mut Character>,
) {
    let face_until = time.elapsed_secs() + FACE_SECONDS;
    for SimEvent(event) in events.read() {
        for mut character in &mut characters {
            let me = character.index;
            match *event {
                Event::Touched { player, kind } if player == me => {
                    if let Ball::InFlight(flight) = game.current.ball {
                        character.face = Some((Vec2::new(flight.velocity.x, flight.velocity.z), face_until));
                    }
                    let clip = match kind {
                        HitKind::Serve => Clip::Serve,
                        HitKind::Pass | HitKind::Lob => Clip::Pass,
                        HitKind::Spike => Clip::Spike,
                        // Digs happen mid-dive; the roll keeps playing.
                        HitKind::Dig => continue,
                    };
                    if character.winding_up && character.action == Some(clip) {
                        character.release = true;
                    } else {
                        character.start(clip, clip.swing().map(|swing| swing.contact));
                    }
                }
                Event::Dove { player } if player == me => {
                    if let Some(dive) = game.current.players[me].dive {
                        character.face = Some((dive.direction, face_until));
                    }
                    character.start(Clip::Dive, None);
                }
                Event::Point { team, .. } if team == game.current.players[me].team => {
                    character.start(Clip::Celebrate, None);
                }
                _ => {}
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

/// Moves each model to its player and turns it: toward where it just sent the
/// ball or dove, else toward where it's running, else toward the ball.
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
        let to_ball = Vec2::new(ball.x - feet.x, ball.z - feet.z);
        let facing = match character.face {
            Some((direction, until)) if time.elapsed_secs() < until => direction,
            _ if character.winding_up => to_ball,
            // Square to the net, hands up.
            _ if game.current.players[character.index].blocking() => {
                Vec2::new(-court::side(game.current.players[character.index].team), 0.0)
            }
            _ if velocity.length() > JOG_SPEED => velocity,
            _ => to_ball,
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

        let me = &game.current.players[character.index];
        let airborne = !me.grounded();
        if airborne != character.airborne {
            character.airborne = airborne;
            // A block goes straight to hands up, without the takeoff crouch.
            if !character.action.is_some_and(Clip::is_game_action) && !me.blocking() {
                character.start(if airborne { Clip::JumpStart } else { Clip::Land }, None);
            }
        }

        // A hit button was just pressed: wind up and wait for the ball. Serves
        // happen on the press itself, so they skip this.
        let was_pressed = game.previous.players[character.index].pending_hit(game.previous.tick).is_some();
        let swing = match me.pending_hit(game.current.tick) {
            _ if was_pressed || me.dive.is_some() || game.current.ball == Ball::Held { by: character.index } => None,
            Some(HitRequest::Pass) => Some(Clip::Pass),
            Some(HitRequest::Spike) if airborne => Some(Clip::Spike),
            _ => None,
        };
        if let Some(clip) = swing
            && !character.action.is_some_and(Clip::is_game_action)
        {
            character.start(clip, None);
            character.winding_up = true;
        }

        let speed = ground_velocity(&game, character.index).length();
        if let Some(action) = character.action
            && !character.restart
        {
            let finished = match action {
                Clip::Celebrate => game.current.phase == Phase::Rally,
                // Running cuts a landing short, so it never slows you down.
                Clip::Land if speed > JOG_SPEED => true,
                _ if character.winding_up => false,
                _ => player.animation(animations.nodes[&action]).is_none_or(|active| active.is_finished()),
            };
            if finished {
                character.action = None;
            }
        }

        if character.winding_up
            && !character.restart
            && let Some(action) = character.action
            && let Some(swing) = action.swing()
            && let Some(active) = player.animation_mut(animations.nodes[&action])
        {
            if character.release {
                // Connected: jump to the contact frame and follow through.
                active.seek_to(swing.contact).set_speed(action.speed()).resume();
                character.winding_up = false;
            } else if me.pending_hit(game.current.tick).is_none() {
                // Nothing to hit: swing through anyway.
                active.set_speed(action.speed()).resume();
                character.winding_up = false;
            } else if active.seek_time() >= swing.wind_up {
                active.pause();
            }
        }

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
            // Restarting doesn't un-pause, and a held wind-up may have paused this clip.
            let active = transitions.play(&mut player, animations.nodes[&clip], BLEND).resume();
            active.set_speed(if character.winding_up { WIND_UP_SPEED } else { clip.speed() });
            if let Some(seek) = character.seek.take() {
                active.seek_to(seek);
            }
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

/// How quickly arms go up for a block and come back down, per second.
const ARMS_UP_SPEED: f32 = 12.0;

/// The free animation library has no block, so blocking arms are posed in code:
/// after the animation has placed the skeleton, each arm is turned to point up
/// and a little forward over the net, with the elbow straightened. This works
/// on the final bone positions, so everything below the upper arm (forearm,
/// hand, fingers) is repositioned to follow.
fn raise_arms_to_block(
    game: Res<Match>,
    time: Res<Time>,
    mut characters: Query<(&mut Character, &Transform)>,
    locals: Query<&Transform, Without<Character>>,
    children: Query<&Children>,
    mut globals: Query<&mut GlobalTransform>,
) {
    for (mut character, body) in &mut characters {
        let target = if game.current.players[character.index].blocking() { 1.0 } else { 0.0 };
        let step = ARMS_UP_SPEED * time.delta_secs();
        character.arms_up += (target - character.arms_up).clamp(-step, step);
        let weight = character.arms_up;
        if weight <= 0.0 {
            continue;
        }
        let forward = body.rotation * Vec3::Z;
        let up = (Vec3::Y + forward * 0.25).normalize();
        for &[upper, lower, hand] in &character.arms {
            // Turn the upper arm, then the forearm, so each points up: a bone
            // points toward the next one down the arm.
            for (bone, child) in [(upper, lower), (lower, hand)] {
                let (Ok(global), Ok(child_local)) = (globals.get(bone).copied(), locals.get(child)) else {
                    continue;
                };
                let (scale, rotation, translation) = global.to_scale_rotation_translation();
                let pointing = (rotation * child_local.translation).normalize_or_zero();
                let turn = Quat::IDENTITY.slerp(Quat::from_rotation_arc(pointing, up), weight);
                let posed = GlobalTransform::from(Transform { translation, rotation: turn * rotation, scale });
                follow_parent(bone, posed, &locals, &children, &mut globals);
            }
        }
    }
}

/// Sets `entity`'s world transform and recomputes everything below it from
/// their local transforms.
fn follow_parent(
    entity: Entity,
    global: GlobalTransform,
    locals: &Query<&Transform, Without<Character>>,
    children: &Query<&Children>,
    globals: &mut Query<&mut GlobalTransform>,
) {
    if let Ok(mut current) = globals.get_mut(entity) {
        *current = global;
    }
    for &child in children.get(entity).into_iter().flatten() {
        if let Ok(local) = locals.get(child) {
            follow_parent(child, global.mul_transform(*local), locals, children, globals);
        }
    }
}
