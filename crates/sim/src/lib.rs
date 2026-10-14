//! The match simulation: every rule that decides what happens in a match.
//!
//! Pure Rust with no engine, rendering or networking, so the server, the client
//! and the tests all run exactly the same rules. Advance it with [`Sim::step`]
//! at [`TICK_HZ`]; the same state and inputs always produce the same result.

pub mod assist;
pub mod attack;
mod ball;
pub mod bot;
pub mod court;
pub mod moves;
mod player;
pub mod reception;

pub use ball::{Ball, Flight};
pub use glam::{Vec2, Vec3};
pub use moves::{Kit, Move, MoveId, Passive};
pub use player::{Action, Carry, Dash, MovePhase, Player};

use moves::Touch;
use court::{BALL_RADIUS, BODY_BOTTOM, BODY_RADIUS, BODY_TOP, HALF_LENGTH, HALF_WIDTH, NET_HEIGHT, NET_HALF_WIDTH, PANELS};

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

pub(crate) const MAX_TOUCHES: u32 = 3;
/// After a touch nobody can touch the ball for this long, so one swing never counts twice.
const TOUCH_LOCKOUT_TICKS: u32 = 10;
/// Over the net: a clean hit clears it by this much (m), the worst aims this
/// much short of clearing, and no lob takes longer than this to land (s). See
/// `clearing_seconds`.
const NET_CLEARANCE: f32 = 0.15;
const NET_RISK: f32 = 0.3;
const MAX_LOFT_SECONDS: f32 = 2.0;
const POINT_PAUSE_TICKS: u32 = 90;
/// A serve is swung, not flicked: the ball leaves the hand this long after the
/// press, as the swing reaches it.
pub const SERVE_SWING_TICKS: u32 = 15;
/// How long a player knocked down by a dunk stays down.
const KNOCKDOWN_TICKS: u32 = 60;
/// A ball that hits a player who isn't playing it bounces off them, losing
/// this share of its speed into them, and pops up more the higher it hit
/// (m/s, at the feet and at the head). Its way off swings up to this far
/// from the mirror bounce, as a body is no flat wall.
const BODY_RESTITUTION: f32 = 0.45;
const BODY_POP_LOW: f32 = 2.0;
const BODY_POP_HIGH: f32 = 6.5;
const BODY_SCATTER: f32 = 35.0 * std::f32::consts::PI / 180.0;
const BODY_MAX_SPEED: f32 = 9.0;
/// Balls from this fast (m/s) knock the player back, stunned, longer the
/// faster (s per m/s, up to a cap), and send them sliding back.
const BODY_KNOCK_SPEED: f32 = 8.0;
const BODY_STUN_PER_SPEED: f32 = 0.045;
const BODY_MAX_STUN: f32 = 1.3;
const BODY_SHOVE: f32 = 0.3;
const BODY_MAX_SHOVE: f32 = 7.0;
/// A reception this hard (difficulty) gone this wrong (error) shoves the
/// receiver back with a short stun (s, at the threshold and at the worst).
const SHOVE_DIFFICULTY: f32 = 0.55;
const SHOVE_ERROR: f32 = 0.3;
const SHOVE_STUN: (f32, f32) = (0.25, 0.7);
const SHOVE_SPEED: (f32, f32) = (2.5, 6.0);
/// An attack this soon after kicking off a wall (s) is a wall shot: cleaner,
/// faster, and as hard to receive as a hero's power.
const WALL_SHOT_SECONDS: f32 = 0.8;
const WALL_SHOT_QUALITY: f32 = 0.25;
const WALL_SHOT_SPEED: f32 = 0.15;
/// Flash Freeze: the ball hangs where it crossed the net this long (ticks).
const FREEZE_TICKS: u32 = TICK_HZ / 2;
/// Triple Axel's ice ball flies this much faster than a spike, and where it's
/// first received (or lands) it bursts into a ring of ice this wide (radius,
/// m) that lasts this long (ticks): everyone inside glides.
const ICE_SPEED: f32 = 0.15;
const ICE_RING_RADIUS: f32 = 1.5;
const ICE_TICKS: u32 = 2 * TICK_HZ;
/// Ultimate charge from each touch, and for each point the team wins.
const CHARGE_PER_TOUCH: f32 = 0.05;
/// How hard a banana kick bends, sideways (m/s²).
const BANANA_CURVE: f32 = 14.0;
/// A Chilena's decoy lands at least this far across the court from the real ball.
const DECOY_SPREAD: f32 = 8.0;
/// Balls coming off a wall are this much easier to reach for a hero with Wall Pass.
const WALL_PASS_REACH: f32 = 1.4;
const CHARGE_PER_POINT: f32 = 0.05;
const SET_PAUSE_TICKS: u32 = 240;

/// Default distance past the net for unaimed shots over it. Spikes go deep:
/// their flat path would otherwise catch the net unless hit from right beside it.
pub(crate) const OVER_DEPTH: f32 = HALF_LENGTH * 0.5;
const SPIKE_DEPTH: f32 = HALF_LENGTH * 0.6;
/// Default spot for a team's first touch: mid-court, for a teammate to set.
const RECEIVE_DEPTH: f32 = 4.0;
/// Aimed shots land at least this far from the walls.
const WALL_MARGIN: f32 = 1.0;
/// Default spot for a set: close to the net, for a teammate to spike.
pub(crate) const SET_DEPTH: f32 = 1.3;

/// Seconds a hit takes to travel `distance` meters: a base plus a little per
/// meter, so short shots are quick and flat and long ones stay playable.
pub(crate) fn flight_seconds(kind: HitKind, distance: f32) -> f32 {
    match kind {
        HitKind::Serve => 0.9 + 0.035 * distance,
        HitKind::Lob => 0.8 + 0.035 * distance,
        // Scrambles like digs and kicks add the move's own hang time on top.
        HitKind::Pass | HitKind::Dig | HitKind::Kick => 1.2 + 0.05 * distance,
        // Attacks depend on how cleanly they're hit.
        HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk | HitKind::Curve => {
            attack::flight_seconds(kind, distance, 1.0)
        }
    }
}

/// One player's controls for one tick. Buttons mean "pressed this tick", not "held".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// World-space XZ direction, length at most 1.
    pub movement: Vec2,
    /// Where to send the next hit. `None` uses the default spot for that kind of hit.
    pub aim: Option<Aim>,
    pub jump: bool,
    /// Jump was let go: a jump still rising is cut short into a hop.
    pub jump_released: bool,
    pub pass: bool,
    /// Pass is held down: a pass stays armed, waiting for the ball.
    pub pass_held: bool,
    pub spike: bool,
    pub dive: bool,
    pub kick: bool,
    /// A quick burst along the ground, toward `movement`.
    pub dash: bool,
    /// The hero's ability, and their ultimate.
    pub ability: bool,
    pub ultimate: bool,
}

/// Where a player sends their hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// A spot on the floor, world XZ.
    Spot(Vec2),
    /// A direction on the floor (world XZ), and a boost from 0 to 1 for a hit
    /// held a moment before it's made: a little farther and faster. Hits over
    /// the net always go toward the other side, keeping the direction's angle
    /// across the court.
    Toward { direction: Vec2, power: f32 },
}

