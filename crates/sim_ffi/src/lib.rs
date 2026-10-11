//! The match simulation behind a C interface, for clients outside Rust: the
//! Unreal test (docs/unreal-test.md) loads it as a DLL. `include/volley_sim.h`
//! declares the same functions and structs for C and C++; keep the two in step.
//!
//! The sim's world is Y up, in meters, with the net along x = 0. Everything here
//! keeps those units and axes; converting to an engine's is the client's job.

use volley_sim::attack;
use volley_sim::court::BALL_RADIUS;
use volley_sim::moves::{ALL_ROUNDER, CROSS, GOLAZO};
use volley_sim::moves::Button;
use volley_sim::{Aim, Ball, DT, Event, HitKind, MatchConfig, MoveId, MovePhase, Passive, Phase, PlayerInput, Sim, Vec2, Vec3, bot};

/// A match, and the events of its latest tick.
pub struct VolleyMatch {
    pub sim: Sim,
    events: Vec<Event>,
    /// Each player's near future, from the match stepped ahead.
    ahead: Vec<Ahead>,
}

/// One player's controls for one tick, as [`PlayerInput`]. Flags are 0 or 1.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VolleyInput {
    /// World XZ direction, length at most 1.
    pub move_x: f32,
    pub move_z: f32,
    /// 1: aim toward (aim_x, aim_z) with a boost of aim_power (0 to 1).
    /// 0: the default spot for the hit.
    pub has_aim: u8,
    pub aim_x: f32,
    pub aim_z: f32,
    pub aim_power: f32,
    pub jump: u8,
    pub jump_released: u8,
    pub pass: u8,
    pub pass_held: u8,
    pub spike: u8,
    pub dive: u8,
    pub kick: u8,
    pub dash: u8,
    pub ability: u8,
    pub ultimate: u8,
}

