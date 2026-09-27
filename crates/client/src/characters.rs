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
use volley_sim::court::BALL_RADIUS;
use volley_sim::moves::CROSS;
use volley_sim::{Ball, DT, Event, HitKind, Kit, MoveId, MovePhase, Passive, Phase, Sim, attack};

use crate::flow::Screen;

use crate::input::LOCAL_TEAM;
use crate::scene::{TEAM_COLORS, player_feet};
use crate::{Match, SimEvent};

/// A body and its hairstyle, rigged to the same skeleton.
pub struct Look {
    body: &'static str,
    /// A separate hair model, and its color: the pack's hair textures are
    /// grey, meant to be colored by a shader, so we tint the material instead.
    hair: Option<(&'static str, Color)>,
}

/// Heroes without their own look yet alternate between these.
const LOOKS: [Look; 2] = [
    Look {
        body: "characters/Superhero_Male_FullBody.gltf",
        hair: Some(("characters/Hair_SimpleParted.gltf", Color::srgb(0.3, 0.18, 0.1))),
    },
    Look {
        body: "characters/Superhero_Female_FullBody.gltf",
        hair: Some(("characters/Hair_Buns.gltf", Color::srgb(0.12, 0.1, 0.09))),
    },
];

/// Cross's stand-in until he has his own model: the male body painted in his
/// kit, fade included (see `tools/blender/paint_cross.py`).
const CROSS_LOOK: Look = Look { body: "characters/Cross.glb", hair: None };

fn look_for(kit: &Kit, index: usize) -> &'static Look {
    if kit.name == CROSS.name { &CROSS_LOOK } else { &LOOKS[index % LOOKS.len()] }
}
/// Quaternius's general library; our volleyball moves made for its skeleton
/// (see `tools/blender/volley_animations.py`); and Mixamo motion capture
/// retargeted onto it (see `tools/blender/retarget_mixamo.py`).
const ANIMATION_LIBRARIES: [&str; 3] = ["animations/UAL1_Standard.glb", "animations/Volley.glb", "animations/Mocap.glb"];
const QUATERNIUS: usize = 0;
const VOLLEY: usize = 1;
const MOCAP: usize = 2;
/// Ground speeds (m/s) the motion-captured jog and sprint look right at; they
/// play faster or slower to match how fast a player really runs.
const JOG_PACE: f32 = 4.0;
const SPRINT_PACE: f32 = 7.0;

