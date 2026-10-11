//! The match simulation behind a C interface, for clients outside Rust: the
//! Unreal test (docs/unreal-test.md) loads it as a DLL. `include/volley_sim.h`
//! declares the same functions and structs for C and C++; keep the two in step.
//!
//! The sim's world is Y up, in meters, with the net along x = 0. Everything here
//! keeps those units and axes; converting to an engine's is the client's job.

use volley_sim::court::BALL_RADIUS;
use volley_sim::moves::{ALL_ROUNDER, CROSS, GOLAZO};
use volley_sim::{Aim, Ball, Event, HitKind, MatchConfig, Phase, PlayerInput, Sim, Vec2, Vec3, bot};

/// A match, and the events of its latest tick.
pub struct VolleyMatch {
    pub sim: Sim,
    events: Vec<Event>,
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
}

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
    let mut sim = Sim::new(MatchConfig::default());
    let lineup = [[ALL_ROUNDER, ALL_ROUNDER, ALL_ROUNDER], [CROSS, GOLAZO, ALL_ROUNDER]];
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
    Box::into_raw(Box::new(VolleyMatch { sim: new_match(), events: Vec::new() }))
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
    let sim = &unsafe { &*game }.sim;
    let ball = sim.ball_position();
    let (ball_state, ball_holder) = match sim.ball {
        Ball::Dead { .. } => (BALL_DEAD, -1),
        Ball::Held { by } => (BALL_HELD, by as i32),
        Ball::InFlight(_) => (BALL_IN_FLIGHT, -1),
        Ball::Carried { by, .. } => (BALL_CARRIED, by as i32),
    };
    let landing = sim.landing_point();
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
        })
    };
    let count = sim.players.len().min(max_players as usize);
    for (i, player) in sim.players.iter().take(count).enumerate() {
        let p = player.position;
        unsafe {
            players.add(i).write(VolleyPlayerState {
                x: p.x,
                y: p.y,
                z: p.z,
                vx: player.velocity.x,
                vy: player.vertical_velocity,
                vz: player.velocity.y,
                team: player.team as u32,
                stunned: (player.stunned_until > sim.tick) as u8,
            })
        };
    }
    count as u32
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
        assert_eq!(size_of::<VolleyPlayerState>(), 32);
        assert_eq!(size_of::<VolleyMatchState>(), 88);
        assert_eq!(size_of::<VolleyEvent>(), 44);
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