impl From<&VolleyInput> for PlayerInput {
    fn from(input: &VolleyInput) -> Self {
        PlayerInput {
            movement: Vec2::new(input.move_x, input.move_z).clamp_length_max(1.0),
            aim: (input.has_aim != 0).then(|| Aim::Toward {
                direction: Vec2::new(input.aim_x, input.aim_z),
                power: input.aim_power,
            }),
            jump: input.jump != 0,
            jump_released: input.jump_released != 0,
            pass: input.pass != 0,
            pass_held: input.pass_held != 0,
            spike: input.spike != 0,
            dive: input.dive != 0,
            kick: input.kick != 0,
            dash: input.dash != 0,
            ability: input.ability != 0,
            ultimate: input.ultimate != 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VolleyPlayerState {
    /// Feet.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub team: u32,
    /// 1 while knocked down.
    pub stunned: u8,
    /// Hands up at the net, in the air.
    pub blocking: u8,
    pub grounded: u8,
    /// 1 if `action_*` holds a move (playing or just finished).
    pub has_action: u8,
    /// `KIT_*`.
    pub kit: u32,
    /// The move (`MoveId::index`), its phase (`MOVE_*`), when it started and
    /// its lunge direction (world XZ).
    pub action_move: i32,
    pub action_phase: u32,
    pub action_start_tick: u32,
    pub action_dir_x: f32,
    pub action_dir_z: f32,
    /// The move that can touch the ball right now, or -1.
    pub active_move: i32,
    /// The latest dash, and its direction (world XZ).
    pub dash_dir_x: f32,
    pub dash_dir_z: f32,
    pub dash_start_tick: u32,
    pub has_dash: u8,
    /// Carrying the ball (a crossover), shifting sideways at (carry_vx, carry_vz).
    pub has_carry: u8,
    /// Touched the ball last: touching it again would be a fault.
    pub must_not_touch: u8,
    /// The kit's passives that change how a player looks: Feet (Golazo does
    /// everything with his feet) and NoLook (Cross doesn't turn to his hits).
    pub feet: u8,
    pub carry_vx: f32,
    pub carry_vz: f32,
    /// When the active move will meet the ball, within a second: ticks from
    /// now (-1 if not) and where the ball will be.
    pub contact_ticks: i32,
    pub contact_x: f32,
    pub contact_y: f32,
    pub contact_z: f32,
    /// The attack technique (`hit_kind`: spike, volley or bicycle) for the
    /// ball at contact, or the ball now if there's no contact.
    pub technique: u32,
    pub no_look: u8,
    /// About to jump: ticks until the feet leave the ground (-1 if not
    /// within `GATHER_TICKS`), for the gather before a jump. From the match
    /// stepped ahead with the bots' inputs; a human's jumps can't be known.
    pub takeoff_ticks: i32,
    /// Where the feet will be (world XZ) `FUTURE_TICKS` from now, for motion
    /// matching: the same look ahead.
    pub future_x: [f32; 3],
    pub future_z: [f32; 3],
}

/// The ticks `future_x`/`future_z` look ahead: 0.2, 0.4 and 0.6 s.
pub const FUTURE_TICKS: [u32; 3] = [12, 24, 36];

/// A player's near future, from the match stepped ahead on a copy.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ahead {
    /// Ticks until the feet leave the ground, within `GATHER_TICKS`.
    pub takeoff: Option<u32>,
    /// Where the feet are at `FUTURE_TICKS`.
    pub future: [Vec2; 3],
}

/// How far ahead (ticks) `takeoff_ticks` looks, as the Bevy client's gather.
pub const GATHER_TICKS: u32 = 8;

pub const KIT_ALL_ROUNDER: u32 = 0;
pub const KIT_CROSS: u32 = 1;
pub const KIT_GOLAZO: u32 = 2;

pub const MOVE_WINDUP: u32 = 0;
pub const MOVE_ACTIVE: u32 = 1;
pub const MOVE_RECOVERY: u32 = 2;
/// The action is still set but its move has played out.
pub const MOVE_OVER: u32 = 3;

/// How far ahead `contact_ticks` looks: a second, as the Bevy client does.
const CONTACT_HORIZON: u32 = 60;

pub const BALL_DEAD: u32 = 0;
pub const BALL_HELD: u32 = 1;
pub const BALL_IN_FLIGHT: u32 = 2;
pub const BALL_CARRIED: u32 = 3;

pub const PHASE_RALLY: u32 = 0;
pub const PHASE_POINT_SCORED: u32 = 1;
pub const PHASE_MATCH_OVER: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VolleyMatchState {
    pub tick: u32,
    /// Increments each rally, so a reset can be told from movement.
    pub rally: u32,
    /// `PHASE_*`.
    pub phase: u32,
    pub score: [u32; 2],
    pub sets: [u32; 2],
    pub set: u32,
    /// Ball center, `BALL_*`, and who holds or carries it (-1 for nobody).
    pub ball_x: f32,
    pub ball_y: f32,
    pub ball_z: f32,
    pub ball_state: u32,
    pub ball_holder: i32,
    pub ball_radius: f32,
    /// Where the ball will land, if it's in flight.
    pub has_landing: u8,
    pub landing_x: f32,
    pub landing_y: f32,
    pub landing_z: f32,
    /// A decoy ball beside the real one, after a split.
    pub has_decoy: u8,
    pub decoy_x: f32,
    pub decoy_y: f32,
    pub decoy_z: f32,
    /// The ball's velocity, while in flight.
    pub ball_vx: f32,
    pub ball_vy: f32,
    pub ball_vz: f32,
}

/// What `VolleyEvent::kind` can be, one per [`Event`] variant.
pub const EVENT_MOVE_STARTED: u32 = 0;
pub const EVENT_DASHED: u32 = 1;
pub const EVENT_CARRIED: u32 = 2;
pub const EVENT_DRIBBLED: u32 = 3;
pub const EVENT_CHAINED: u32 = 4;
pub const EVENT_POSTERIZED: u32 = 5;
pub const EVENT_SPLIT: u32 = 6;
pub const EVENT_DECOY_POPPED: u32 = 7;
pub const EVENT_TOUCHED: u32 = 8;
pub const EVENT_BLOCKED: u32 = 9;
pub const EVENT_HIT_NET: u32 = 10;
pub const EVENT_LANDED: u32 = 11;
pub const EVENT_WALL_BOUNCE: u32 = 12;
pub const EVENT_WALL_JUMPED: u32 = 13;
pub const EVENT_POINT: u32 = 14;
pub const EVENT_SIDES_SWITCHED: u32 = 15;
pub const EVENT_SET_WON: u32 = 16;
pub const EVENT_MATCH_WON: u32 = 17;

/// One event, flattened. Fields an event doesn't use are 0 (`player` and
/// `team` are -1).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VolleyEvent {
    pub kind: u32,
    pub player: i32,
    pub team: i32,
    /// MoveStarted: the move (`MoveId::index`). Touched: the hit (`hit_kind`).
    /// Blocked: 1 if stuffed. Point: the reason (0 landed in, 1 too many
    /// touches, 2 double touch).
    pub detail: u32,
    /// Touched: how cleanly an attack was hit, 0 to 1.
    pub quality: f32,
    /// Where it happened (HitNet, Landed, WallBounce), or a direction
    /// (WallJumped: the way off the wall, in x and z).
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Landed: the ball's velocity as it hit the floor.
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

impl VolleyEvent {
    fn new(kind: u32) -> Self {
        Self { kind, player: -1, team: -1, detail: 0, quality: 0.0, x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 }
    }

    fn by(kind: u32, player: usize) -> Self {
        Self { player: player as i32, ..Self::new(kind) }
    }

    fn at(kind: u32, at: Vec3) -> Self {
        Self { x: at.x, y: at.y, z: at.z, ..Self::new(kind) }
    }
}

/// What the HUD shows for a player (the Bevy client's hud.rs and aim.rs).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VolleyHud {
    /// Where their next hit would land (world, Y up), how far off a hit from a
    /// bad position may stray (m), and its kind (`hit_kind`).
    pub aim_x: f32,
    pub aim_y: f32,
    pub aim_z: f32,
    pub aim_spread: f32,
    pub aim_kind: u32,
    /// The ball is theirs to play: held or carried by them, or coming down on
    /// their side and theirs to touch. The aim marker shows then.
    pub yours: u8,
    /// `PROMPT_*`: a foot save or a dive is the move right now.
    pub prompt: u8,
    pub has_ability: u8,
    pub has_ultimate: u8,
    /// Seconds until the ability can be used again (0: ready).
    pub ability_cooldown: f32,
    /// The ultimate's charge, 0 to 1.
    pub charge: f32,
    /// Points needed to win the set being played.
    pub points_to_win: u32,
    /// The moves' names, zero-terminated.
    pub ability_name: [u8; 24],
    pub ultimate_name: [u8; 24],
}

pub const PROMPT_NONE: u8 = 0;
pub const PROMPT_FOOT_SAVE: u8 = 1;
pub const PROMPT_DIVE: u8 = 2;

/// How far ahead (ticks) the save prompt looks.
const PROMPT_LOOKAHEAD_TICKS: u32 = 30;

fn name_bytes(name: &str) -> [u8; 24] {
    let mut out = [0u8; 24];
    for (slot, byte) in out.iter_mut().zip(name.bytes().take(23)) {
        *slot = byte;
    }
    out
}

/// A foot save when the ball is about to come by too low for the arms but
/// within a foot; a dive when it will land too far to run to (hud.rs's
/// `save_prompt`).
pub fn save_prompt(sim: &Sim, me: usize) -> u8 {
    let player = &sim.players[me];
    let Ball::InFlight(flight) = sim.ball else { return PROMPT_NONE };
    let busy = player.action.is_some_and(|action| action.id.spec().lunge.is_some());
    if sim.must_not_touch(me) || busy || !player.grounded() {
        return PROMPT_NONE;
    }
    let upcoming = || (1..=PROMPT_LOOKAHEAD_TICKS).map(|ahead| flight.position_at(sim.tick + ahead));
    let arms = upcoming().any(|ball| player.reaches(MoveId::Pass, ball));
    let feet = player.kit.has_move(MoveId::FootSave) && upcoming().any(|ball| player.reaches(MoveId::FootSave, ball));
    if feet && !arms {
        return PROMPT_FOOT_SAVE;
    }
    let landing = flight.landing_point();
    let seconds_left = flight.landing_time() - flight.elapsed(sim.tick);
    let distance = Vec2::new(landing.x - player.position.x, landing.z - player.position.z).length();
    let dive = MoveId::Dive.spec();
    let dive_reach = dive.lunge.unwrap_or_default() * dive.active as f32 * DT + dive.reach;
    let ours = sim.team_on(landing.x) == player.team;
    if ours && seconds_left < 0.6 && distance > player.kit.run_speed * seconds_left + 1.0 && distance < dive_reach {
        PROMPT_DIVE
    } else {
        PROMPT_NONE
    }
}

/// The HUD's view of player `me`.
pub fn hud(sim: &Sim, me: usize) -> VolleyHud {
    let player = &sim.players[me];
    let preview = sim.preview_hit(me);
    let yours = match sim.ball {
        Ball::Held { by } | Ball::Carried { by, .. } => by == me,
        Ball::InFlight(flight) => sim.team_on(flight.landing_point().x) == player.team && !sim.must_not_touch(me),
        Ball::Dead { .. } => false,
    };
    let ability = player.kit.move_on(Button::Ability);
    let ultimate = player.kit.move_on(Button::Ultimate);
    VolleyHud {
        aim_x: preview.target.x,
        aim_y: preview.target.y,
        aim_z: preview.target.z,
        aim_spread: preview.spread,
        aim_kind: hit_kind(preview.kind),
        yours: yours as u8,
        prompt: save_prompt(sim, me),
        has_ability: ability.is_some() as u8,
        has_ultimate: ultimate.is_some() as u8,
        ability_cooldown: ability.map_or(0.0, |id| if player.ready(id, sim.tick) { 0.0 } else { player.cooldown_left(id, sim.tick) }),
        charge: player.charge,
        points_to_win: sim.points_to_win_set(),
        ability_name: name_bytes(ability.map_or("", |id| id.spec().name)),
        ultimate_name: name_bytes(ultimate.map_or("", |id| id.spec().name)),
    }
}

/// A small number for each kind of hit, in `HitKind`'s order.
pub fn hit_kind(kind: HitKind) -> u32 {
    match kind {
        HitKind::Serve => 0,
        HitKind::Pass => 1,
        HitKind::Dig => 2,
        HitKind::Kick => 3,
        HitKind::Lob => 4,
        HitKind::Spike => 5,
        HitKind::Volley => 6,
        HitKind::Bicycle => 7,
        HitKind::Dunk => 8,
        HitKind::Curve => 9,
    }
}

impl From<&Event> for VolleyEvent {
    fn from(event: &Event) -> Self {
        match *event {
            Event::MoveStarted { player, id } => {
                VolleyEvent { detail: id.index() as u32, ..VolleyEvent::by(EVENT_MOVE_STARTED, player) }
            }
            Event::Dashed { player } => VolleyEvent::by(EVENT_DASHED, player),
            Event::Carried { player } => VolleyEvent::by(EVENT_CARRIED, player),
            Event::Dribbled { player } => VolleyEvent::by(EVENT_DRIBBLED, player),
            Event::Chained { player } => VolleyEvent::by(EVENT_CHAINED, player),
            Event::Posterized { player } => VolleyEvent::by(EVENT_POSTERIZED, player),
            Event::Split { player } => VolleyEvent::by(EVENT_SPLIT, player),
            Event::DecoyPopped { player } => VolleyEvent::by(EVENT_DECOY_POPPED, player),
            Event::Touched { player, kind, quality } => {
                VolleyEvent { detail: hit_kind(kind), quality, ..VolleyEvent::by(EVENT_TOUCHED, player) }
            }
            Event::Blocked { player, stuffed } => {
                VolleyEvent { detail: stuffed as u32, ..VolleyEvent::by(EVENT_BLOCKED, player) }
            }
            Event::HitNet { at } => VolleyEvent::at(EVENT_HIT_NET, at),
            Event::Landed { at, velocity } => VolleyEvent {
                vx: velocity.x,
                vy: velocity.y,
                vz: velocity.z,
                ..VolleyEvent::at(EVENT_LANDED, at)
            },
            Event::WallBounce { at } => VolleyEvent::at(EVENT_WALL_BOUNCE, at),
            Event::WallJumped { player, away } => {
                VolleyEvent { x: away.x, z: away.y, ..VolleyEvent::by(EVENT_WALL_JUMPED, player) }
            }
            Event::Point { team, reason } => VolleyEvent {
                team: team as i32,
                detail: reason as u32,
                ..VolleyEvent::new(EVENT_POINT)
            },
            Event::SidesSwitched => VolleyEvent::new(EVENT_SIDES_SWITCHED),
            Event::SetWon { team } => VolleyEvent { team: team as i32, ..VolleyEvent::new(EVENT_SET_WON) },
            Event::MatchWon { team } => VolleyEvent { team: team as i32, ..VolleyEvent::new(EVENT_MATCH_WON) },
        }
    }
}

/// The game's lineup, as in the client's `heroes::new_match` with an
/// All-rounder for the player: Red is three All-rounders, Blue a Cross, a
/// Golazo and an All-rounder.
pub fn new_match() -> Sim {
    new_match_as(KIT_ALL_ROUNDER)
}

/// The lineup with Red's first player (the one a person plays) as `kit`
/// (`KIT_*`), two All-rounder teammates, against a Cross, a Golazo and an
/// All-rounder: the client's `heroes::new_match(choice)`.
pub fn new_match_as(kit: u32) -> Sim {
    let mut sim = Sim::new(MatchConfig::default());
    let hero = match kit {
        KIT_CROSS => CROSS,
        KIT_GOLAZO => GOLAZO,
        _ => ALL_ROUNDER,
    };
    let lineup = [[hero, ALL_ROUNDER, ALL_ROUNDER], [CROSS, GOLAZO, ALL_ROUNDER]];
    for (team, kits) in lineup.into_iter().enumerate() {
        for (slot, kit) in kits.into_iter().enumerate() {
            sim.set_kit(sim.player_index(team, slot), kit);
        }
    }
    sim
}

/// One tick with bots playing everyone except `human` (if any), who plays
/// `input`.
pub fn step(sim: &mut Sim, human: Option<usize>, input: PlayerInput) -> Vec<Event> {
    let inputs: Vec<_> =
        (0..sim.players.len()).map(|i| if Some(i) == human { input } else { bot::input_for(sim, i) }).collect();
    sim.step(&inputs)
}

/// A fingerprint of the whole match state (FNV-1a of its debug form), to check
/// that two runs are the same tick for tick.
pub fn checksum(sim: &Sim) -> u64 {
    format!("{sim:?}").bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3))
}

