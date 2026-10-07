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
use bevy::app::AnimationSystems;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::world_serialization::WorldInstanceReady;
use volley_sim::court::BALL_RADIUS;
use volley_sim::moves::{CROSS, GOLAZO, HEROES};
use volley_sim::{Ball, DT, Event, HitKind, Kit, MoveId, MovePhase, Passive, Phase, Sim, attack};

use crate::feel::HitStop;
use crate::inertia::{Blending, Inertia};
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

/// Heroes' own models, made from their concept art with an image-to-3D tool
/// and rigged to the shared skeleton by `tools/blender/rig_hero.py`.
const CROSS_LOOK: Look = Look { body: "characters/Cross.glb", hair: None };
const GOLAZO_LOOK: Look = Look { body: "characters/Golazo.glb", hair: None };

fn look_for(kit: &Kit, index: usize) -> &'static Look {
    match kit.name {
        name if name == CROSS.name => &CROSS_LOOK,
        name if name == GOLAZO.name => &GOLAZO_LOOK,
        _ => &LOOKS[index % LOOKS.len()],
    }
}
/// Our volleyball and hero moves, made for the Quaternius skeleton (see
/// `tools/blender/volley_animations.py`), and Mixamo motion capture
/// and CMU captures retargeted onto it (see `tools/blender/retarget_mocap.py`).
const ANIMATION_LIBRARIES: [&str; 2] = ["animations/Volley.glb", "animations/Mocap.glb"];
const VOLLEY: usize = 0;
const MOCAP: usize = 1;
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
/// A stride plays at least this long (seconds) before a slower one, or the
/// other of jog and sprint, takes over: braking and speeding up in a burst
/// doesn't flick between strides, and a hit ending as the player stops doesn't
/// chain through them.
const MIN_GAIT_SECONDS: f32 = 0.25;

pub fn plugin(app: &mut App) {
    app.add_message::<SwingReleased>()
        .add_systems(Startup, (load_animation_libraries, spawn_characters))
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
        .add_systems(PostUpdate, pose_limbs.in_set(PoseLimbs).after(TransformSystems::Propagate))
        .add_systems(PostUpdate, twist_hips.after(AnimationSystems).after(crate::inertia::Inertialize).before(TransformSystems::Propagate));
}

/// Where bodies get their final pose each frame: anything measuring the
/// skeleton (like `bench`) runs after it.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PoseLimbs;

/// Which player a character model shows.
#[derive(Component)]
pub struct PlayerBody(pub usize);

/// A wound-up hit connected: where its swing was (clip seconds) against where
/// its contact frame is, whether it had been timed to the touch, and how far
/// the body was slid to meet the ball.
#[derive(Message, Clone, Copy)]
pub struct SwingReleased {
    pub clip: &'static str,
    pub at: f32,
    pub contact: f32,
    pub timed: bool,
    pub warp: f32,
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
    /// A player who only plays with their feet (Golazo) uses these instead of
    /// the bump, set, spike, serve and dive: an instep lift, a high front kick,
    /// a scissor volley, a drop-kick serve and a feet-first slide.
    KickPassLow,
    KickPassHigh,
    HighVolley,
    KickServe,
    SlideTackle,
    Celebrate,
}

/// Timing inside a hit animation, in seconds of the clip.
#[derive(Clone, Copy)]
struct Swing {
    /// The end of the wind-up. A pressed hit holds here until the ball arrives.
    wind_up: f32,
    /// Where the hand (or foot) meets the ball. The swing is timed so this
    /// lands on the moment of the touch.
    contact: f32,
    /// Where the ball is at the contact frame, in the model's space (x toward
    /// its left, y up, z forward), as the clip was authored. The body is
    /// slid so this spot meets the real ball.
    ball: Vec3,
}

/// A spot in the authoring script's terms (right, forward, up) in the model's space.
const fn spot(right: f32, forward: f32, up: f32) -> Vec3 {
    Vec3::new(-right, up, forward)
}

impl Clip {
    const ALL: [Clip; 27] = [
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
        Clip::KickPassLow,
        Clip::KickPassHigh,
        Clip::HighVolley,
        Clip::KickServe,
        Clip::SlideTackle,
        Clip::Celebrate,
    ];