/// How far a hit of `kind` goes when aimed by direction, in meters: a pass
/// reaches a teammate, an attack lands deep, a serve mid-way into the other half.
fn aim_distance(kind: HitKind) -> f32 {
    match kind {
        HitKind::Serve => 30.0,
        HitKind::Lob => 16.0,
        HitKind::Pass | HitKind::Dig | HitKind::Kick => 7.0,
        HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk | HitKind::Curve => 18.0,
    }
}

/// A fully boosted hit goes this much farther, and gets there this much sooner.
/// Small on purpose, Mario Tennis style: a tap is a good hit, holding a moment
/// makes it a little better, and the game stays quick.
const BOOST_RANGE: f32 = 0.2;
const BOOST_SPEED: f32 = 0.15;
/// A pass is clean taken this close to the body (horizontally, m), the body
/// behind the ball rather than stretching for it: a reception within
/// `CLEAN_RECEPTION` (it comes in fast and flat, so it's taken further out),
/// a set within `CLEAN_SET` (the ball drops onto the setter). Both are the
/// median of what bots manage. Two clean touches in a row (a clean pass, a
/// clean set) make the attack after them a chain: cleaner by `CHAIN_QUALITY`
/// and flying `CHAIN_SPEED` faster (as Switch Sports volleyball rewards good
/// touches with a power spike).
const CLEAN_RECEPTION: f32 = 1.12;
const CLEAN_SET: f32 = 0.65;
pub(crate) const CHAINED_TOUCHES: u32 = 2;
const CHAIN_QUALITY: f32 = 0.25;
const CHAIN_SPEED: f32 = 0.15;

/// How much a hit is boosted, from 0 to 1.
fn boost(aim: Option<Aim>) -> f32 {
    match aim {
        Some(Aim::Toward { power, .. }) => power.clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// The spot on the floor an aim sends a hit of `kind` from `from` to, for a
/// player on the half at `side`.
fn aim_spot(aim: Option<Aim>, kind: HitKind, from: Vec3, side: f32, over_net: bool) -> Option<Vec2> {
    match aim? {
        Aim::Spot(spot) => Some(spot),
        Aim::Toward { direction, power } => {
            let mut direction = direction.try_normalize().unwrap_or(Vec2::new(-side, 0.0));
            // Over the net means away from your own half.
            if over_net && direction.x * side > 0.0 {
                direction.x = -direction.x;
            }
            let distance = aim_distance(kind) * (1.0 + BOOST_RANGE * power.clamp(0.0, 1.0));
            Some(Vec2::new(from.x, from.z) + direction * distance)
        }
    }
}

/// Players per team and the scoring rules. The defaults are beach volleyball's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub players_per_team: usize,
    /// Points to win a set, and the deciding set. Either way, by two.
    pub set_points: u32,
    pub deciding_set_points: u32,
    pub sets_to_win: u32,
    /// Teams switch sides every this many points, and more often in the deciding set.
    pub switch_every: u32,
    pub deciding_switch_every: u32,
    /// Wall panels crack on a hit and break on the next, letting balls out.
    pub breakable_walls: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            players_per_team: 3,
            set_points: 21,
            deciding_set_points: 15,
            sets_to_win: 2,
            switch_every: 7,
            deciding_switch_every: 5,
            breakable_walls: true,
        }
    }
}