/// A new 3v3 match with the game's lineup (see [`new_match`]). Free it with
/// `volley_match_free`.
#[unsafe(no_mangle)]
pub extern "C" fn volley_match_new() -> *mut VolleyMatch {
    Box::into_raw(Box::new(VolleyMatch { sim: new_match(), events: Vec::new(), ahead: Vec::new() }))
}

/// A new match with Red's first player as `kit` (`KIT_*`): see [`new_match_as`].
#[unsafe(no_mangle)]
pub extern "C" fn volley_match_new_as(kit: u32) -> *mut VolleyMatch {
    Box::into_raw(Box::new(VolleyMatch { sim: new_match_as(kit), events: Vec::new(), ahead: Vec::new() }))
}

/// # Safety
/// `game` comes from `volley_match_new` and isn't used afterwards. Null is fine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_free(game: *mut VolleyMatch) {
    if !game.is_null() {
        drop(unsafe { Box::from_raw(game) });
    }
}

/// # Safety
/// `game` comes from `volley_match_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_player_count(game: *const VolleyMatch) -> u32 {
    unsafe { &*game }.sim.players.len() as u32
}

/// Advances one tick (1/60 s). Bots play everyone, except player `human` (if
/// 0 or more), who plays `input`. Returns the number of events this tick; read
/// them with `volley_match_events`.
///
/// # Safety
/// `game` comes from `volley_match_new`; `input` points to a valid input when
/// `human` is 0 or more.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_step(game: *mut VolleyMatch, human: i32, input: *const VolleyInput) -> u32 {
    let game = unsafe { &mut *game };
    let human = usize::try_from(human).ok().filter(|&i| i < game.sim.players.len());
    let input = match human {
        Some(_) if !input.is_null() => PlayerInput::from(unsafe { &*input }),
        _ => PlayerInput::default(),
    };
    game.events = step(&mut game.sim, human, input);
    game.ahead = look_ahead(&game.sim, human);
    game.events.len() as u32
}