/// Blend time into and out of moves, and between running speeds and standing,
/// which differ more and change more often.
const BLEND: Duration = Duration::from_millis(150);
const GAIT_BLEND: Duration = Duration::from_millis(250);
/// Radians per second.
const TURN_SPEED: f32 = 10.0;
/// Ground speeds (m/s) to start jogging or sprinting, and lower ones to stop, so
/// a player hovering around one speed doesn't flicker between animations.
const JOG_SPEED: f32 = 0.8;
const STOP_JOG_SPEED: f32 = 0.3;
const SPRINT_SPEED: f32 = 4.5;
const STOP_SPRINT_SPEED: f32 = 3.5;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, (load_animation_libraries, spawn_characters))
        .add_systems(OnEnter(Screen::Playing), spawn_characters)
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
        .add_systems(PostUpdate, pose_limbs.after(TransformSystems::Propagate));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Clip {
    Idle,
    Jog,
    Sprint,
    /// Just left the ground, arms driving up.
    Takeoff,
    Airborne,
    Land,
    Dash,
    /// Knees bent, ready to move: standing still during a rally.
    Ready,
    Bump,
    Set,
    Spike,
    VolleyKick,
    BicycleKick,
    Serve,
    Block,
    Dive,
    FootSave,
    /// Cross carrying the ball across his body, shifting left or right.
    Crossover,
    CrossoverRight,
    Dunk,
    /// Flattened by a dunk through the block.
    KnockedDown,
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
    const ALL: [Clip; 22] = [
        Clip::Idle,
        Clip::Jog,
        Clip::Sprint,
        Clip::Takeoff,
        Clip::Airborne,
        Clip::Land,
        Clip::Dash,
        Clip::Ready,
        Clip::Bump,
        Clip::Set,
        Clip::Spike,
        Clip::VolleyKick,
        Clip::BicycleKick,
        Clip::Serve,
        Clip::Block,
        Clip::Dive,
        Clip::FootSave,
        Clip::Crossover,
        Clip::CrossoverRight,
        Clip::Dunk,
        Clip::KnockedDown,
        Clip::Celebrate,
    ];

    /// The library the animation is in, and its name there.
    fn source(self) -> (usize, &'static str) {
        match self {
            Clip::Idle => (QUATERNIUS, "Idle_Loop"),
            Clip::Jog => (MOCAP, "Jog"),
            Clip::Sprint => (MOCAP, "Sprint"),
            Clip::Airborne => (MOCAP, "Airborne"),
            Clip::Land => (MOCAP, "Land"),
            Clip::Takeoff => (VOLLEY, "Takeoff"),
            Clip::Dash => (MOCAP, "Dash"),
            Clip::Ready => (MOCAP, "Ready"),
            Clip::Bump => (VOLLEY, "Bump"),
            Clip::Set => (VOLLEY, "Set"),
            Clip::Spike => (VOLLEY, "Spike"),
            Clip::VolleyKick => (VOLLEY, "Volley_Kick"),
            Clip::BicycleKick => (VOLLEY, "Bicycle_Kick"),
            Clip::Serve => (VOLLEY, "Serve"),
            Clip::Block => (VOLLEY, "Block"),
            Clip::Dive => (VOLLEY, "Dive"),
            Clip::FootSave => (VOLLEY, "Foot_Save"),
            Clip::Crossover => (VOLLEY, "Crossover"),
            Clip::CrossoverRight => (VOLLEY, "Crossover_Right"),
            Clip::Dunk => (VOLLEY, "Dunk"),
            Clip::KnockedDown => (MOCAP, "Knocked_Down"),
            Clip::Celebrate => (MOCAP, "Celebrate"),
        }
    }

    fn looping(self) -> bool {
        matches!(self, Clip::Idle | Clip::Ready | Clip::Jog | Clip::Sprint | Clip::Airborne | Clip::Celebrate)
    }

    /// Playback speed. Motion capture happens at human speed, a little slow for
    /// the game's jumps, dashes and knockdowns; our own clips are timed for it.
    fn speed(self) -> f32 {
        match self {
            Clip::Land | Clip::Dash => 1.3,
            Clip::KnockedDown => 1.6,
            _ => 1.0,
        }
    }

    /// Where to start playing, in clip seconds, skipping motion capture's lead-in:
    /// the landing's fall, the dash's first shift of weight, the knockdown's stagger.
    fn start_at(self) -> f32 {
        match self {
            Clip::Land => 0.33,
            Clip::Dash => 0.1,
            Clip::KnockedDown => 0.35,
            _ => 0.0,
        }
    }

    /// Timings of the hits, as authored in `tools/blender/volley_animations.py`.
    fn swing(self) -> Option<Swing> {
        match self {
            Clip::Bump | Clip::Set => Some(Swing { wind_up: 0.15, contact: 0.25 }),
            Clip::Spike => Some(Swing { wind_up: 0.24, contact: 0.32 }),
            Clip::VolleyKick => Some(Swing { wind_up: 0.1, contact: 0.18 }),
            Clip::BicycleKick => Some(Swing { wind_up: 0.12, contact: 0.24 }),
            Clip::Serve => Some(Swing { wind_up: 0.0, contact: 0.22 }),
            Clip::Dunk => Some(Swing { wind_up: 0.3, contact: 0.4 }),
            _ => None,
        }
    }

    /// The attack clip for a technique.
    fn attack(kind: HitKind) -> Clip {
        match kind {
            HitKind::Volley => Clip::VolleyKick,
            HitKind::Bicycle => Clip::BicycleKick,
            _ => Clip::Spike,
        }
    }

    fn is_attack(self) -> bool {
        matches!(self, Clip::Spike | Clip::VolleyKick | Clip::BicycleKick)
    }

    /// Moves started by the game rather than by running and jumping. Takeoffs and
    /// landings never cut these short.
    fn is_game_action(self) -> bool {
        !matches!(
            self,
            Clip::Idle | Clip::Ready | Clip::Jog | Clip::Sprint | Clip::Takeoff | Clip::Airborne | Clip::Land | Clip::Dash
        )
    }
}