    /// The library the animation is in, and its name there.
    fn source(self) -> (usize, &'static str) {
        match self {
            // Between points too: motion capture only moves the hips, so it fits
            // any body built on this skeleton, whatever its proportions.
            Clip::Idle => (MOCAP, "Ready"),
            Clip::Jog => (MOCAP, "Jog"),
            Clip::Sprint => (MOCAP, "Sprint"),
            Clip::Airborne => (MOCAP, "Airborne"),
            Clip::Land => (MOCAP, "Land"),
            Clip::Takeoff => (VOLLEY, "Takeoff"),
            Clip::Dash => (MOCAP, "Dash"),
            Clip::Ready => (MOCAP, "Ready"),
            Clip::Bump => (VOLLEY, "Bump"),
            Clip::Set => (MOCAP, "Set"),
            Clip::Spike => (MOCAP, "Spike"),
            Clip::VolleyKick => (VOLLEY, "Volley_Kick"),
            Clip::BicycleKick => (VOLLEY, "Bicycle_Kick"),
            Clip::Serve => (MOCAP, "Serve"),
            Clip::Block => (VOLLEY, "Block"),
            Clip::Dive => (VOLLEY, "Dive"),
            Clip::FootSave => (VOLLEY, "Foot_Save"),
            Clip::Crossover => (VOLLEY, "Crossover"),
            Clip::CrossoverRight => (VOLLEY, "Crossover_Right"),
            Clip::Dunk => (VOLLEY, "Dunk"),
            Clip::KnockedDown => (MOCAP, "Knocked_Down"),
            Clip::KickPassLow => (VOLLEY, "Kick_Pass_Low"),
            Clip::KickPassHigh => (VOLLEY, "Kick_Pass_High"),
            Clip::HighVolley => (VOLLEY, "High_Volley"),
            Clip::KickServe => (VOLLEY, "Kick_Serve"),
            Clip::SlideTackle => (VOLLEY, "Slide_Tackle"),
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

    /// Timings of the hits, as authored in `tools/blender/volley_animations.py`,
    /// or measured in motion capture (`retarget_mocap.py --measure`): the set,
    /// spike and serve.
    fn swing(self) -> Option<Swing> {
        match self {
            Clip::Bump => Some(Swing { wind_up: 0.15, contact: 0.25, ball: spot(0.0, 0.45, 0.8) }),
            Clip::Set => Some(Swing { wind_up: 0.9, contact: 1.05, ball: spot(-0.09, 0.1, 1.92) }),
            Clip::Spike => Some(Swing { wind_up: 0.62, contact: 0.82, ball: spot(0.38, 0.38, 1.96) }),
            Clip::VolleyKick => Some(Swing { wind_up: 0.1, contact: 0.18, ball: spot(0.12, 0.62, 0.95) }),
            Clip::BicycleKick => Some(Swing { wind_up: 0.12, contact: 0.24, ball: spot(0.08, -0.4, 1.75) }),
            Clip::Serve => Some(Swing { wind_up: 0.0, contact: 0.53, ball: spot(0.34, -0.03, 1.9) }),
            Clip::Dunk => Some(Swing { wind_up: 0.3, contact: 0.4, ball: spot(0.0, 0.5, 1.75) }),
            Clip::KickPassLow => Some(Swing { wind_up: 0.12, contact: 0.2, ball: spot(0.05, 0.55, 0.55) }),
            Clip::KickPassHigh => Some(Swing { wind_up: 0.12, contact: 0.22, ball: spot(0.05, 0.55, 1.45) }),
            Clip::HighVolley => Some(Swing { wind_up: 0.1, contact: 0.18, ball: spot(0.1, 0.5, 1.45) }),
            Clip::KickServe => Some(Swing { wind_up: 0.0, contact: 0.3, ball: spot(0.05, 0.35, 1.1) }),
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
        matches!(self, Clip::Spike | Clip::VolleyKick | Clip::BicycleKick | Clip::HighVolley)
    }

    /// The clip a player who only plays with their feet uses instead.
    fn with_feet(self) -> Clip {
        match self {
            Clip::Bump => Clip::KickPassLow,
            Clip::Set => Clip::KickPassHigh,
            Clip::Spike | Clip::VolleyKick => Clip::HighVolley,
            Clip::Serve => Clip::KickServe,
            Clip::Dive => Clip::SlideTackle,
            clip => clip,
        }
    }

    /// Kicks whose leg reaches for the real ball.
    fn kicks(self) -> bool {
        matches!(self, Clip::KickPassLow | Clip::KickPassHigh | Clip::HighVolley | Clip::VolleyKick | Clip::KickServe)
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
    // Where the ball will be met, if the move is armed and on course; else
    // where it will be in a moment.
    let ball = match sim.ball {
        _ if let Some((_, at)) = sim.predicted_contact(player, CONTACT_HORIZON) => at,
        Ball::InFlight(flight) => flight.position_at(sim.tick + LOOKAHEAD_TICKS),
        _ => sim.ball_position(),
    };
    match id {
        MoveId::Spike => Clip::attack(attack::best_technique(me, ball).0),
        // Cocked to spike, until the ball is caught.
        MoveId::Crossover => Clip::Spike,
        MoveId::Posterizer => Clip::Dunk,
        MoveId::BananaKick if me.grounded() => Clip::KickPassLow,
        MoveId::BananaKick => Clip::VolleyKick,
        MoveId::Chilena => Clip::BicycleKick,
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
    /// A hero's own version of a clip, for their style of moving: motion
    /// capture named `<Hero>_<Clip>` in the library.
    hero_nodes: HashMap<(&'static str, Clip), AnimationNodeIndex>,
    /// Hits played by the upper body alone, over running legs.
    upper_nodes: HashMap<Clip, AnimationNodeIndex>,
    /// Standing and running played by the legs alone, under an upper-body hit;
    /// keyed by hero too, for heroes with their own.
    lower_nodes: HashMap<(Option<&'static str>, Clip), AnimationNodeIndex>,
    /// Every hit and move has a twin node playing the same clip: starting it
    /// again while it's still fading out plays the twin, instead of restarting
    /// the fading copy (which would jump the limbs back to its first frame).
    twins: HashMap<AnimationNodeIndex, AnimationNodeIndex>,
}

impl Animations {
    /// The node that plays `clip` for `hero`: their own version, if they have one.
    fn node(&self, clip: Clip, hero: &str) -> AnimationNodeIndex {
        self.hero_nodes.get(&(hero, clip)).copied().unwrap_or(self.nodes[&clip])
    }

    /// The node that plays a character's current move: the upper-body version
    /// while it's layered over running legs.
    /// `node`, or its twin if `node` is still playing (fading out).
    fn fresh(&self, node: AnimationNodeIndex, player: &AnimationPlayer) -> AnimationNodeIndex {
        match self.twins.get(&node) {
            Some(&twin) if player.animation(node).is_some() => twin,
            _ => node,
        }
    }

    fn action_node(&self, clip: Clip, hero: &str, layered: bool) -> AnimationNodeIndex {
        match self.upper_nodes.get(&clip) {
            Some(&node) if layered => node,
            _ => self.node(clip, hero),
        }
    }

    /// The legs-only version of a gait, for `hero`.
    fn lower_node(&self, clip: Clip, hero: &'static str) -> AnimationNodeIndex {
        self.lower_nodes.get(&(Some(hero), clip)).copied().unwrap_or(self.lower_nodes[&(None, clip)])
    }
}

/// Hits (and the cheer) the upper body can play while the legs keep running.
const UPPER_BODY_HITS: [Clip; 4] = [Clip::Bump, Clip::Set, Clip::Serve, Clip::Celebrate];
/// Gaits the legs can keep playing under an upper-body hit.
const LOWER_BODY_GAITS: [Clip; 3] = [Clip::Ready, Clip::Jog, Clip::Sprint];
/// Mask groups: the hips and legs, and everything above.
const LOWER_BODY: u32 = 0;
const UPPER_BODY: u32 = 1;

/// The skeleton's bones, as paths of names from the armature down, and
/// whether each belongs to the lower body.
fn skeleton_paths() -> Vec<(Vec<String>, bool)> {
    fn add(paths: &mut Vec<(Vec<String>, bool)>, parent: &[String], name: String, lower: bool) -> Vec<String> {
        let mut path = parent.to_vec();
        path.push(name);
        paths.push((path.clone(), lower));
        path
    }
    let mut paths = Vec::new();
    let armature = vec!["Armature".to_string()];
    let root = add(&mut paths, &armature, "root".into(), true);
    let pelvis = add(&mut paths, &root, "pelvis".into(), true);
    for side in ["l", "r"] {
        let mut leg = pelvis.clone();
        for bone in ["thigh", "calf", "foot", "ball", "ball_leaf"] {
            leg = add(&mut paths, &leg, format!("{bone}_{side}"), true);
        }
    }
    let mut spine = pelvis;
    for bone in ["spine_01", "spine_02", "spine_03"] {
        spine = add(&mut paths, &spine, bone.into(), false);
    }
    let neck = add(&mut paths, &spine, "neck_01".into(), false);
    add(&mut paths, &neck, "Head".into(), false);
    for side in ["l", "r"] {
        let mut arm = spine.clone();
        for bone in ["clavicle", "upperarm", "lowerarm", "hand"] {
            arm = add(&mut paths, &arm, format!("{bone}_{side}"), false);
        }
        for finger in ["index", "middle", "ring", "pinky", "thumb"] {
            let mut joint = arm.clone();
            for part in ["01", "02", "03", "04_leaf"] {
                joint = add(&mut paths, &joint, format!("{finger}_{part}_{side}"), false);
            }
        }
    }
    paths
}

/// The model showing player `index`.
#[derive(Component)]
struct Character {
    index: usize,
    /// The hero's name, for their own versions of clips.
    hero: &'static str,
    look: &'static Look,
    /// Plays only with the feet: kicks instead of hand hits.
    feet: bool,
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
    /// When and where the armed hit will meet the ball: seconds from now, and
    /// the ball's position then. Refreshed every frame.
    contact: Option<(f32, Vec3)>,
    /// The swing is playing toward a predicted contact, not holding.
    timed: bool,
    /// How far the model is slid from the player's real position, so the
    /// contact spot of the swing meets the ball.
    warp: Vec3,
    /// Running, jogging or standing, as last chosen.
    gait: Clip,
    /// When the gait last changed (game seconds).
    gait_since: f32,
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
    /// Both legs (thigh, calf, foot), left then right, and where each foot is
    /// locked to the ground.
    legs: Vec<[Entity; 3]>,
    foot_locks: [FootLock; 2],
    /// Where the posed leg points, and how strongly (0 to 1) it's posed.
    leg_goal: Vec3,
    leg_weight: f32,
    /// Neck and head bones, for looking at the ball.
    neck: Option<[Entity; 2]>,
    /// Pelvis and lower spine, for turning the hips toward where the player
    /// runs; and how far they're turned (radians about the vertical, as yaw).
    hips: Option<[Entity; 3]>,
    hip_yaw: f32,
    /// The latest landing: how far the hips dip for it (m), and how long ago
    /// it was (seconds).
    dip: f32,
    dip_age: f32,
    /// How strongly (0 to 1) the head tracks the ball.
    look_weight: f32,
    /// Body lean from acceleration: forward and to the right, in radians.
    lean: Vec2,
    /// The move underway plays on the upper body only, over the legs' own gait.
    layered: bool,
    /// The legs-only gait playing under a layered move.
    lower_playing: Option<AnimationNodeIndex>,
    /// The node playing the main clip (a clip's own, or its twin).
    node: Option<AnimationNodeIndex>,
    /// Legs-only gaits fading out, after the legs changed stride or the hit
    /// left the upper body.
    lower_fading: Vec<AnimationNodeIndex>,
}

impl Character {
    fn new(index: usize, hero: &'static str, yaw: f32, look: &'static Look, feet: bool) -> Self {
        Self {
            index,
            hero,
            look,
            feet,
            armature: None,
            playing: None,
            action: None,
            restart: false,
            seek: None,
            winding_up: false,
            release: false,
            catch_up_to: None,
            contact: None,
            timed: false,
            warp: Vec3::ZERO,
            gait: Clip::Idle,
            gait_since: 0.0,
            airborne: false,
            yaw,
            face: None,
            arms: Vec::new(),
            arm_goal: Vec3::ZERO,
            arm_weight: 0.0,
            leg: None,
            legs: Vec::new(),
            foot_locks: [FootLock::default(); 2],
            leg_goal: Vec3::ZERO,
            leg_weight: 0.0,
            neck: None,
            hips: None,
            hip_yaw: 0.0,
            dip: 0.0,
            dip_age: f32::MAX,
            look_weight: 0.0,
            lean: Vec2::ZERO,
            layered: false,
            lower_playing: None,
            node: None,
            lower_fading: Vec::new(),
        }
    }

    /// The clip this character plays for `clip`: the feet version, for a
    /// player who only plays with their feet.
    fn style(&self, clip: Clip) -> Clip {
        if self.feet { clip.with_feet() } else { clip }
    }

    fn start(&mut self, clip: Clip, seek: Option<f32>) {
        let clip = self.style(clip);
        self.action = Some(clip);
        self.restart = true;
        self.seek = seek;
        self.winding_up = false;
        self.release = false;
        self.timed = false;
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
    for (path, lower) in skeleton_paths() {
        let target = AnimationTargetId::from_names(path.iter().map(|name| Name::new(name.clone())).collect::<Vec<_>>().iter());
        graph.add_target_to_mask_group(target, if lower { LOWER_BODY } else { UPPER_BODY });
    }
    let mut nodes = HashMap::default();
    let mut hero_nodes = HashMap::default();
    let mut upper_nodes = HashMap::default();
    let mut lower_nodes = HashMap::default();
    let mut twins = HashMap::default();
    // A clip's node, and for one-shot clips, its twin.
    let mut add = |graph: &mut AnimationGraph, clip: Clip, handle: &Handle<AnimationClip>, mask: u64| {
        let node = graph.add_clip_with_mask(handle.clone(), mask, 1.0, graph.root);
        if !clip.looping() {
            twins.insert(node, graph.add_clip_with_mask(handle.clone(), mask, 1.0, graph.root));
        }
        node
    };
    for clip in Clip::ALL {
        let (library, name) = clip.source();
        let handle = loaded[library].named_animations.get(name).unwrap_or_else(|| {
            panic!("{} has no animation named {name}", ANIMATION_LIBRARIES[library])
        });
        nodes.insert(clip, add(&mut graph, clip, handle, 0));
        if UPPER_BODY_HITS.contains(&clip) {
            upper_nodes.insert(clip, add(&mut graph, clip, handle, 1 << LOWER_BODY));
        }
        if LOWER_BODY_GAITS.contains(&clip) {
            lower_nodes.insert((None, clip), graph.add_clip_with_mask(handle.clone(), 1 << UPPER_BODY, 1.0, graph.root));
        }
        for hero in HEROES {
            if let Some(handle) = loaded[library].named_animations.get(format!("{}_{name}", hero.name).as_str()) {
                hero_nodes.insert((hero.name, clip), add(&mut graph, clip, handle, 0));
                if LOWER_BODY_GAITS.contains(&clip) {
                    lower_nodes.insert((Some(hero.name), clip), graph.add_clip_with_mask(handle.clone(), 1 << UPPER_BODY, 1.0, graph.root));
                }
            }
        }
    }
    commands.insert_resource(Animations { graph: graphs.add(graph), nodes, hero_nodes, upper_nodes, lower_nodes, twins });
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
                Character::new(index, player.kit.name, yaw, look, player.kit.has(Passive::Feet)),
                PlayerBody(index),
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
    let mut inertia = Inertia::default();
    inertia.track(armature, &children);
    commands.entity(armature).insert((AnimationPlayer::default(), AnimationTransitions::new(), inertia));
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
    character.legs = [["thigh_l", "calf_l", "foot_l"], ["thigh_r", "calf_r", "foot_r"]]
        .into_iter()
        .filter_map(|[thigh, calf, foot]| Some([bone(thigh)?, bone(calf)?, bone(foot)?]))
        .collect();
    character.neck = (|| Some([bone("neck_01")?, bone("Head")?]))();
    character.hips = (|| Some([bone("pelvis")?, bone("spine_01")?, bone("spine_02")?]))();

    let Some((hair, hair_color)) = character.look.hair else {
        return;
    };
    let hair = assets.load(GltfAssetLabel::Scene(0).from_asset(hair));
    commands.spawn((WorldAssetRoot(hair), Transform::default(), ChildOf(ready.entity))).observe(
        move |ready: On<WorldInstanceReady>,
              mut commands: Commands,
              children: Query<&Children>,
              names: Query<&Name>,
              mut inertias: Query<&mut Inertia>,
              meshes: Query<&MeshMaterial3d<StandardMaterial>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            if let Some(hair_armature) = find_armature(ready.entity, &children, &names) {
                add_animation_targets(&mut commands, hair_armature, armature, &children, &names);
                // The hair follows the head through cuts, too.
                if let Ok(mut inertia) = inertias.get_mut(armature) {
                    inertia.track(hair_armature, &children);
                }
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
                        // A banana kick from the ground is a pass-style strike; in the air, a volley.
                        HitKind::Curve if game.current.players[me].grounded() => Clip::KickPassLow,
                        HitKind::Curve => Clip::VolleyKick,
                        // Digs and kicks happen mid-move, which keeps playing.
                        HitKind::Dig | HitKind::Kick => continue,
                    };
                    let clip = character.style(clip);
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
                        // are already wound up, and swing to meet the ball as
                        // it leaves the hand (`predicted_contact`).
                        MoveId::Pass | MoveId::Spike | MoveId::Crossover | MoveId::Posterizer | MoveId::BananaKick | MoveId::Chilena => {
                            let serving = game.current.ball == Ball::Held { by: me };
                            if !serving && !character.action.is_some_and(Clip::is_game_action) {
                                // If the ball is nearly there, start partway in, so
                                // the swing reaches contact just as it touches.
                                let clip = character.style(hit_clip(&game.current, me, id));
                                let seek = clip.swing().zip(game.current.predicted_contact(me, CONTACT_HORIZON)).and_then(
                                    |(swing, (ahead, _))| {
                                        let seconds = ahead as f32 * DT;
                                        let lead = swing.contact - seconds * clip.speed();
                                        (lead > 0.0).then_some(lead.min(swing.contact))
                                    },
                                );
                                character.start(clip, seek);
                                character.winding_up = true;
                                character.timed = seek.is_some();
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
                // Kicking off a wall: spring up again, turned away from it.
                Event::WallJumped { player, away } if player == me => {
                    character.face = Some((away, face_until));
                    if !character.action.is_some_and(Clip::is_game_action) {
                        character.start(Clip::Takeoff, None);
                    }
                }
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
    real: Res<Time<Real>>,
    hit_stop: Res<HitStop>,
    mut characters: Query<(&mut Character, &mut Transform)>,
) {
    let ball = game.current.ball_position();
    for (mut character, mut transform) in &mut characters {
        let feet = player_feet(&game, &fixed, character.index);
        // Where and when an armed hit will meet the ball, from the frame's
        // point between ticks.
        character.contact = game
            .current
            .predicted_contact(character.index, CONTACT_HORIZON)
            .map(|(ahead, at)| ((ahead as f32 - fixed.overstep_fraction()) * DT, at));
        let velocity = ground_velocity(&game, character.index);
        let to_ball = Vec2::new(ball.x - feet.x, ball.z - feet.z);
        let facing = match character.face {
            Some((direction, until)) if time.elapsed_secs() < until => direction,
            // Serving: square to the net, the ball up over the shoulder.
            _ if game.current.ball == (Ball::Held { by: character.index }) => {
                Vec2::new(-game.current.players[character.index].side, 0.0)
            }
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
        // Lean into acceleration and turns, from the feet: forward when
        // speeding up, back when braking, sideways into a turn.
        let player = &game.current.players[character.index];
        let before = game.previous.players.get(character.index).map_or(player.velocity, |p| p.velocity);
        let acceleration = if game.previous.rally == game.current.rally { (player.velocity - before) / DT } else { Vec2::ZERO };
        let forward = Vec2::new(character.yaw.sin(), character.yaw.cos());
        let right = Vec2::new(-forward.y, forward.x);
        let grounded = player.grounded() && !player.stunned(game.current.tick);
        let wanted = if grounded {
            Vec2::new(acceleration.dot(forward), acceleration.dot(right)) * LEAN_PER_ACCELERATION
        } else {
            Vec2::ZERO
        }
        .clamp_length_max(MAX_LEAN);
        let blend = 1.0 - (-LEAN_SMOOTHING * time.delta_secs()).exp();
        let lean = character.lean + (wanted - character.lean) * blend;
        character.lean = lean;
        // The model faces +z with its right toward -x: a turn about x tips it
        // forward, and about z (positive) tips it to its right.
        let lean = Quat::from_rotation_x(lean.x) * Quat::from_rotation_z(lean.y);
        let facing = Quat::from_rotation_y(character.yaw);

        // Running one way while facing another (turning around, or toward the
        // ball winding up a hit): the hips turn toward the run so the legs
        // stride where they go, and the spine turns back so the chest stays.
        let striding = character.action.is_none() || character.layered;
        let wanted = if grounded && striding && velocity.length() > JOG_SPEED {
            // The turn about the vertical from facing to running, as yaw is measured.
            let off = (velocity.x.atan2(velocity.y) - character.yaw + PI).rem_euclid(TAU) - PI;
            // Turning around, the hips lead the turn; right behind, which
            // way to turn is a coin toss, so they wait.
            if off.abs() < 2.8 { off.clamp(-MAX_HIP_TURN, MAX_HIP_TURN) } else { 0.0 }
        } else {
            0.0
        };
        character.hip_yaw += (wanted - character.hip_yaw) * (1.0 - (-HIP_TURN_RATE * time.delta_secs()).exp());

        // Slide the body so the swing's contact spot meets the real ball:
        // eased in over the last moments before contact, and back out after.
        let swing = character.action.filter(|_| character.winding_up).and_then(Clip::swing);
        let wanted = match (swing, character.contact) {
            (Some(swing), Some((seconds, ball))) if seconds < WARP_WINDOW => {
                let offset = ball - (feet + facing * swing.ball);
                let closeness = 1.0 - seconds / WARP_WINDOW;
                Vec3::new(offset.x, 0.0, offset.z).clamp_length_max(MAX_WARP) * closeness
            }
            // Hold the slide through contact; let it go once the swing is done.
            _ if character.action.is_some_and(|clip| clip.swing().is_some()) && character.timed => character.warp,
            _ => Vec3::ZERO,
        };
        let rate = if wanted == Vec3::ZERO { WARP_RELEASE } else { WARP_FOLLOW };
        let warp = character.warp + (wanted - character.warp) * (1.0 - (-rate * time.delta_secs()).exp());
        character.warp = warp;
        // The hitter shakes through a hit-stop: across the body on the ground,
        // up and down in the air.
        let shake = hit_stop.shake(character.index, real.elapsed_secs()).map_or(Vec3::ZERO, |amount| {
            if player.grounded() { facing * Vec3::X * amount } else { Vec3::Y * amount }
        });
        *transform = Transform::from_translation(feet + warp + shake).with_rotation(facing * lean);
    }
}

/// How far ahead (ticks) to look for a hit's contact with the ball.
const CONTACT_HORIZON: u32 = 60;
/// The body starts sliding toward the contact this long before it, by at
/// most this far, following at `WARP_FOLLOW` and letting go at `WARP_RELEASE`
/// per second.
const WARP_WINDOW: f32 = 0.45;
const MAX_WARP: f32 = 1.1;
const WARP_FOLLOW: f32 = 25.0;
const WARP_RELEASE: f32 = 8.0;

/// Radians of body lean per m/s² of acceleration, at most `MAX_LEAN`, eased in
/// at `LEAN_SMOOTHING` per second.
const LEAN_PER_ACCELERATION: f32 = 0.004;
const MAX_LEAN: f32 = 0.2;
const LEAN_SMOOTHING: f32 = 10.0;

fn animate_characters(
    game: Res<Match>,
    animations: Res<Animations>,
    mut commands: Commands,
    mut characters: Query<&mut Character>,
    blending: Res<Blending>,
    time: Res<Time>,
    mut rigs: Query<(&mut AnimationPlayer, &mut AnimationTransitions, Option<&mut Inertia>, Has<AnimationGraphHandle>)>,
    mut released: MessageWriter<SwingReleased>,
) {
    for mut character in &mut characters {
        let Some(armature) = character.armature else {
            continue;
        };
        let Ok((mut player, mut transitions, mut inertia, has_graph)) = rigs.get_mut(armature) else {
            continue;
        };
        if !has_graph {
            commands.entity(armature).insert(AnimationGraphHandle(animations.graph.clone()));
        }

        let hero = character.hero;
        let sim = &game.current;
        let me = &sim.players[character.index];
        let airborne = !me.grounded();
        if airborne != character.airborne {
            character.airborne = airborne;
            // Landing: the hips give under the body's weight, the more the
            // faster it came down.
            if !airborne {
                let falling = game.previous.players.get(character.index).map_or(0.0, |p| -p.vertical_velocity);
                character.dip = (falling * DIP_PER_SPEED).clamp(0.0, MAX_DIP);
                character.dip_age = 0.0;
            }
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
        // A hit winding up switches technique as the ball comes in: an attack
        // between spike and kicks, a pass between bump and set. Not once the
        // swing has started toward contact.
        if character.winding_up
            && !character.timed
            && let Some(clip) = character.action
            && let Some(id) = me.active_move(sim.tick).filter(|&id| match id {
                MoveId::Spike => clip.is_attack(),
                MoveId::Pass => matches!(clip, Clip::Bump | Clip::Set | Clip::KickPassLow | Clip::KickPassHigh),
                _ => false,
            })
        {
            let wanted = character.style(hit_clip(sim, character.index, id));
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
                _ => character.node.and_then(|node| player.animation(node)).is_none_or(|active| active.is_finished()),
            };
            if finished {
                character.action = None;
            }
        }

        if character.winding_up
            && !character.restart
            && let Some(action) = character.action
            && let Some(swing) = action.swing()
            && let Some(active) = character.node.and_then(|node| player.animation_mut(node))
        {
            let at = active.seek_time();
            if character.release {
                released.write(SwingReleased {
                    clip: action.source().1,
                    at,
                    contact: swing.contact,
                    timed: character.timed,
                    warp: character.warp.length(),
                });
                // Connected. A timed swing is already at contact; otherwise
                // hurry through to it. Then follow through.
                if at >= swing.contact - 0.02 {
                    active.set_speed(action.speed());
                } else {
                    active.set_speed(CATCH_UP_SPEED);
                    character.catch_up_to = Some(swing.contact);
                }
                character.winding_up = false;
            } else if let Some((seconds, _)) = character.contact
                && seconds <= (swing.contact - at).max(0.0) / action.speed() + DT
            {
                // The ball is coming: play on so the contact frame lands on
                // the touch, and wait there if it's a moment late.
                character.timed = true;
                let remaining = swing.contact - at;
                // Clip seconds to cover per real second until contact.
                active.set_speed(if remaining <= 0.0 { 0.0 } else { (remaining / seconds.max(DT)).clamp(0.3, 4.0) });
            } else if character.timed && at >= swing.contact {
                active.set_speed(0.0);
            } else if if matches!(action, Clip::Serve | Clip::KickServe) { !serving } else { me.active_move(sim.tick).is_none() } {
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
            && let Some(active) = character.node.and_then(|node| player.animation_mut(node))
            && active.seek_time() >= contact
        {
            active.set_speed(action.speed());
            character.catch_up_to = None;
        }

        let gait = match character.gait {
            Clip::Sprint if speed > STOP_SPRINT_SPEED => Clip::Sprint,
            _ if speed > SPRINT_SPEED => Clip::Sprint,
            Clip::Jog | Clip::Sprint if speed > STOP_JOG_SPEED => Clip::Jog,
            _ if speed > JOG_SPEED => Clip::Jog,
            _ if sim.phase == Phase::Rally => Clip::Ready,
            _ => Clip::Idle,
        };
        // Speeding up changes stride at once; slowing down, or flicking between
        // jog and sprint, waits until the current stride has played a moment.
        let rank = |clip: Clip| match clip {
            Clip::Sprint => 3,
            Clip::Jog => 2,
            Clip::Ready => 1,
            _ => 0,
        };
        let settled = time.elapsed_secs() - character.gait_since >= MIN_GAIT_SECONDS;
        let flicker = matches!((character.gait, gait), (Clip::Jog, Clip::Sprint) | (Clip::Sprint, Clip::Jog));
        let slowing = rank(gait) < rank(character.gait);
        if gait != character.gait && (settled || !(flicker || slowing)) {
            character.gait = gait;
            character.gait_since = time.elapsed_secs();
        }
        // A hit or cheer that can play on the upper body moves onto it as soon
        // as the player runs, keeping its timing, and the legs take up the
        // stride instead of gliding along under it.
        if let Some(action) = character.action
            && character.playing == Some(action)
            && UPPER_BODY_HITS.contains(&action)
            && !character.layered
            && !character.restart
            && !airborne
            && speed > JOG_SPEED
            && let Some((at, pace)) = character.node.and_then(|node| player.animation(node)).map(|a| (a.seek_time(), a.speed()))
            // A serve keeps its whole body through the hit: its hips are part
            // of where the hand meets the ball. Running in after it is fine.
            && (action != Clip::Serve || action.swing().is_some_and(|swing| at > swing.contact + 0.05))
        {
            character.layered = true;
            let node = animations.fresh(animations.action_node(action, hero, true), &player);
            character.node = Some(node);
            let active = transitions.play(&mut player, node, BLEND);
            active.seek_to(at);
            active.set_speed(pace);
            if action.looping() {
                active.repeat();
            }
        }
        let movement = if airborne { Clip::Airborne } else { character.gait };
        let clip = character.action.unwrap_or(movement);
        if character.playing != Some(clip) || character.restart {
            let changing_gait = !clip.is_game_action() && !character.playing.is_some_and(Clip::is_game_action);
            let blend = if changing_gait { GAIT_BLEND } else { BLEND };
            character.catch_up_to = None;
            // Hits made on the run play on the upper body; the legs keep running.
            character.layered = UPPER_BODY_HITS.contains(&clip) && !airborne && speed > JOG_SPEED && !serving;
            // With inertialization on, cut into and out of hits and let the old
            // pose fade out; between gaits, always crossfade so strides line up.
            let blend = match (inertia.as_deref_mut(), *blending) {
                (Some(inertia), Blending::Inertia) if !changing_gait => {
                    inertia.cut(blend.as_secs_f32());
                    Duration::ZERO
                }
                _ => blend,
            };
            // Back into a stride that's still playing (fading out, or on the
            // legs alone under a hit that just ended): carry on from where it
            // is rather than restarting it, which would jump the limbs.
            let node = animations.fresh(animations.action_node(clip, hero, character.layered), &player);
            let legs_stride = clip.looping().then(|| animations.lower_nodes.get(&(Some(hero), clip)).or(animations.lower_nodes.get(&(None, clip))).and_then(|&n| player.animation(n)).map(|a| a.seek_time())).flatten();
            character.node = Some(node);
            // A stride starts its minimum time whenever it starts playing.
            if !clip.is_game_action() {
                character.gait_since = time.elapsed_secs();
            }
            let resume = clip.looping().then(|| player.animation(node).map(|a| a.seek_time())).flatten().or(legs_stride);
            let active = transitions.play(&mut player, node, blend);
            if let Some(at) = resume
                && character.seek.is_none()
            {
                active.seek_to(at);
            }
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
        // Under a layered hit, the legs keep standing or running at their own pace.
        let layered = character.layered && character.action.is_some_and(|clip| UPPER_BODY_HITS.contains(&clip)) && !airborne;
        let legs = layered.then(|| {
            let gait = if speed > SPRINT_SPEED { Clip::Sprint } else if speed > STOP_JOG_SPEED { Clip::Jog } else { Clip::Ready };
            (gait, animations.lower_node(gait, hero))
        });
        // The legs blend from one stride to the next, and in and out: the hips
        // carry the whole upper body, so a sudden change would jump the hands.
        if character.lower_playing != legs.map(|(_, node)| node) {
            if let Some(old) = character.lower_playing.take() {
                character.lower_fading.push(old);
            }
            if let Some((gait, node)) = legs {
                if let Some(i) = character.lower_fading.iter().position(|&n| n == node) {
                    character.lower_fading.remove(i);
                } else {
                    // Pick up the stride where the whole body had it.
                    let at = player.animation(animations.node(gait, hero)).map(|a| a.seek_time());
                    let active = player.play(node).repeat().set_weight(0.0);
                    if let Some(at) = at {
                        active.seek_to(at);
                    }
                }
                character.lower_playing = Some(node);
            }
        }
        let step = time.delta_secs() / BLEND.as_secs_f32();
        if let Some(node) = character.lower_playing
            && let Some(active) = player.animation_mut(node)
        {
            active.set_weight((active.weight() + step).min(1.0));
        }
        character.lower_fading.retain(|&node| {
            let Some(active) = player.animation_mut(node) else { return false };
            let weight = active.weight() - step;
            if weight <= 0.0 {
                player.stop(node);
                return false;
            }
            active.set_weight(weight);
            true
        });
        if let Some((gait, node)) = legs
            && let Some(active) = player.animation_mut(node)
        {
            let pace = match gait {
                Clip::Jog => Some(JOG_PACE),
                Clip::Sprint => Some(SPRINT_PACE),
                _ => None,
            };
            active.set_speed(pace.map_or(1.0, |pace| (speed / pace).clamp(0.7, 1.8)));
        }
        if !layered {
            character.layered = false;
        }

        // Running clips keep pace with the feet.
        if let Some(pace) = match clip {
            Clip::Jog => Some(JOG_PACE),
            Clip::Sprint => Some(SPRINT_PACE),
            _ => None,
        } && let Some(active) = player.animation_mut(animations.node(clip, hero))
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
/// Arms reach fully for the ball over this long before contact.
const FULL_REACH_SECONDS: f32 = 0.15;
/// How far a reach bends the animated arms toward the ball: enough to meet it,
/// not so much the bump or set loses its shape.
const REACH_WEIGHT: f32 = 0.6;
/// A foot save's leg reaches for a ball this close; otherwise it kicks straight out.
const KICK_REACH_START: f32 = 2.5;
/// How far a kick's animated leg bends toward the real ball.
const KICK_REACH_WEIGHT: f32 = 0.6;

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
    mut characters: Query<(&mut Character, &Transform)>,
    locals: Query<&Transform, Without<Character>>,
    children: Query<&Children>,
    mut globals: Query<&mut GlobalTransform>,
) {
    let sim = &game.current;
    for (mut character, body) in &mut characters {
        let step = ARM_BLEND_SPEED * time.delta_secs();
        lock_feet(&mut character, body, sim, time.delta_secs(), &locals, &children, &mut globals);
        look_at_ball(&mut character, body, sim, time.delta_secs(), &locals, &children, &mut globals);

        // Keep the last goal while blending out, so arms ease back from where they were.
        // In the last moments before contact the arms reach all the way to
        // the ball, so they meet it whatever height it comes in at.
        let at_contact = character.contact.map_or(0.0, |(seconds, _)| (1.0 - seconds / FULL_REACH_SECONDS).clamp(0.0, 1.0));
        let target = match arm_goal(sim, character.index, character.action) {
            Some((ball, strength)) => {
                character.arm_goal = ball;
                strength + (1.0 - strength) * at_contact
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
        // A kick's leg reaches for the real ball as it comes close.
        let ball = sim.ball_position();
        let kick_reach = character.action.is_some_and(Clip::kicks)
            && matches!(sim.ball, Ball::InFlight(_))
            && ball.distance(me.position) < KICK_REACH_START;
        if kick_reach && kicking.is_none() {
            character.leg_goal = ball;
        }
        let target = if kicking.is_some() {
            1.0
        } else if kick_reach {
            KICK_REACH_WEIGHT
        } else {
            0.0
        };
        character.leg_weight += (target - character.leg_weight).clamp(-step, step);
        if character.leg_weight > 0.0
            && let Some(leg) = character.leg
        {
            let goal = character.leg_goal;
            point_limb(leg, |joint| (goal - joint).normalize_or_zero(), character.leg_weight, &locals, &children, &mut globals);
        }
    }
}

/// How far (radians) the hips turn toward where the player runs, at most, and
/// how quickly they follow.
const MAX_HIP_TURN: f32 = 1.3;
const HIP_TURN_RATE: f32 = 12.0;

/// Turns each character's hips by `hip_yaw` about the vertical and the lower
/// spine back by as much, so the legs face one way and the chest another; and
/// drops the hips for a landing.
/// Works on the local transforms after the animation, before they're
/// propagated.
fn twist_hips(
    time: Res<Time>,
    mut characters: Query<&mut Character>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    for mut character in &mut characters {
        let Some([pelvis, spine_1, spine_2]) = character.hips else { continue };
        // The pelvis's parent (the skeleton's root bone) as last drawn: it
        // doesn't animate, and the body turns slowly.
        let Some(parent) = parents.get(pelvis).ok().and_then(|p| globals.get(p.parent()).ok()) else { continue };
        // A landing's dip: the hips drop quickly and come back up; planted
        // feet stay put, so the knees take it.
        character.dip_age += time.delta_secs();
        let drop = landing_dip(character.dip, character.dip_age);
        if drop > 0.0
            && let Ok(mut transform) = transforms.get_mut(pelvis)
        {
            transform.translation += parent.affine().inverse().transform_vector3(Vec3::NEG_Y * drop);
        }
        let angle = character.hip_yaw;
        if angle.abs() < 1e-3 {
            continue;
        }
        // Turning a bone about the world's vertical: the vertical in its
        // parent's frame, from the parent's world rotation.
        let mut parent = parent.rotation();
        for (bone, turn) in [(pelvis, angle), (spine_1, -angle / 2.0), (spine_2, -angle / 2.0)] {
            let Ok(mut transform) = transforms.get_mut(bone) else { break };
            let up = parent.inverse() * Vec3::Y;
            transform.rotation = Quat::from_axis_angle(up, turn) * transform.rotation;
            parent *= transform.rotation;
        }
    }
}

/// Landing: the hips dip this much (m) for each m/s the body came down at, up
/// to `MAX_DIP`, reaching it in `DIP_IN` seconds and coming back over `DIP_OUT`.
const DIP_PER_SPEED: f32 = 0.014;
const MAX_DIP: f32 = 0.1;
const DIP_IN: f32 = 0.06;
const DIP_OUT: f32 = 0.3;

/// How far (m) the hips are down `age` seconds into a landing of `depth`.
fn landing_dip(depth: f32, age: f32) -> f32 {
    if age < DIP_IN {
        depth * (age / DIP_IN * std::f32::consts::FRAC_PI_2).sin()
    } else if age < DIP_IN + DIP_OUT {
        let x = (age - DIP_IN) / DIP_OUT;
        depth * (1.0 - x * x * (3.0 - 2.0 * x))
    } else {
        0.0
    }
}

/// Foot locking: a foot whose ankle is this low (m above the feet) is on the
/// ground, and stays where it landed while it is, unless the animation pulls
/// it this far away (m); once lifted, it eases back to the animation over
/// this long (seconds).
const PLANT_HEIGHT: f32 = 0.14;
const MAX_STRETCH: f32 = 0.35;
const RELEASE_SECONDS: f32 = 0.1;

/// Where a foot is held on the ground, and where it eases back from once
/// lifted (with how far through that it is, 0 to 1).
#[derive(Clone, Copy, Default)]
struct FootLock {
    at: Option<Vec3>,
    release_from: Option<Vec3>,
    release: f32,
}

/// Feet stick where they land: while a foot is on the ground, the leg is
/// solved so the ankle stays put on the floor (its height still the
/// animation's, so heels roll as they should), however the body moves over
/// it. Strides that don't quite match the ground speed, the body sliding into
/// a hit and turning on the spot no longer drag planted feet along.
fn lock_feet(
    character: &mut Character,
    body: &Transform,
    sim: &Sim,
    dt: f32,
    locals: &Query<&Transform, Without<Character>>,
    children: &Query<&Children>,
    globals: &mut Query<&mut GlobalTransform>,
) {
    let me = &sim.players[character.index];
    // Legs posed on purpose (dives, slides, kicks, getting knocked down) and
    // legs in the air are left alone.
    let posed = character.action.is_some_and(|clip| {
        clip.kicks() || matches!(clip, Clip::Dive | Clip::SlideTackle | Clip::KnockedDown | Clip::FootSave)
    });
    let locking = me.grounded() && !me.stunned(sim.tick) && !posed;
    // A landing's dip lowers the hips, and the legs with them: planted feet
    // stay on the floor, so the knees bend instead.
    let drop = landing_dip(character.dip, character.dip_age);
    let legs = character.legs.clone();
    for (side, [thigh, calf, foot]) in legs.into_iter().enumerate() {
        let Ok(ankle) = globals.get(foot).map(|g| g.translation()) else { continue };
        let lock = &mut character.foot_locks[side];
        let planted = locking && ankle.y - body.translation.y < PLANT_HEIGHT;
        let mut target = None;
        match lock.at {
            Some(at) if planted && Vec2::new(at.x - ankle.x, at.z - ankle.z).length() <= MAX_STRETCH => {
                target = Some(Vec3::new(at.x, ankle.y + drop, at.z));
            }
            Some(at) => {
                // Lifted (or stretched too far): ease back to the animation.
                lock.release_from = Some(at);
                lock.release = 0.0;
                lock.at = planted.then_some(ankle);
            }
            None if planted => {
                lock.at = Some(ankle);
                if drop > 0.0 {
                    target = Some(ankle + Vec3::Y * drop);
                }
            }
            None => {}
        }
        if let Some(from) = lock.release_from {
            lock.release += dt / RELEASE_SECONDS;
            if lock.release >= 1.0 {
                lock.release_from = None;
            } else if target.is_none() {
                let x = lock.release * lock.release * (3.0 - 2.0 * lock.release);
                let from = Vec3::new(from.x, ankle.y, from.z);
                target = Some(from.lerp(ankle, x));
            }
        }
        if let Some(target) = target {
            solve_leg([thigh, calf, foot], target, locals, children, globals);
        }
    }
}

/// Bends a leg (thigh, calf, foot) so the ankle reaches `target`, keeping the
/// knee bending the way the animation bends it and the foot turned as the
/// animation has it.
fn solve_leg(
    [thigh, calf, foot]: [Entity; 3],
    target: Vec3,
    locals: &Query<&Transform, Without<Character>>,
    children: &Query<&Children>,
    globals: &mut Query<&mut GlobalTransform>,
) {
    let (Ok(hip), Ok(knee), Ok(ankle)) = (globals.get(thigh).copied(), globals.get(calf).copied(), globals.get(foot).copied()) else {
        return;
    };
    let (hip_at, knee_at, ankle_at) = (hip.translation(), knee.translation(), ankle.translation());
    let (upper, lower) = (hip_at.distance(knee_at), knee_at.distance(ankle_at));
    if upper < 1e-4 || lower < 1e-4 {
        return;
    }
    let reach = target - hip_at;
    let distance = reach.length().clamp(0.01, (upper + lower) * 0.999);
    let along = reach.normalize_or_zero();
    // The knee stays in the plane it bends in now.
    let bend = ((knee_at - hip_at) - along * (knee_at - hip_at).dot(along)).normalize_or_zero();
    let cos = ((upper * upper + distance * distance - lower * lower) / (2.0 * upper * distance)).clamp(-1.0, 1.0);
    let new_knee = hip_at + (along * cos + bend * (1.0 - cos * cos).sqrt()) * upper;
    let new_ankle = hip_at + along * distance;

    let turn = |bone: Entity, from: Vec3, to: Vec3, globals: &mut Query<&mut GlobalTransform>| {
        let Ok(global) = globals.get(bone).copied() else { return };
        let (scale, rotation, translation) = global.to_scale_rotation_translation();
        let arc = Quat::from_rotation_arc(from.normalize_or_zero(), to.normalize_or_zero());
        follow_parent(bone, GlobalTransform::from(Transform { translation, rotation: arc * rotation, scale }), locals, children, globals);
    };
    turn(thigh, knee_at - hip_at, new_knee - hip_at, globals);
    let (Ok(knee_now), Ok(ankle_now)) = (globals.get(calf).map(|g| g.translation()), globals.get(foot).map(|g| g.translation())) else {
        return;
    };
    turn(calf, ankle_now - knee_now, new_ankle - knee_now, globals);
    // The foot keeps the turn the animation gave it.
    if let Ok(placed) = globals.get(foot).copied() {
        let (scale, _, translation) = placed.to_scale_rotation_translation();
        follow_parent(foot, GlobalTransform::from(Transform { translation, rotation: ankle.rotation(), scale }), locals, children, globals);
    }
}

/// How far (radians) the head turns toward the ball, at most, left or right
/// and up or down; how strongly; and how quickly it eases in and out.
const LOOK_YAW: f32 = 1.1;
const LOOK_PITCH: f32 = 0.6;
const LOOK_WEIGHT: f32 = 0.8;
const LOOK_BLEND_SPEED: f32 = 4.0;
/// The neck takes this share of the turn, the head the rest.
const NECK_SHARE: f32 = 0.4;

/// Turns the neck and head toward the ball, on top of the animation: players
/// keep their eyes on it. Not while lying down, flipping or celebrating.
fn look_at_ball(
    character: &mut Character,
    body: &Transform,
    sim: &Sim,
    dt: f32,
    locals: &Query<&Transform, Without<Character>>,
    children: &Query<&Children>,
    globals: &mut Query<&mut GlobalTransform>,
) {
    let Some([neck, head]) = character.neck else { return };
    let me = &sim.players[character.index];
    let busy = character.action.is_some_and(|clip| {
        matches!(
            clip,
            Clip::Dive | Clip::SlideTackle | Clip::KnockedDown | Clip::BicycleKick | Clip::Dunk | Clip::Celebrate
        )
    });
    let watching = matches!(sim.ball, Ball::InFlight(_) | Ball::Carried { .. } | Ball::Held { .. }) && !busy && !me.stunned(sim.tick);
    let target = if watching { LOOK_WEIGHT } else { 0.0 };
    character.look_weight += (target - character.look_weight).clamp(-LOOK_BLEND_SPEED * dt, LOOK_BLEND_SPEED * dt);
    if character.look_weight <= 0.0 {
        return;
    }
    let Ok(head_at) = globals.get(head).map(|g| g.translation()) else { return };
    let to_ball = sim.ball_position() - head_at;
    // The turn from facing straight ahead to facing the ball, in the body's
    // own terms, kept within what a neck can do.
    let local = body.rotation.inverse() * to_ball;
    let yaw = local.x.atan2(local.z).clamp(-LOOK_YAW, LOOK_YAW);
    let pitch = (-local.y).atan2(Vec2::new(local.x, local.z).length()).clamp(-LOOK_PITCH, LOOK_PITCH);
    let turn = body.rotation * Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * body.rotation.inverse();
    let turn = Quat::IDENTITY.slerp(turn, character.look_weight);
    // The neck turns part of the way; the head, riding on it, the rest.
    for (bone, share) in [(neck, NECK_SHARE), (head, 1.0 - NECK_SHARE)] {
        let Ok(global) = globals.get(bone).copied() else { continue };
        let (scale, rotation, translation) = global.to_scale_rotation_translation();
        let part = Quat::IDENTITY.slerp(turn, share);
        let posed = GlobalTransform::from(Transform { translation, rotation: part * rotation, scale });
        follow_parent(bone, posed, locals, children, globals);
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