/// Copies up to `max` of the latest tick's events into `out`, returning how
/// many it copied.
///
/// # Safety
/// `game` comes from `volley_match_new`; `out` has room for `max` events.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_events(game: *const VolleyMatch, out: *mut VolleyEvent, max: u32) -> u32 {
    let game = unsafe { &*game };
    let count = game.events.len().min(max as usize);
    for (i, event) in game.events.iter().take(count).enumerate() {
        unsafe { out.add(i).write(VolleyEvent::from(event)) };
    }
    count as u32
}

/// Fills `state` with the match, and `players` with up to `max_players`
/// players (team 0's first). Returns how many players it filled.
///
/// # Safety
/// `game` comes from `volley_match_new`; `state` is valid; `players` has room
/// for `max_players`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_state(
    game: *const VolleyMatch,
    state: *mut VolleyMatchState,
    players: *mut VolleyPlayerState,
    max_players: u32,
) -> u32 {
    let game = unsafe { &*game };
    let sim = &game.sim;
    let ball = sim.ball_position();
    let (ball_state, ball_holder) = match sim.ball {
        Ball::Dead { .. } => (BALL_DEAD, -1),
        Ball::Held { by } => (BALL_HELD, by as i32),
        Ball::InFlight(_) => (BALL_IN_FLIGHT, -1),
        Ball::Carried { by, .. } => (BALL_CARRIED, by as i32),
    };
    let landing = sim.landing_point();
    let velocity = match sim.ball {
        Ball::InFlight(flight) => flight.velocity_at_time(flight.elapsed(sim.tick)),
        _ => Vec3::ZERO,
    };
    let decoy = sim.decoy().map(|flight| flight.position_at(sim.tick));
    let phase = match sim.phase {
        Phase::Rally => PHASE_RALLY,
        Phase::PointScored { .. } => PHASE_POINT_SCORED,
        Phase::MatchOver { .. } => PHASE_MATCH_OVER,
    };
    unsafe {
        state.write(VolleyMatchState {
            tick: sim.tick,
            rally: sim.rally,
            phase,
            score: sim.score,
            sets: sim.sets,
            set: sim.set,
            ball_x: ball.x,
            ball_y: ball.y,
            ball_z: ball.z,
            ball_state,
            ball_holder,
            ball_radius: BALL_RADIUS,
            has_landing: landing.is_some() as u8,
            landing_x: landing.map_or(0.0, |p| p.x),
            landing_y: landing.map_or(0.0, |p| p.y),
            landing_z: landing.map_or(0.0, |p| p.z),
            has_decoy: decoy.is_some() as u8,
            decoy_x: decoy.map_or(0.0, |p| p.x),
            decoy_y: decoy.map_or(0.0, |p| p.y),
            decoy_z: decoy.map_or(0.0, |p| p.z),
            ball_vx: velocity.x,
            ball_vy: velocity.y,
            ball_vz: velocity.z,
        })
    };
    let count = sim.players.len().min(max_players as usize);
    for i in 0..count {
        let ahead = game.ahead.get(i).copied().unwrap_or_default();
        unsafe { players.add(i).write(player_state(sim, i, ahead)) };
    }
    count as u32
}