/// A pass meets balls above this height (over the feet) with a set, lower ones with a bump.
const SET_HEIGHT: f32 = 1.45;
/// How far ahead to look at the ball when picking how a hit will meet it.
const LOOKAHEAD_TICKS: u32 = 8;

/// How `player` will meet the ball with the hit it's winding up: the clip for
/// where the ball will be in a moment.
fn hit_clip(sim: &Sim, player: usize, id: MoveId) -> Clip {
    let me = &sim.players[player];
    let ball = match sim.ball {
        Ball::InFlight(flight) => flight.position_at(sim.tick + LOOKAHEAD_TICKS),
        _ => sim.ball_position(),
    };
    match id {
        MoveId::Spike => Clip::attack(attack::best_technique(me, ball).0),
        // Cocked to spike, until the ball is caught.
        MoveId::Crossover => Clip::Spike,
        MoveId::Posterizer => Clip::Dunk,
        _ if ball.y - me.position.y > SET_HEIGHT => Clip::Set,
        _ => Clip::Bump,
    }
}

/// How fast a wind-up plays on its way to the held pose, and how fast a held
/// hit plays through to contact once it connects, to catch up with the ball.
const WIND_UP_SPEED: f32 = 2.0;
const CATCH_UP_SPEED: f32 = 2.5;
/// How long a player keeps facing where they sent the ball, or where they dove.
const FACE_SECONDS: f32 = 0.6;

#[derive(Resource)]
struct AnimationLibraries(Vec<Handle<Gltf>>);

#[derive(Resource)]
struct Animations {
    graph: Handle<AnimationGraph>,
    nodes: HashMap<Clip, AnimationNodeIndex>,
}

/// The model showing player `index`.
#[derive(Component)]
struct Character {
    index: usize,
    look: &'static Look,
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
    /// The held hit connected: play through to contact and follow through.
    release: bool,
    /// Playing fast from a held wind-up until this contact time, in clip seconds.
    catch_up_to: Option<f32>,
    /// Running, jogging or standing, as last chosen.
    gait: Clip,
    airborne: bool,
    /// Facing, as a rotation about the vertical axis. 0 faces +z.
    yaw: f32,
    /// A direction to face instead of the usual, until a time (`Time::elapsed_secs`).
    face: Option<(Vec2, f32)>,
    /// Each arm's upper arm, forearm and hand bones, for posing arms in code.
    arms: Vec<[Entity; 3]>,
    /// The ball posed arms reach for, and how strongly (0 to 1) they're posed.
    arm_goal: Vec3,
    arm_weight: f32,
    /// The right leg's thigh, calf and foot bones, for foot saves.
    leg: Option<[Entity; 3]>,
    /// Where the posed leg points, and how strongly (0 to 1) it's posed.
    leg_goal: Vec3,
    leg_weight: f32,
}