impl MatchConfig {
    /// The defaults with a different team size.
    pub fn with_players(players_per_team: usize) -> Self {
        Self { players_per_team, ..Self::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Rally,
    PointScored { resume_tick: u32 },
    MatchOver { winner: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Serve,
    /// Soft high ball to your own side, setting up the next touch.
    Pass,
    /// A pass made while diving.
    Dig,
    /// A foot save.
    Kick,
    /// Forced over the net on a team's last touch.
    Lob,
    Spike,
    /// An attack kicked in the air, for a ball too low to spike.
    Volley,
    /// An attack kicked over the head, flipping backwards, for a ball behind.
    Bicycle,
    /// A slam dunk, through any block.
    Dunk,
    /// Kicked with spin, bending around the block.
    Curve,
}

impl HitKind {
    /// Attacks: hit hard over the net from the air.
    pub fn is_attack(self) -> bool {
        matches!(self, HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk | HitKind::Curve)
    }
}

/// Where a hit would go, for showing an aim marker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitPreview {
    pub kind: HitKind,
    /// Where it's aimed to land.
    pub target: Vec3,
    /// It may land up to this far from `target`.
    pub spread: f32,
    /// For attacks, how cleanly it would be hit: 1 perfectly, 0 barely.
    pub quality: f32,
}

/// A hit before it's launched.
struct Plan {
    preview: HitPreview,
    seconds: f32,
    /// Sideways bend in flight (m/s²).
    curve: f32,
    /// A decoy ball flying alongside, landing here.
    decoy: Option<Vec3>,
    /// Sent by a hero's ability or ultimate: harder to receive.
    power: bool,
    /// Banked off the side wall nearest its target: the flight aims at the
    /// target's mirror image beyond that wall, and folds back onto it.
    bank: bool,
    /// Freezes as it crosses the net (Flash Freeze).
    freeze: bool,
    /// An ice ball (Triple Axel).
    ice: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointReason {
    LandedIn,
    TooManyTouches,
    DoubleTouch,
    /// Out through a broken wall panel: the team that didn't touch it last scores.
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A move began: a press, a dive, a foot save.
    MoveStarted { player: usize, id: MoveId },
    Dashed { player: usize },
    /// A hero caught the ball to carry it (a crossover); the attack follows.
    Carried { player: usize },
    /// Touched twice in a row, which only a dribbler may.
    Dribbled { player: usize },
    /// An attack after a clean pass and a clean set: harder and faster.
    Chained { player: usize },
    /// Knocked down by a dunk through their block.
    Posterized { player: usize },
    /// A hit split the ball in two: one of them is a decoy.
    Split { player: usize },
    /// Someone went for the decoy, and it vanished.
    DecoyPopped { player: usize },
    /// `quality`: how cleanly an attack was hit, from 0 to 1. Other hits are 1.
    Touched { player: usize, kind: HitKind, quality: f32 },
    /// A block at the net. `stuffed`: sent straight back down on the attackers;
    /// otherwise softened, popping up on the blocker's side.
    Blocked { player: usize, stuffed: bool },
    HitNet { at: Vec3 },
    /// `velocity` is the ball's as it hit the floor.
    Landed { at: Vec3, velocity: Vec3 },
    /// Bounced off an arena wall.
    WallBounce { at: Vec3 },
    /// Kicked off a wall in the air: a second jump. `away` is the way off the
    /// wall (world XZ).
    WallJumped { player: usize, away: Vec2 },
    /// The ball cracked a wall panel (`court::wall_panel`).
    WallCracked { panel: usize, at: Vec3 },
    /// The ball broke through a cracked panel, out.
    WallBroken { panel: usize, at: Vec3 },
    /// The ball went out through a broken panel.
    Out { at: Vec3 },
    /// The ball hit a player who wasn't playing it, and bounced off.
    /// `strength`: 0 for a tap, 1 for a ball that knocked them down.
    BodyHit { player: usize, at: Vec3, strength: f32 },
    /// A hard ball received badly pushed the receiver back, stunned.
    Shoved { player: usize, strength: f32 },
    /// An attack straight off a wall jump: harder and faster.
    WallShot { player: usize },
    /// A Flash Freeze ball stopped in mid-air as it crossed the net...
    BallFrozen { at: Vec3 },
    /// ...and goes on its way again.
    BallThawed { at: Vec3 },
    /// An ice ball burst into a ring of ice here (on the floor below).
    IceBurst { at: Vec3 },
    Point { team: usize, reason: PointReason },
    /// The teams will switch sides before the next rally.
    SidesSwitched,
    SetWon { team: usize },
    MatchWon { team: usize },
}

/// Touches by the team currently in possession.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Touches {
    team: usize,
    count: u32,
    last: Option<usize>,
    /// Someone has already touched twice in a row this possession.
    dribbled: bool,
    /// Clean passes in a row this possession: a clean pass and a clean set
    /// make the attack a chain.
    clean: u32,
}

impl Touches {
    fn new(team: usize) -> Self {
        Self { team, count: 0, last: None, dribbled: false, clean: 0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sim {
    pub config: MatchConfig,
    pub tick: u32,
    /// Team 0's players first, then team 1's.
    pub players: Vec<Player>,
    pub ball: Ball,
    /// Points in the current set.
    pub score: [u32; 2],
    /// Sets won.
    pub sets: [u32; 2],
    /// The set being played, from 1.
    pub set: u32,
    pub phase: Phase,
    /// Increments each rally, so observers can tell a reset from movement.
    pub rally: u32,
    pub rally_start_tick: u32,
    serving_team: usize,
    /// Which player on each team serves next.
    serve_rotation: [usize; 2],
    touches: Touches,
    touch_lockout_until: u32,
    /// The kind of the latest hit, and who made it.
    last_hit: Option<HitKind>,
    last_hitter: Option<usize>,
    /// Whether the latest hit was a hero's ability or ultimate.
    last_power: bool,
    /// A decoy ball in flight beside the real one, until either is touched.
    decoy: Option<Flight>,
    /// A serve underway: the tick the ball leaves the server's hand, and the
    /// move that swung it.
    serve_at: Option<(u32, MoveId)>,
    /// Each team's half: -1 or +1.
    sides: [f32; 2],
    /// Switch sides when the next rally starts.
    switch_pending: bool,
    /// Each wall panel: 0 whole, 1 cracked, 2 broken. Whole again each set.
    pub walls: [u8; PANELS],
    /// The ball in flight freezes when it next crosses the net.
    freeze_pending: bool,
    /// Ticks the ball stays frozen in mid-air.
    frozen_left: u32,
    /// The ball in flight is an ice ball, not yet burst.
    ice_ball: bool,
    /// The latest ring of ice: its center on the floor and the tick it melts.
    pub ice: Option<(Vec3, u32)>,
}

impl Sim {
    pub fn new(config: MatchConfig) -> Self {
        let sides = [-1.0, 1.0];
        let players = (0..2 * config.players_per_team)
            .map(|i| {
                let team = i / config.players_per_team;
                Player::new(team, sides[team], Vec3::ZERO)
            })
            .collect();
        let mut sim = Self {
            config,
            tick: 0,
            players,
            ball: Ball::Dead { at: Vec3::ZERO },
            score: [0, 0],
            sets: [0, 0],
            set: 1,
            phase: Phase::Rally,
            rally: 0,
            rally_start_tick: 0,
            serving_team: 0,
            serve_rotation: [0, 0],
            touches: Touches::new(0),
            touch_lockout_until: 0,
            last_hit: None,
            last_hitter: None,
            last_power: false,
            decoy: None,
            serve_at: None,
            sides,
            switch_pending: false,
            walls: [0; PANELS],
            freeze_pending: false,
            frozen_left: 0,
            ice_ball: false,
            ice: None,
        };
        sim.start_rally();
        sim
    }

    /// Advances the match one tick. `inputs` has one entry per player.
    pub fn step(&mut self, inputs: &[PlayerInput]) -> Vec<Event> {
        assert_eq!(inputs.len(), self.players.len(), "one input per player");
        let mut events = Vec::new();
        self.tick += 1;
        if matches!(self.phase, Phase::MatchOver { .. }) {
            return events;
        }

        if let Phase::PointScored { resume_tick } = self.phase
            && self.tick >= resume_tick
        {
            self.start_rally();
        }

        // Anyone in a ring of ice glides while it lasts.
        if let Some((center, until)) = self.ice
            && self.tick < until
        {
            for player in &mut self.players {
                if Vec2::new(player.position.x - center.x, player.position.z - center.z).length() <= ICE_RING_RADIUS {
                    player.ice_until = self.tick + 1;
                }
            }
        }
        let ball = self.ball_position();
        for (i, input) in inputs.iter().enumerate() {
            let serving = self.ball == Ball::Held { by: i };
            let started = self.players[i].update(input, self.tick, serving, ball);
            if let Some(id) = started.move_id {
                events.push(Event::MoveStarted { player: i, id });
            }
            if started.dash {
                events.push(Event::Dashed { player: i });
            }
            if let Some(away) = started.wall_jump {
                events.push(Event::WallJumped { player: i, away });
            }
        }

        match self.ball {
            Ball::Held { by } => self.update_serve(by, &mut events),
            Ball::InFlight(flight) if self.frozen_left > 0 => self.update_frozen(flight, &mut events),
            Ball::InFlight(flight) => self.update_flight(flight, &mut events),
            Ball::Carried { by, release_tick } => self.update_carry(by, release_tick, &mut events),
            Ball::Dead { .. } => {}
        }
        events
    }

    /// Gives `player` a hero's kit.
    pub fn set_kit(&mut self, player: usize, kit: Kit) {
        self.players[player].kit = kit;
    }

    /// Who hit the ball last, if anyone this rally.
    pub fn last_hitter(&self) -> Option<usize> {
        self.last_hitter
    }

    /// The decoy ball flying beside the real one, if a hit split it.
    pub fn decoy(&self) -> Option<Flight> {
        self.decoy
    }

    pub fn ball_position(&self) -> Vec3 {
        match self.ball {
            Ball::Held { by } => held_ball_position(&self.players[by]),
            Ball::InFlight(flight) => flight.position_at(self.tick),
            Ball::Carried { by, .. } => carried_ball_position(&self.players[by]),
            Ball::Dead { at } => at,
        }
    }

    /// Where the ball will come down if nobody touches it.
    pub fn landing_point(&self) -> Option<Vec3> {
        match self.ball {
            Ball::InFlight(flight) => Some(flight.landing_point()),
            _ => None,
        }
    }

    /// The half `team` plays on: -1 or +1.
    pub fn side(&self, team: usize) -> f32 {
        self.sides[team]
    }

    /// The team playing on the half that `x` is in.
    pub fn team_on(&self, x: f32) -> usize {
        if court::half_of(x) == self.sides[0] { 0 } else { 1 }
    }

    /// Whether this is the last possible set.
    pub fn deciding_set(&self) -> bool {
        self.set == 2 * self.config.sets_to_win - 1
    }

    /// Points needed to win the current set, by two.
    pub fn points_to_win_set(&self) -> u32 {
        if self.deciding_set() { self.config.deciding_set_points } else { self.config.set_points }
    }

    /// Whether the rules forbid `player` from touching the ball next: nobody touches
    /// twice in a row, except a player with no teammates to pass to.
    pub fn must_not_touch(&self, player: usize) -> bool {
        self.config.players_per_team > 1 && self.touches.last == Some(player) && !self.can_dribble(player)
    }

    /// Whether `player` may still touch twice in a row this possession.
    pub fn can_dribble(&self, player: usize) -> bool {
        self.players[player].kit.has(Passive::Dribble) && !self.touches.dribbled
    }

    /// How many times `team` has touched the ball since it came to their side.
    pub fn team_touches(&self, team: usize) -> u32 {
        if self.touches.team == team { self.touches.count } else { 0 }
    }

    /// Who last touched the ball for `team`, if they have it.
    pub fn last_toucher(&self, team: usize) -> Option<usize> {
        if self.touches.team == team { self.touches.last } else { None }
    }

    /// Index of `team`'s player in `slot`.
    pub fn player_index(&self, team: usize, slot: usize) -> usize {
        team * self.config.players_per_team + slot
    }

    /// Where a player lines up at the start of a rally. With three or more, the
    /// first plays up front and the rest spread across the back; with fewer,
    /// they spread across the middle.
    pub fn home_position(&self, player: usize) -> Vec3 {
        let per_team = self.config.players_per_team;
        let slot = player % per_team;
        let side = self.side(self.players[player].team);
        let spread = |i: usize, n: usize| if n <= 1 { 0.0 } else { HALF_WIDTH * (i as f32 / (n - 1) as f32 - 0.5) };
        let (depth, z) = match per_team {
            1 | 2 => (0.5, spread(slot, per_team)),
            _ if slot == 0 => (0.3, 0.0),
            _ => (0.6, spread(slot - 1, per_team - 1)),
        };
        Vec3::new(side * HALF_LENGTH * depth, 0.0, z)
    }

    fn start_rally(&mut self) {
        self.rally += 1;
        self.rally_start_tick = self.tick;
        self.phase = Phase::Rally;
        self.touches = Touches::new(self.serving_team);
        self.touch_lockout_until = 0;
        self.last_hit = None;
        self.last_hitter = None;
        self.last_power = false;
        self.decoy = None;
        self.serve_at = None;
        self.freeze_pending = false;
        self.frozen_left = 0;
        self.ice_ball = false;
        self.ice = None;
        if self.switch_pending {
            self.switch_pending = false;
            self.sides = [self.sides[1], self.sides[0]];
        }

        for i in 0..self.players.len() {
            self.players[i].side = self.side(self.players[i].team);
            let home = self.home_position(i);
            self.players[i].reset(home);
        }

        let server = self.player_index(self.serving_team, self.serve_rotation[self.serving_team]);
        self.players[server].position = Vec3::new(self.side(self.serving_team) * (HALF_LENGTH - 1.5), 0.0, 0.0);
        self.ball = Ball::Held { by: server };
    }

    /// What `player` would do if they hit the ball right now. For showing an
    /// aim marker; changes nothing.
    pub fn preview_hit(&self, player: usize) -> HitPreview {
        let p = &self.players[player];
        let fallback = if p.grounded() { MoveId::Pass } else { MoveId::Spike };
        let id = p.active_move(self.tick).unwrap_or(fallback);
        let touches = self.team_touches(p.team) + 1;
        self.plan_hit(player, id, touches, self.ball_position()).preview
    }

    /// The hit `hitter` makes with move `id` from `from`, given the team's touch
    /// count including this one.
    fn plan_hit(&self, hitter: usize, id: MoveId, touches: u32, from: Vec3) -> Plan {
        let player = &self.players[hitter];
        let side = player.side;
        let spec = id.spec();
        // Aimed past a side wall, a shot over the net banks off it (see `bank_off_wall`).
        let mut bank = false;
        let mut over = |kind: HitKind, depth: f32| {
            let (spot, banked) = bank_off_wall(aim_spot(player.aim, kind, from, side, true));
            bank = banked;
            over_net_target(side, spot, depth)
        };
        let (kind, target, quality) = if self.ball == (Ball::Held { by: hitter }) {
            (HitKind::Serve, over(HitKind::Serve, OVER_DEPTH), 1.0)
        } else if let Touch::Keep(kind) = spec.touch {
            if touches >= MAX_TOUCHES {
                // The team's last touch has to go over.
                let kind = if kind == HitKind::Pass { HitKind::Lob } else { kind };
                (kind, over(kind, OVER_DEPTH), 1.0)
            } else {
                let depth = if touches == 1 { RECEIVE_DEPTH } else { SET_DEPTH };
                (kind, own_side_target(side, aim_spot(player.aim, kind, from, side, false), depth), 1.0)
            }
        } else if let Touch::Carry { .. } = spec.touch {
            // Released from a carry: a clean spike from wherever the ball was taken.
            (HitKind::Spike, over(HitKind::Spike, SPIKE_DEPTH), 1.0)
        } else if spec.touch == Touch::Dunk {
            (HitKind::Dunk, over(HitKind::Dunk, SPIKE_DEPTH), 1.0)
        } else if spec.touch == Touch::Curve {
            (HitKind::Curve, over(HitKind::Curve, SPIKE_DEPTH), 1.0)
        } else if spec.touch == Touch::Split {
            (HitKind::Bicycle, over(HitKind::Bicycle, SPIKE_DEPTH), 1.0)
        } else if spec.touch == Touch::Ice {
            // Out of the spin at the top of the leap: a clean spike.
            (HitKind::Spike, over(HitKind::Spike, SPIKE_DEPTH), 1.0)
        } else {
            let (kind, quality) = attack::best_technique(player, from);
            (kind, over(kind, SPIKE_DEPTH), quality)
        };
        let distance = from.with_y(0.0).distance(target.with_y(0.0));
        // Great feet kick as hard and true as a hand spikes.
        let feet = player.kit.has(Passive::Feet);
        let flies_as = if feet && matches!(kind, HitKind::Volley | HitKind::Bicycle) { HitKind::Spike } else { kind };
        let (seconds, spread) = if kind.is_attack() {
            (attack::flight_seconds(flies_as, distance, quality), attack::wobble(flies_as, quality))
        } else if feet && kind == HitKind::Kick {
            (flight_seconds(kind, distance) + spec.hang, 0.0)
        } else {
            (flight_seconds(kind, distance) + spec.hang, spec.wobble)
        };
        let seconds = seconds * (1.0 - BOOST_SPEED * boost(player.aim));
        // A banana kick heads out wide and bends back onto its spot.
        let curve = if spec.touch == Touch::Curve {
            if target.z >= from.z { -BANANA_CURVE } else { BANANA_CURVE }
        } else {
            0.0
        };
        let decoy = (spec.touch == Touch::Split).then(|| {
            // Mirrored across the court, or pushed away from the middle.
            let z = if target.z.abs() > DECOY_SPREAD / 2.0 {
                -target.z
            } else if target.z >= 0.0 {
                target.z - DECOY_SPREAD
            } else {
                target.z + DECOY_SPREAD
            };
            target.with_z(z.clamp(-(HALF_WIDTH - WALL_MARGIN), HALF_WIDTH - WALL_MARGIN))
        });
        let power = matches!(spec.touch, Touch::Carry { .. } | Touch::Dunk | Touch::Curve | Touch::Split | Touch::Freeze | Touch::Ice);
        // A curve already bends; it doesn't bank as well.
        let bank = bank && curve == 0.0 && decoy.is_none();
        let seconds = if bank { seconds * bank_stretch(from, target) } else { seconds };
        let (freeze, ice) = (spec.touch == Touch::Freeze, spec.touch == Touch::Ice);
        let seconds = if ice { seconds * (1.0 - ICE_SPEED) } else { seconds };
        Plan { preview: HitPreview { kind, target, spread, quality }, seconds, curve, decoy, power, bank, freeze, ice }
    }

    /// Launches a planned hit, landing somewhere within its spread.
    fn hit(&mut self, hitter: usize, plan: Plan, from: Vec3, events: &mut Vec<Event>) {
        let HitPreview { kind, target, spread, quality } = plan.preview;
        let target = target + wobble(spread, self.tick, hitter);
        let target = if plan.bank { mirrored_beyond_wall(target) } else { target };
        self.last_hit = Some(kind);
        self.last_hitter = Some(hitter);
        self.last_power = plan.power;
        let seconds = clearing_seconds(from, target, plan.seconds, quality);
        self.ball = Ball::InFlight(Flight::curving_to_target(from, target, seconds, plan.curve, self.tick));
        self.decoy = plan.decoy.map(|spot| Flight::to_target(from, spot, plan.seconds, self.tick));
        self.freeze_pending = plan.freeze;
        self.frozen_left = 0;
        self.ice_ball = plan.ice;
        self.touch_lockout_until = self.tick + TOUCH_LOCKOUT_TICKS;
        events.push(Event::Touched { player: hitter, kind, quality });
        if self.decoy.is_some() {
            events.push(Event::Split { player: hitter });
        }
    }

    fn update_serve(&mut self, server: usize, events: &mut Vec<Event>) {
        // The press starts the swing; the ball goes when the swing reaches it.
        let Some((at, id)) = self.serve_at else {
            if let Some(id) = self.players[server].spend(self.tick) {
                self.serve_at = Some((self.tick + SERVE_SWING_TICKS, id));
            }
            return;
        };
        if self.tick < at {
            return;
        }
        self.serve_at = None;
        let from = held_ball_position(&self.players[server]);
        let plan = self.plan_hit(server, id, 1, from);
        self.touches = Touches { count: 1, last: Some(server), ..Touches::new(self.players[server].team) };
        self.hit(server, plan, from, events);
    }

    fn update_flight(&mut self, flight: Flight, events: &mut Vec<Event>) {
        let before = flight.elapsed(self.tick - 1);
        let now = flight.elapsed(self.tick);
        let landing = flight.landing_time();

        let (x_before, z_before) = flight.axis_bounces(before);
        let (x_now, z_now) = flight.axis_bounces(now);
        if (x_now > x_before || z_now > z_before) && now < landing {
            let at = flight.position_at_time(now);
            if self.config.breakable_walls {
                let panel = court::wall_panel(at, x_now > x_before);
                match self.walls[panel] {
                    0 => {
                        self.walls[panel] = 1;
                        events.push(Event::WallCracked { panel, at });
                    }
                    state => {
                        // Through the glass (or the gap it left): out.
                        if state == 1 {
                            self.walls[panel] = 2;
                            events.push(Event::WallBroken { panel, at });
                        }
                        events.push(Event::Out { at });
                        let team = match self.last_hitter {
                            Some(hitter) => 1 - self.players[hitter].team,
                            None => 1 - self.team_on(at.x),
                        };
                        self.award_point(team, PointReason::Out, at, events);
                        return;
                    }
                }
            }
            events.push(Event::WallBounce { at });
        }
        self.update_decoy(events);
        if let Some(crossing) = flight.next_net_crossing(before)
            && crossing <= now
            && crossing < landing
        {
            let at = flight.position_at_time(crossing);
            let v = flight.velocity_at_time(crossing);
            // Just clear of the net on the side the ball came from.
            let back = -v.x.signum() * (BALL_RADIUS + 0.01);
            // Only the first crossing freezes, and not a ball blocked there.
            let freeze = std::mem::take(&mut self.freeze_pending);
            if self.last_hit == Some(HitKind::Dunk) || self.ice_ball {
                // Straight through the block, flattening whoever tried.
                for blocker in self.blockers_in_the_way(at, v) {
                    let player = &mut self.players[blocker];
                    player.stunned_until = self.tick + KNOCKDOWN_TICKS;
                    player.hands_up = false;
                    player.action = None;
                    events.push(Event::Posterized { player: blocker });
                }
            } else if let Some((blocker, stuffed)) = self.blocker_for(at, v) {
                let (x, velocity) = if stuffed {
                    (back, Vec3::new(-v.x * 0.45, v.y.min(0.0) - 2.0, v.z * 0.5))
                } else {
                    (-back, Vec3::new(v.x * 0.2, 4.0, v.z * 0.4))
                };
                self.ball = Ball::InFlight(Flight { origin: Vec3::new(x, at.y, at.z), velocity, start_tick: self.tick, curve: 0.0 });
                // A block isn't one of the team's three touches, and the blocker may play the ball again.
                self.touches = Touches::new(self.players[blocker].team);
                self.touch_lockout_until = self.tick + TOUCH_LOCKOUT_TICKS;
                events.push(Event::Blocked { player: blocker, stuffed });
                return;
            }
            if at.y < NET_HEIGHT + BALL_RADIUS && at.z.abs() <= NET_HALF_WIDTH {
                // Drops back down on the side it came from.
                let origin = Vec3::new(back, at.y, at.z);
                let velocity = Vec3::new(-v.x * 0.2, v.y.min(0.0) * 0.5, v.z * 0.5);
                self.ball = Ball::InFlight(Flight { origin, velocity, start_tick: self.tick, curve: 0.0 });
                events.push(Event::HitNet { at });
                return;
            }
            if freeze {
                self.frozen_left = FREEZE_TICKS;
                events.push(Event::BallFrozen { at: flight.position_at_time(now) });
                return;
            }
        }

        if now >= landing {
            // Walls keep everything in: wherever it lands, that side loses the point.
            let at = flight.landing_point();
            events.push(Event::Landed { at, velocity: flight.velocity_at_time(landing) });
            self.ice_burst(at, events);
            self.award_point(1 - self.team_on(at.x), PointReason::LandedIn, at, events);
            return;
        }

        if self.tick < self.touch_lockout_until {
            return;
        }
        let ball = flight.position_at(self.tick);
        let next_ball = flight.position_at(self.tick + 1);
        let rebounded = flight.wall_bounces(now) > 0;
        let hitter = (0..self.players.len())
            .filter(|&i| {
                let player = &self.players[i];
                let reach = if rebounded && player.kit.has(Passive::WallPass) { WALL_PASS_REACH } else { 1.0 };
                player.active_move(self.tick).is_some_and(|id| {
                    player.reaches_scaled(id, ball, reach * player.extension_at(self.tick))
                        && (matches!(id.spec().touch, Touch::Keep(_) | Touch::Curve) || attack::hits_now(player, id, ball, next_ball))
                })
            })
            .min_by(|&a, &b| {
                let da = self.players[a].position.distance_squared(ball);
                let db = self.players[b].position.distance_squared(ball);
                da.total_cmp(&db)
            });
        if let Some(hitter) = hitter {
            self.ice_burst(ball, events);
            self.touch(hitter, ball, events);
        } else if let Some(player) = self.body_in_the_way(ball, flight.velocity_at_time(now)) {
            self.ice_burst(ball, events);
            self.body_hit(player, ball, flight.velocity_at_time(now), events);
        }
    }

    /// A frozen ball: it hangs where it is (its flight starts a tick later
    /// each tick), out of everyone's reach, then goes on exactly as it was.
    fn update_frozen(&mut self, flight: Flight, events: &mut Vec<Event>) {
        self.ball = Ball::InFlight(Flight { start_tick: flight.start_tick + 1, ..flight });
        self.frozen_left -= 1;
        if self.frozen_left == 0 {
            events.push(Event::BallThawed { at: self.ball_position() });
        }
    }

    /// An ice ball meeting a player or the floor at `at` bursts into a ring
    /// of ice there: everyone inside it glides while it lasts (see `step`).
    fn ice_burst(&mut self, at: Vec3, events: &mut Vec<Event>) {
        if !std::mem::take(&mut self.ice_ball) {
            return;
        }
        self.ice = Some((at.with_y(0.0), self.tick + ICE_TICKS));
        events.push(Event::IceBurst { at });
    }

    /// Who a ball at `ball`, moving at `velocity`, hits: a player not stunned
    /// whose body it's inside and coming at.
    fn body_in_the_way(&self, ball: Vec3, velocity: Vec3) -> Option<usize> {
        (0..self.players.len()).find(|&i| {
            let player = &self.players[i];
            let rise = ball.y - player.position.y;
            let away = Vec2::new(ball.x - player.position.x, ball.z - player.position.z);
            !player.stunned(self.tick)
                && (BODY_BOTTOM..=BODY_TOP).contains(&rise)
                && away.length() < BODY_RADIUS + BALL_RADIUS
                && away.dot(Vec2::new(velocity.x, velocity.z)) < 0.0
        })
    }

    /// A ball hitting `player`'s body: it bounces off, scattered, popping up
    /// more the higher it hit, and counts as their team's touch. A fast one
    /// knocks them back, stunned.
    fn body_hit(&mut self, player: usize, ball: Vec3, velocity: Vec3, events: &mut Vec<Event>) {
        let body = self.players[player];
        let rise = ((ball.y - body.position.y - BODY_BOTTOM) / (BODY_TOP - BODY_BOTTOM)).clamp(0.0, 1.0);
        let center = Vec3::new(body.position.x, ball.y, body.position.z);
        let normal = (ball - center).with_y(0.0).try_normalize().unwrap_or(-velocity.with_y(0.0).normalize_or(Vec3::X));
        let speed = velocity.length();
        let into = velocity.dot(normal).min(0.0);
        let mut out = (velocity - (1.0 + BODY_RESTITUTION) * into * normal) * BODY_RESTITUTION;
        let roll = dice(self.tick, (player as u32).wrapping_add(0xB0D1));
        let swing = ((roll & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * BODY_SCATTER;
        let across = Vec2::from_angle(swing).rotate(Vec2::new(out.x, out.z)).clamp_length_max(BODY_MAX_SPEED);
        out = Vec3::new(across.x, out.y.max(0.0) + BODY_POP_LOW + (BODY_POP_HIGH - BODY_POP_LOW) * rise, across.y);
        let origin = ball + normal * 0.05;
        self.ball = Ball::InFlight(Flight { origin, velocity: out, start_tick: self.tick, curve: 0.0 });
        self.decoy = None;
        self.touch_lockout_until = self.tick + TOUCH_LOCKOUT_TICKS;
        self.last_hitter = Some(player);
        self.last_power = false;

        let strength = ((speed - BODY_KNOCK_SPEED) / BODY_KNOCK_SPEED).clamp(0.0, 1.0);
        events.push(Event::BodyHit { player, at: ball, strength });
        if speed >= BODY_KNOCK_SPEED {
            let p = &mut self.players[player];
            let seconds = (speed * BODY_STUN_PER_SPEED).min(BODY_MAX_STUN);
            p.stunned_until = self.tick + (seconds * TICK_HZ as f32) as u32;
            p.action = None;
            p.hands_up = false;
            let back = Vec2::new(-normal.x, -normal.z) * (speed * BODY_SHOVE).min(BODY_MAX_SHOVE);
            p.velocity = back;
        }

        // A touch by their team, like any other.
        let team = body.team;
        if self.touches.team != team {
            self.touches = Touches::new(team);
        }
        self.touches.count += 1;
        self.touches.last = Some(player);
        self.touches.clean = 0;
        if self.touches.count > MAX_TOUCHES {
            self.award_point(1 - team, PointReason::TooManyTouches, ball, events);
        }
    }

    /// A decoy lasts until it lands, or until someone on the defending side
    /// goes for it: then it vanishes, and their move is spent on nothing.
    fn update_decoy(&mut self, events: &mut Vec<Event>) {
        let Some(decoy) = self.decoy else { return };
        if decoy.elapsed(self.tick) >= decoy.landing_time() {
            self.decoy = None;
            return;
        }
        let at = decoy.position_at(self.tick);
        let attackers = self.last_hitter.map(|hitter| self.players[hitter].team);
        let fooled = (0..self.players.len()).find(|&i| {
            let player = &self.players[i];
            Some(player.team) != attackers && player.active_move(self.tick).is_some_and(|id| player.reaches(id, at))
        });
        if let Some(player) = fooled {
            self.players[player].spend(self.tick);
            self.decoy = None;
            events.push(Event::DecoyPopped { player });
        }
    }

    /// When and where `player`'s armed move would meet the ball, looking up to
    /// `horizon` ticks ahead: the ticks until contact and the ball's position
    /// then. Follows the same rules as a real touch, with the player carried
    /// along by their current velocity (and gravity, in the air). For timing
    /// animations; changes nothing.
    pub fn predicted_contact(&self, player: usize, horizon: u32) -> Option<(u32, Vec3)> {
        // A serve being swung meets the ball where it's held.
        if let (Ball::Held { by }, Some((at, _))) = (self.ball, self.serve_at)
            && by == player
        {
            let ahead = at.saturating_sub(self.tick);
            return (ahead <= horizon).then(|| (ahead, held_ball_position(&self.players[player])));
        }
        let Ball::InFlight(flight) = self.ball else { return None };
        let mut body = self.players[player];
        let id = body.active_move(self.tick)?;
        // As the touch judges it: a banana kick is met wherever it reaches.
        let attack = !matches!(id.spec().touch, Touch::Keep(_) | Touch::Curve);
        let landing = flight.landing_time();
        for ahead in 1..=horizon {
            let tick = self.tick + ahead;
            if flight.elapsed(tick) >= landing {
                return None;
            }
            if !body.grounded() || body.vertical_velocity > 0.0 {
                body.vertical_velocity -= player::PLAYER_GRAVITY * DT;
                body.position.y = (body.position.y + body.vertical_velocity * DT).max(0.0);
            }
            body.position += Vec3::new(body.velocity.x, 0.0, body.velocity.y) * DT;
            if tick < self.touch_lockout_until {
                continue;
            }
            let ball = flight.position_at(tick);
            if body.reaches_scaled(id, ball, body.extension_at(tick)) && (!attack || attack::hits_now(&body, id, ball, flight.position_at(tick + 1))) {
                return Some((ahead, ball));
            }
        }
        None
    }

    fn touch(&mut self, hitter: usize, ball: Vec3, events: &mut Vec<Event>) {
        // When the button was pressed, and how the ball came in, for judging
        // a reception (read before the touch uses the move up).
        let pressed = self.players[hitter].action.map_or(self.tick, |action| action.pressed_tick);
        let incoming = match self.ball {
            Ball::InFlight(flight) => flight.velocity_at_time(flight.elapsed(self.tick)),
            _ => Vec3::ZERO,
        };
        let Some(id) = self.players[hitter].spend(self.tick) else {
            return;
        };
        self.decoy = None;
        let team = self.players[hitter].team;

        if self.touches.team != team {
            self.touches = Touches::new(team);
        }
        if self.must_not_touch(hitter) {
            self.award_point(1 - team, PointReason::DoubleTouch, ball, events);
            return;
        }
        if self.config.players_per_team > 1 && self.touches.last == Some(hitter) {
            self.touches.dribbled = true;
            events.push(Event::Dribbled { player: hitter });
        }
        self.touches.count += 1;
        self.touches.last = Some(hitter);
        if self.touches.count > MAX_TOUCHES {
            self.award_point(1 - team, PointReason::TooManyTouches, ball, events);
            return;
        }
        let player = &mut self.players[hitter];
        if player.kit.moves.iter().any(|id| id.spec().ultimate) {
            player.charge = (player.charge + CHARGE_PER_TOUCH).min(1.0);
        }

        if let Touch::Carry { ticks, shift } = id.spec().touch {
            // Toward where the carrier is moving across the court, else toward the middle.
            let sideways = if player.movement.y.abs() > 0.2 { player.movement.y.signum() } else { -player.position.z.signum() };
            let velocity = Vec2::new(0.0, sideways * shift / (ticks as f32 * DT));
            player.carry = Some(Carry { until_tick: self.tick + ticks, velocity });
            self.ball = Ball::Carried { by: hitter, release_tick: self.tick + ticks };
            events.push(Event::Carried { player: hitter });
            return;
        }
        let mut plan = self.plan_hit(hitter, id, self.touches.count, ball);
        let position = self.players[hitter].position;
        if let Touch::Keep(kind) = id.spec().touch {
            // Receptions go where the ball and the timing let them (see `reception`).
            let aim = plan.preview.target;
            let out = Vec2::new(aim.x - ball.x, aim.z - ball.z);
            let judged = reception::judge(&self.players[hitter], id, self.tick.saturating_sub(pressed), ball, incoming, out, self.last_power);
            if judged.difficulty >= SHOVE_DIFFICULTY && judged.error() >= SHOVE_ERROR {
                // Too much ball, too badly met: it pushes the receiver back.
                let strength = ((judged.error() - SHOVE_ERROR) / (1.0 - SHOVE_ERROR)).clamp(0.0, 1.0);
                let lerp = |(a, b): (f32, f32)| a + (b - a) * strength;
                let push = Vec2::new(incoming.x, incoming.z).normalize_or_zero() * lerp(SHOVE_SPEED);
                let receiver = &mut self.players[hitter];
                receiver.stunned_until = self.tick + (lerp(SHOVE_STUN) * TICK_HZ as f32) as u32;
                receiver.velocity = push;
                events.push(Event::Shoved { player: hitter, strength });
            }
            plan.preview.spread += judged.spread();
            plan.preview.quality = (1.0 - judged.error()).clamp(0.0, 1.0);
            let shank = judged.shank();
            if shank > 0.0 {
                let wild = reception::shank_target(ball, incoming, out, self.tick, hitter);
                let target = aim.lerp(wild, shank);
                plan.preview.target = target;
                plan.seconds = (flight_seconds(kind, ball.with_y(0.0).distance(target.with_y(0.0))) + id.spec().hang)
                    * (1.0 - reception::SHANK_SPEED * shank);
            }
        }
        if let Touch::Keep(HitKind::Pass) = id.spec().touch {
            let reach = Vec2::new(ball.x - position.x, ball.z - position.z).length();
            let clean = if self.touches.count <= 1 { CLEAN_RECEPTION } else { CLEAN_SET };
            self.touches.clean = if reach <= clean { self.touches.clean + 1 } else { 0 };
        }
        let wall_jump = self.players[hitter].wall_jump_tick;
        if plan.preview.kind.is_attack() && wall_jump > 0 && (self.tick - wall_jump) as f32 * DT <= WALL_SHOT_SECONDS {
            plan.preview.quality = (plan.preview.quality + WALL_SHOT_QUALITY).min(1.0);
            plan.seconds *= 1.0 - WALL_SHOT_SPEED;
            plan.power = true;
            events.push(Event::WallShot { player: hitter });
        }
        let passing = matches!(id.spec().touch, Touch::Keep(HitKind::Pass));
        if !passing && plan.preview.kind.is_attack() && self.touches.clean >= CHAINED_TOUCHES {
            plan.preview.quality = (plan.preview.quality + CHAIN_QUALITY).min(1.0);
            plan.seconds *= 1.0 - CHAIN_SPEED;
            events.push(Event::Chained { player: hitter });
        }
        self.hit(hitter, plan, ball, events);
    }

    /// Releases a carried ball as an attack when the carry ends, or if the
    /// carrier lands first.
    fn update_carry(&mut self, carrier: usize, release_tick: u32, events: &mut Vec<Event>) {
        let player = &self.players[carrier];
        if self.tick < release_tick && !player.grounded() {
            return;
        }
        let from = carried_ball_position(player);
        let plan = self.plan_hit(carrier, MoveId::Crossover, self.touches.count, from);
        self.hit(carrier, plan, from, events);
    }

    /// Who blocks a ball crossing the net at `at` moving at `velocity`, and
    /// whether squarely. Serves can't be blocked, and balls going into the net
    /// aren't blocks.
    fn blocker_for(&self, at: Vec3, velocity: Vec3) -> Option<(usize, bool)> {
        if self.last_hit == Some(HitKind::Serve) || at.y < NET_HEIGHT {
            return None;
        }
        // A square block beats a glancing one.
        self.block_contacts(at, velocity).max_by_key(|&(_, stuffed)| stuffed)
    }

    /// Everyone whose block a ball crossing the net at `at` goes into.
    fn blockers_in_the_way(&self, at: Vec3, velocity: Vec3) -> Vec<usize> {
        self.block_contacts(at, velocity).map(|(player, _)| player).collect()
    }

    fn block_contacts(&self, at: Vec3, velocity: Vec3) -> impl Iterator<Item = (usize, bool)> + '_ {
        // The team on the side the ball is heading into.
        let defending = self.team_on(velocity.x);
        (0..self.players.len())
            .filter(move |&i| self.players[i].team == defending)
            .filter_map(move |i| self.players[i].block_contact(at).map(|stuffed| (i, stuffed)))
    }

    fn award_point(&mut self, team: usize, reason: PointReason, ball_at: Vec3, events: &mut Vec<Event>) {
        self.score[team] += 1;
        for player in self.players.iter_mut().filter(|p| p.team == team && p.kit.moves.iter().any(|id| id.spec().ultimate)) {
            player.charge = (player.charge + CHARGE_PER_POINT).min(1.0);
        }
        // Winning back the serve rotates who serves.
        if team != self.serving_team {
            self.serving_team = team;
            self.serve_rotation[team] = (self.serve_rotation[team] + 1) % self.config.players_per_team;
        }
        self.ball = Ball::Dead { at: ball_at.with_y(BALL_RADIUS) };
        events.push(Event::Point { team, reason });

        let other = 1 - team;
        let mut pause = POINT_PAUSE_TICKS;
        if self.score[team] >= self.points_to_win_set() && self.score[team] >= self.score[other] + 2 {
            self.sets[team] += 1;
            events.push(Event::SetWon { team });
            if self.sets[team] == self.config.sets_to_win {
                self.phase = Phase::MatchOver { winner: team };
                events.push(Event::MatchWon { team });
                return;
            }
            self.set += 1;
            self.score = [0, 0];
            self.walls = [0; PANELS];
            // The team that lost the set serves first in the next.
            self.serving_team = other;
            pause = SET_PAUSE_TICKS;
        } else {
            let every = if self.deciding_set() { self.config.deciding_switch_every } else { self.config.switch_every };
            if (self.score[0] + self.score[1]) % every == 0 {
                self.switch_pending = true;
                events.push(Event::SidesSwitched);
            }
        }
        self.phase = Phase::PointScored { resume_tick: self.tick + pause };
    }
}

/// Where a server holds the ball: tossed up over the hitting (right)
/// shoulder for a hand serve, or out in front at the waist to drop-kick it,
/// for a hero who only plays with their feet. Players face the net, with
/// their right toward -z on the +x half.
fn held_ball_position(player: &Player) -> Vec3 {
    let offset = if player.kit.has(Passive::Feet) { Vec3::new(-player.side * 0.35, 1.1, 0.0) } else { Vec3::new(0.0, 1.9, -player.side * 0.34) };
    player.position + offset
}

/// Palmed high overhead, just in front: where a carried ball rides.
fn carried_ball_position(player: &Player) -> Vec3 {
    player.position + Vec3::new(-player.side * 0.3, 2.0, 0.0)
}

/// A spot past a side wall (`|z|` beyond it) asks for a bank shot off that
/// wall: it lands at the spot's mirror image back inside, by way of the wall.
/// Returns the landing spot and whether it banks.
pub(crate) fn bank_off_wall(spot: Option<Vec2>) -> (Option<Vec2>, bool) {
    match spot {
        Some(spot) if spot.y.abs() > HALF_WIDTH => {
            let inside = (2.0 * HALF_WIDTH - spot.y.abs()).max(WALL_MARGIN - HALF_WIDTH);
            (Some(Vec2::new(spot.x, spot.y.signum() * inside)), true)
        }
        other => (other, false),
    }
}

/// Where a ball aimed at `target` by way of the nearer side wall really heads:
/// the target mirrored beyond that wall (where the ball's center meets it).
pub(crate) fn mirrored_beyond_wall(target: Vec3) -> Vec3 {
    let wall = target.z.signum() * (HALF_WIDTH - BALL_RADIUS);
    target.with_z(2.0 * wall - target.z)
}

/// How much longer a banked shot takes than a straight one: by how much
/// farther its path is.
fn bank_stretch(from: Vec3, target: Vec3) -> f32 {
    let straight = from.with_y(0.0).distance(target.with_y(0.0)).max(1.0);
    let banked = from.with_y(0.0).distance(mirrored_beyond_wall(target).with_y(0.0));
    (banked / straight).sqrt().max(1.0)
}

/// Where a shot over the net from the half at `side` lands: the aimed spot, kept
/// on the other half and off the walls, or `depth` past the net if unaimed.
pub(crate) fn over_net_target(side: f32, spot: Option<Vec2>, depth: f32) -> Vec3 {
    let spot = spot.unwrap_or(Vec2::new(-side * depth, 0.0));
    let (min_x, max_x) = court::x_range(-side, 1.0, HALF_LENGTH - WALL_MARGIN);
    let max_z = HALF_WIDTH - WALL_MARGIN;
    Vec3::new(spot.x.clamp(min_x, max_x), BALL_RADIUS, spot.y.clamp(-max_z, max_z))
}

/// Where a pass on the half at `side` lands: the aimed spot, kept inside that
/// half, or `depth` from the net if unaimed.
fn own_side_target(side: f32, spot: Option<Vec2>, depth: f32) -> Vec3 {
    let spot = spot.unwrap_or(Vec2::new(side * depth, 0.0));
    let (min_x, max_x) = court::x_range(side, 0.8, HALF_LENGTH - WALL_MARGIN);
    let max_z = HALF_WIDTH - WALL_MARGIN;
    Vec3::new(spot.x.clamp(min_x, max_x), BALL_RADIUS, spot.y.clamp(-max_z, max_z))
}

/// A ball hit from below the top of the net has to go up and over it. The
/// flight lengthens (a higher, slower arc) until it clears: by
/// `NET_CLEARANCE` for a clean contact, down to `NET_RISK` short of clearing
/// for the worst, so a sloppy touch at the net can still find it. Hit from
/// right at the net, under its top, there's no arc that clears: at most
/// `MAX_LOFT_SECONDS`. `seconds` stands if it already clears.
fn clearing_seconds(from: Vec3, target: Vec3, seconds: f32, quality: f32) -> f32 {
    let span = (from.x - target.x).abs();
    if from.x * target.x >= 0.0 || span < 1e-3 {
        return seconds;
    }
    // How far along the path the net is; the ball flies at an even pace in x.
    let share = from.x.abs() / span;
    let need = NET_HEIGHT + BALL_RADIUS - NET_RISK + (NET_CLEARANCE + NET_RISK) * quality.clamp(0.0, 1.0);
    // The straight line from contact to landing, and how far a flight of t
    // seconds arcs above it there: g t^2 share (1 - share) / 2.
    let line = from.y + share * (target.y - from.y);
    let lift = need - line;
    if lift <= 0.0 {
        return seconds;
    }
    let sag = court::BALL_GRAVITY * share * (1.0 - share) / 2.0;
    let least = (lift / sag.max(1e-4)).sqrt();
    seconds.max(least.min(MAX_LOFT_SECONDS))
}

/// An offset of up to `radius` meters for inaccurate touches, varying from hit
/// to hit.
fn wobble(radius: f32, tick: u32, hitter: usize) -> Vec3 {
    if radius <= 0.0 {
        return Vec3::ZERO;
    }
    let roll = dice(tick, hitter as u32);
    let angle = (roll & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
    let distance = radius * (roll >> 16) as f32 / 65535.0;
    Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance)
}

/// A number that varies from call to call with its inputs, standing in for
/// randomness while keeping the simulation deterministic.
pub(crate) fn dice(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 13)
}

#[cfg(test)]
mod tests;