/// Everything a client needs to animate player `i`.
pub fn player_state(sim: &Sim, i: usize, ahead: Ahead) -> VolleyPlayerState {
    let player = &sim.players[i];
    let p = player.position;
    let kit = match player.kit.name {
        name if name == CROSS.name => KIT_CROSS,
        name if name == GOLAZO.name => KIT_GOLAZO,
        _ => KIT_ALL_ROUNDER,
    };
    let action = player.action;
    let action_phase = action.map_or(MOVE_OVER, |action| match action.phase(sim.tick) {
        Some(MovePhase::Windup) => MOVE_WINDUP,
        Some(MovePhase::Active) => MOVE_ACTIVE,
        Some(MovePhase::Recovery) => MOVE_RECOVERY,
        None => MOVE_OVER,
    });
    let contact = sim.predicted_contact(i, CONTACT_HORIZON);
    let ball = contact.map_or(sim.ball_position(), |(_, at)| at);
    let (technique, _) = attack::best_technique(player, ball);
    VolleyPlayerState {
        x: p.x,
        y: p.y,
        z: p.z,
        vx: player.velocity.x,
        vy: player.vertical_velocity,
        vz: player.velocity.y,
        team: player.team as u32,
        stunned: (player.stunned_until > sim.tick) as u8,
        blocking: player.blocking() as u8,
        grounded: player.grounded() as u8,
        has_action: action.is_some() as u8,
        kit,
        action_move: action.map_or(-1, |action| action.id.index() as i32),
        action_phase,
        action_start_tick: action.map_or(0, |action| action.start_tick),
        action_dir_x: action.map_or(0.0, |action| action.direction.x),
        action_dir_z: action.map_or(0.0, |action| action.direction.y),
        active_move: player.active_move(sim.tick).map_or(-1, |id| id.index() as i32),
        dash_dir_x: player.dash.map_or(0.0, |dash| dash.direction.x),
        dash_dir_z: player.dash.map_or(0.0, |dash| dash.direction.y),
        dash_start_tick: player.dash.map_or(0, |dash| dash.start_tick),
        has_dash: player.dash.is_some() as u8,
        has_carry: player.carry.is_some() as u8,
        must_not_touch: sim.must_not_touch(i) as u8,
        feet: player.kit.has(Passive::Feet) as u8,
        carry_vx: player.carry.map_or(0.0, |carry| carry.velocity.x),
        carry_vz: player.carry.map_or(0.0, |carry| carry.velocity.y),
        contact_ticks: contact.map_or(-1, |(ticks, _)| ticks as i32),
        contact_x: contact.map_or(0.0, |(_, at)| at.x),
        contact_y: contact.map_or(0.0, |(_, at)| at.y),
        contact_z: contact.map_or(0.0, |(_, at)| at.z),
        technique: hit_kind(technique),
        no_look: player.kit.has(Passive::NoLook) as u8,
        takeoff_ticks: ahead.takeoff.map_or(-1, |ticks| ticks as i32),
        future_x: ahead.future.map(|spot| spot.x),
        future_z: ahead.future.map(|spot| spot.y),
    }
}