impl Character {
    fn new(index: usize, yaw: f32, look: &'static Look) -> Self {
        Self {
            index,
            look,
            armature: None,
            playing: None,
            action: None,
            restart: false,
            seek: None,
            winding_up: false,
            release: false,
            catch_up_to: None,
            gait: Clip::Idle,
            airborne: false,
            yaw,
            face: None,
            arms: Vec::new(),
            arm_goal: Vec3::ZERO,
            arm_weight: 0.0,
            leg: None,
            leg_goal: Vec3::ZERO,
            leg_weight: 0.0,
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

fn load_animation_libraries(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(AnimationLibraries(ANIMATION_LIBRARIES.iter().map(|path| assets.load(*path)).collect()));
}

fn build_animation_graph(
    mut commands: Commands,
    libraries: Res<AnimationLibraries>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Some(loaded) = libraries.0.iter().map(|handle| gltfs.get(handle)).collect::<Option<Vec<_>>>() else {
        return;
    };
    let mut graph = AnimationGraph::new();
    let nodes = Clip::ALL
        .into_iter()
        .map(|clip| {
            let (library, name) = clip.source();
            let handle = loaded[library].named_animations.get(name).unwrap_or_else(|| {
                panic!("{} has no animation named {name}", ANIMATION_LIBRARIES[library])
            });
            (clip, graph.add_clip(handle.clone(), 1.0, graph.root))
        })
        .collect();
    commands.insert_resource(Animations { graph: graphs.add(graph), nodes });
}

/// Spawns everyone's model, dressed as their hero, replacing any from before
/// (each match's heroes are picked anew).
fn spawn_characters(mut commands: Commands, assets: Res<AssetServer>, game: Res<Match>, existing: Query<Entity, With<Character>>) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    for (index, player) in game.current.players.iter().enumerate() {
        let look = look_for(&player.kit, index);
        let model = assets.load(GltfAssetLabel::Scene(0).from_asset(look.body));
        // Start out facing the net.
        let yaw = -player.side * FRAC_PI_2;
        commands
            .spawn((
                Character::new(index, yaw, look),
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
    character.leg = (|| Some([bone("thigh_r")?, bone("calf_r")?, bone("foot_r")?]))();

    let Some((hair, hair_color)) = character.look.hair else {
        return;
    };
    let hair = assets.load(GltfAssetLabel::Scene(0).from_asset(hair));
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
                Event::Touched { player, kind, .. } if player == me => {
                    // A no-look hitter keeps looking where they were.
                    let no_look = game.current.players[me].kit.has(Passive::NoLook);
                    if let Ball::InFlight(flight) = game.current.ball
                        && !no_look
                    {
                        character.face = Some((Vec2::new(flight.velocity.x, flight.velocity.z), face_until));
                    }
                    let clip = match kind {
                        HitKind::Serve => Clip::Serve,
                        // Whichever of bump or set was winding up; otherwise by the ball's height.
                        HitKind::Pass | HitKind::Lob => match character.action {
                            Some(clip @ (Clip::Bump | Clip::Set)) if character.winding_up => clip,
                            _ => hit_clip(&game.current, me, MoveId::Pass),
                        },
                        HitKind::Spike | HitKind::Volley | HitKind::Bicycle => {
                            // A bicycle kick faces away from where the ball goes.
                            if kind == HitKind::Bicycle
                                && let Some((direction, until)) = character.face
                            {
                                character.face = Some((-direction, until));
                            }
                            Clip::attack(kind)
                        }
                        HitKind::Dunk => Clip::Dunk,
                        // Digs and kicks happen mid-move, which keeps playing.
                        HitKind::Dig | HitKind::Kick => continue,
                    };
                    if character.winding_up && character.action == Some(clip) {
                        character.release = true;
                    } else {
                        character.start(clip, clip.swing().map(|swing| swing.contact));
                    }
                }
                Event::MoveStarted { player, id } if player == me => {
                    let body = &game.current.players[me];
                    match id {
                        MoveId::Dive | MoveId::FootSave => {
                            if let Some(action) = body.action {
                                character.face = Some((action.direction, face_until));
                            }
                            character.start(if id == MoveId::Dive { Clip::Dive } else { Clip::FootSave }, None);
                        }
                        // A pressed hit winds up and waits for the ball. Serves
                        // happen on the press itself, so they skip this.
                        MoveId::Pass | MoveId::Spike | MoveId::Crossover | MoveId::Posterizer => {
                            let serving = game.current.ball == Ball::Held { by: me };
                            if !serving && !character.action.is_some_and(Clip::is_game_action) {
                                character.start(hit_clip(&game.current, me, id), None);
                                character.winding_up = true;
                            }
                        }
                    }
                }
                Event::Carried { player } if player == me => {
                    let body = &game.current.players[me];
                    let shift = body.carry.map(|carry| carry.velocity).unwrap_or_default();
                    // Facing +z, the right hand is toward -x.
                    let right = Vec2::new(-character.yaw.cos(), character.yaw.sin());
                    character.start(if shift.dot(right) > 0.0 { Clip::CrossoverRight } else { Clip::Crossover }, None);
                }
                Event::Posterized { player } if player == me => character.start(Clip::KnockedDown, None),
                Event::Dashed { player } if player == me => {
                    if let Some(dash) = game.current.players[me].dash {
                        character.face = Some((dash.direction, face_until));
                    }
                    if !character.action.is_some_and(Clip::is_game_action) {
                        character.start(Clip::Dash, None);
                    }
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
                Vec2::new(-game.current.players[character.index].side, 0.0)
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

        let sim = &game.current;
        let me = &sim.players[character.index];
        let airborne = !me.grounded();
        if airborne != character.airborne {
            character.airborne = airborne;
            if !character.action.is_some_and(Clip::is_game_action) && !me.blocking() {
                character.start(if airborne { Clip::Takeoff } else { Clip::Land }, None);
            }
        }
        if me.blocking() && character.action != Some(Clip::Block) {
            character.start(Clip::Block, None);
        }
        // Waiting to serve: ball up in the hand, arm cocked.
        let serving = sim.ball == (Ball::Held { by: character.index });
        if serving && character.action.is_none() {
            character.start(Clip::Serve, None);
            character.winding_up = true;
        }
        // An attack winding up switches technique as the ball comes in.
        if character.winding_up
            && let Some(clip) = character.action.filter(|clip| clip.is_attack())
            && me.active_move(sim.tick) == Some(MoveId::Spike)
        {
            let wanted = hit_clip(sim, character.index, MoveId::Spike);
            if wanted != clip {
                character.start(wanted, None);
                character.winding_up = true;
            }
        }

        let speed = ground_velocity(&game, character.index).length();
        if let Some(action) = character.action
            && !character.restart
        {
            let finished = match action {
                Clip::Celebrate => game.current.phase == Phase::Rally,
                Clip::FootSave => !me.action.is_some_and(|action| action.id == MoveId::FootSave),
                // Hands stay up until landing.
                Clip::Block => !me.blocking(),
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
                // Connected: hurry through to contact, then follow through.
                active.set_speed(CATCH_UP_SPEED);
                character.catch_up_to = Some(swing.contact);
                character.winding_up = false;
            } else if if action == Clip::Serve { !serving } else { me.active_move(sim.tick).is_none() } {
                // Nothing to hit: swing through anyway.
                active.set_speed(action.speed());
                character.winding_up = false;
            } else if active.seek_time() >= swing.wind_up {
                // Hold with speed zero rather than pausing: Bevy only blends
                // out of an animation that isn't paused.
                active.set_speed(0.0);
            }
        }
        if let Some(contact) = character.catch_up_to
            && let Some(action) = character.action
            && let Some(active) = player.animation_mut(animations.nodes[&action])
            && active.seek_time() >= contact
        {
            active.set_speed(action.speed());
            character.catch_up_to = None;
        }

        character.gait = match character.gait {
            Clip::Sprint if speed > STOP_SPRINT_SPEED => Clip::Sprint,
            _ if speed > SPRINT_SPEED => Clip::Sprint,
            Clip::Jog | Clip::Sprint if speed > STOP_JOG_SPEED => Clip::Jog,
            _ if speed > JOG_SPEED => Clip::Jog,
            _ if sim.phase == Phase::Rally => Clip::Ready,
            _ => Clip::Idle,
        };
        let movement = if airborne { Clip::Airborne } else { character.gait };
        let clip = character.action.unwrap_or(movement);
        if character.playing != Some(clip) || character.restart {
            let changing_gait = !clip.is_game_action() && !character.playing.is_some_and(Clip::is_game_action);
            let blend = if changing_gait { GAIT_BLEND } else { BLEND };
            character.catch_up_to = None;
            let active = transitions.play(&mut player, animations.nodes[&clip], blend);
            active.set_speed(if character.winding_up { WIND_UP_SPEED } else { clip.speed() });
            let seek = character.seek.take().unwrap_or(clip.start_at());
            if seek > 0.0 {
                active.seek_to(seek);
            }
            if clip.looping() {
                active.repeat();
            }
            character.playing = Some(clip);
            character.restart = false;
        }
        // Running clips keep pace with the feet.
        if let Some(pace) = match clip {
            Clip::Jog => Some(JOG_PACE),
            Clip::Sprint => Some(SPRINT_PACE),
            _ => None,
        } && let Some(active) = player.animation_mut(animations.nodes[&clip])
        {
            active.set_speed((speed / pace).clamp(0.7, 1.8));
        }
    }
}

/// A ring in team color under every player, doubled under your own.
fn draw_team_markers(game: Res<Match>, time: Res<Time>, characters: Query<(&Character, &Transform)>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let local = game.current.player_index(LOCAL_TEAM, 0);
    for (character, transform) in &characters {
        let player = &game.current.players[character.index];
        let color = TEAM_COLORS[player.team];
        let at = transform.translation.with_y(0.03);
        gizmos.circle(Isometry3d::new(at, flat), 0.55, color);
        if character.index == local {
            gizmos.circle(Isometry3d::new(at, flat), 0.65, color);
        }
        // Ultimate ready: a gold ring that breathes.
        if player.charge >= 1.0 {
            let pulse = 0.8 + 0.08 * (time.elapsed_secs() * 5.0).sin();
            gizmos.circle(Isometry3d::new(at, flat), pulse, Color::srgb(1.0, 0.8, 0.15));
        }
    }
}

/// How quickly posed limbs blend in and out, per second.
const ARM_BLEND_SPEED: f32 = 12.0;
/// Arms start reaching for a ball this close to the chest, and reach fully
/// once it's within `FULL_REACH`.
const REACH_START: f32 = 2.6;
const FULL_REACH: f32 = 1.2;
const CHEST_HEIGHT: f32 = 1.3;
/// How far a reach bends the animated arms toward the ball: enough to meet it,
/// not so much the bump or set loses its shape.
const REACH_WEIGHT: f32 = 0.6;
/// A foot save's leg reaches for a ball this close; otherwise it kicks straight out.
const KICK_REACH_START: f32 = 2.5;

/// Where a bump, set or spike's arms should reach: toward the ball, and how
/// strongly (0 to 1).
fn arm_goal(sim: &Sim, index: usize, clip: Option<Clip>) -> Option<(Vec3, f32)> {
    let me = &sim.players[index];
    if !matches!(clip, Some(Clip::Bump | Clip::Set | Clip::Spike)) || !matches!(sim.ball, Ball::InFlight(_)) {
        return None;
    }
    let ball = sim.ball_position();
    if me.side * ball.x < -BALL_RADIUS || sim.must_not_touch(index) {
        return None;
    }
    let distance = ball.distance(me.position + Vec3::Y * CHEST_HEIGHT);
    let closeness = ((REACH_START - distance) / (REACH_START - FULL_REACH)).clamp(0.0, 1.0);
    (closeness > 0.0).then_some((ball, closeness * REACH_WEIGHT))
}

/// Animations are made for a ball in one spot; this bends the limbs toward the
/// real one. After the animation has placed the skeleton, the arms of a bump,
/// set or spike turn toward the ball as it comes in, and a foot save stretches
/// its leg out to it. This works on the final bone positions, so everything
/// below a turned bone (forearm, hand, fingers; calf, foot) follows.
fn pose_limbs(
    game: Res<Match>,
    time: Res<Time>,
    mut characters: Query<&mut Character>,
    locals: Query<&Transform, Without<Character>>,
    children: Query<&Children>,
    mut globals: Query<&mut GlobalTransform>,
) {
    let sim = &game.current;
    for mut character in &mut characters {
        let step = ARM_BLEND_SPEED * time.delta_secs();

        // Keep the last goal while blending out, so arms ease back from where they were.
        let target = match arm_goal(sim, character.index, character.action) {
            Some((ball, strength)) => {
                character.arm_goal = ball;
                strength
            }
            None => 0.0,
        };
        character.arm_weight += (target - character.arm_weight).clamp(-step, step);
        if character.arm_weight > 0.0 {
            let ball = character.arm_goal;
            let spike = character.action == Some(Clip::Spike);
            for (side, &arm) in character.arms.iter().enumerate() {
                // Arms are stored left, right; a spike reaches with only the right.
                if spike && side == 0 {
                    continue;
                }
                point_limb(arm, |joint| (ball - joint).normalize_or_zero(), character.arm_weight, &locals, &children, &mut globals);
            }
        }

        // A foot save stretches the right leg out toward the ball.
        let me = &sim.players[character.index];
        let kicking = me.action.filter(|action| action.id == MoveId::FootSave && action.phase(sim.tick).is_some());
        if let Some(action) = kicking {
            let ball = sim.ball_position();
            let near_and_low = ball.distance(me.position) < KICK_REACH_START && ball.y - me.position.y < 1.2;
            let extended = me.position + Vec3::new(action.direction.x, 0.0, action.direction.y) * 1.8 + Vec3::Y * 0.2;
            character.leg_goal = if near_and_low && action.phase(sim.tick) != Some(MovePhase::Recovery) { ball } else { extended };
        }
        let target = if kicking.is_some() { 1.0 } else { 0.0 };
        character.leg_weight += (target - character.leg_weight).clamp(-step, step);
        if character.leg_weight > 0.0
            && let Some(leg) = character.leg
        {
            let goal = character.leg_goal;
            point_limb(leg, |joint| (goal - joint).normalize_or_zero(), character.leg_weight, &locals, &children, &mut globals);
        }
    }
}

/// Turns a limb, given as its upper bone, lower bone and end bone, so each
/// bone points where `aim` says from its joint, blended in by `weight`: a bone
/// points toward the next one down the limb.
fn point_limb(
    [upper, lower, end]: [Entity; 3],
    aim: impl Fn(Vec3) -> Vec3,
    weight: f32,
    locals: &Query<&Transform, Without<Character>>,
    children: &Query<&Children>,
    globals: &mut Query<&mut GlobalTransform>,
) {
    for (bone, child) in [(upper, lower), (lower, end)] {
        let (Ok(global), Ok(child_local)) = (globals.get(bone).copied(), locals.get(child)) else {
            continue;
        };
        let (scale, rotation, translation) = global.to_scale_rotation_translation();
        let pointing = (rotation * child_local.translation).normalize_or_zero();
        let turn = Quat::IDENTITY.slerp(Quat::from_rotation_arc(pointing, aim(translation)), weight);
        let posed = GlobalTransform::from(Transform { translation, rotation: turn * rotation, scale });
        follow_parent(bone, posed, locals, children, globals);
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