/// Each player's near future: the match stepped ahead on a copy, bots playing
/// (they're deterministic) and `human` standing still. Takeoffs within
/// `GATHER_TICKS`, as the Bevy client's `look_ahead`; and where everyone will
/// be at `FUTURE_TICKS`.
pub fn look_ahead(sim: &Sim, human: Option<usize>) -> Vec<Ahead> {
    let here = |sim: &Sim, i: usize| Vec2::new(sim.players[i].position.x, sim.players[i].position.z);
    let mut ahead: Vec<Ahead> =
        (0..sim.players.len()).map(|i| Ahead { takeoff: None, future: [here(sim, i); 3] }).collect();
    if sim.phase != Phase::Rally {
        return ahead;
    }
    let mut future = sim.clone();
    for k in 1..=FUTURE_TICKS[2] {
        let grounded: Vec<bool> = future.players.iter().map(|p| p.grounded()).collect();
        step(&mut future, human, PlayerInput::default());
        for (i, player) in future.players.iter().enumerate() {
            if k <= GATHER_TICKS && ahead[i].takeoff.is_none() && grounded[i] && !player.grounded() {
                ahead[i].takeoff = Some(k);
            }
            for (slot, &ticks) in FUTURE_TICKS.iter().enumerate() {
                if k <= ticks {
                    ahead[i].future[slot] = here(&future, i);
                }
            }
        }
        // A new rally resets everyone: the future stops at the point's end.
        if future.phase != Phase::Rally {
            break;
        }
    }
    ahead
}

/// Fills `out` with what the HUD shows for player `player`: see [`hud`].
///
/// # Safety
/// `game` comes from `volley_match_new`; `out` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_hud(game: *const VolleyMatch, player: u32, out: *mut VolleyHud) {
    let sim = &unsafe { &*game }.sim;
    let player = (player as usize).min(sim.players.len() - 1);
    unsafe { out.write(hud(sim, player)) };
}

/// See [`checksum`].
///
/// # Safety
/// `game` comes from `volley_match_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn volley_match_checksum(game: *const VolleyMatch) -> u64 {
    checksum(&unsafe { &*game }.sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C interface plays the same match as the sim used directly.
    #[test]
    fn same_match_through_the_c_interface() {
        let mut direct = new_match();
        let game = volley_match_new();
        let mut events = vec![VolleyEvent::new(0); 64];
        for _ in 0..1800 {
            let expected: Vec<VolleyEvent> = step(&mut direct, None, PlayerInput::default()).iter().map(Into::into).collect();
            let count = unsafe { volley_match_step(game, -1, std::ptr::null()) };
            let copied = unsafe { volley_match_events(game, events.as_mut_ptr(), events.len() as u32) };
            assert_eq!(count, copied);
            assert_eq!(format!("{expected:?}"), format!("{:?}", &events[..copied as usize]));
        }
        assert_eq!(checksum(&direct), unsafe { volley_match_checksum(game) });
        unsafe { volley_match_free(game) };
    }

    /// The C structs' sizes, which the Unreal module checks too
    /// (`static_assert`s in VolleySim.h), so a field added on one side only
    /// fails to build.
    #[test]
    fn struct_sizes() {
        assert_eq!(size_of::<VolleyInput>(), 36);
        assert_eq!(size_of::<VolleyPlayerState>(), 136);
        assert_eq!(size_of::<VolleyMatchState>(), 100);
        assert_eq!(size_of::<VolleyEvent>(), 44);
        assert_eq!(size_of::<VolleyHud>(), 84);
    }

    #[test]
    fn state_matches_the_sim() {
        let game = volley_match_new();
        for _ in 0..600 {
            unsafe { volley_match_step(game, -1, std::ptr::null()) };
        }
        let mut state = VolleyMatchState::default();
        let mut players = [VolleyPlayerState::default(); 8];
        let count = unsafe { volley_match_state(game, &mut state, players.as_mut_ptr(), 8) };
        let sim = &unsafe { &*game }.sim;
        assert_eq!(count, 6);
        assert_eq!(state.tick, 600);
        assert_eq!(Vec3::new(state.ball_x, state.ball_y, state.ball_z), sim.ball_position());
        for (state, player) in players.iter().zip(&sim.players) {
            assert_eq!(Vec3::new(state.x, state.y, state.z), player.position);
        }
        unsafe { volley_match_free(game) };
    }
}
