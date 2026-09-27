//! The match simulation: every rule that decides what happens in a match.
//!
//! Pure Rust with no engine, rendering or networking, so the server, the client
//! and the tests all run exactly the same rules. Advance it with [`Sim::step`]
//! at [`TICK_HZ`]; the same state and inputs always produce the same result.

pub mod attack;
mod ball;
pub mod bot;
pub mod court;
pub mod moves;
mod player;

pub use ball::{Ball, Flight};
pub use glam::{Vec2, Vec3};
pub use moves::{Kit, Move, MoveId, Passive};
pub use player::{Action, Carry, Dash, MovePhase, Player};

use moves::Touch;
use court::{BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HEIGHT, NET_HALF_WIDTH};

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

pub(crate) const MAX_TOUCHES: u32 = 3;
/// After a touch nobody can touch the ball for this long, so one swing never counts twice.
const TOUCH_LOCKOUT_TICKS: u32 = 10;
const POINT_PAUSE_TICKS: u32 = 90;
/// How long a player knocked down by a dunk stays down.
const KNOCKDOWN_TICKS: u32 = 60;
/// Ultimate charge from each touch, and for each point the team wins.
const CHARGE_PER_TOUCH: f32 = 0.05;
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
        HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk => attack::flight_seconds(kind, distance, 1.0),
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
        HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk => 18.0,
    }
}

/// A fully boosted hit goes this much farther, and gets there this much sooner.
/// Small on purpose, Mario Tennis style: a tap is a good hit, holding a moment
/// makes it a little better, and the game stays quick.
const BOOST_RANGE: f32 = 0.2;
const BOOST_SPEED: f32 = 0.15;

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
}

impl HitKind {
    /// Attacks: hit hard over the net from the air.
    pub fn is_attack(self) -> bool {
        matches!(self, HitKind::Spike | HitKind::Volley | HitKind::Bicycle | HitKind::Dunk)
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointReason {
    LandedIn,
    TooManyTouches,
    DoubleTouch,
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
    /// Knocked down by a dunk through their block.
    Posterized { player: usize },
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
}

impl Touches {
    fn new(team: usize) -> Self {
        Self { team, count: 0, last: None, dribbled: false }
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
    /// Each team's half: -1 or +1.
    sides: [f32; 2],
    /// Switch sides when the next rally starts.
    switch_pending: bool,
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
            sides,
            switch_pending: false,
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
        }

        match self.ball {
            Ball::Held { by } => self.update_serve(by, &mut events),
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
        let over = |kind: HitKind, depth: f32| over_net_target(side, aim_spot(player.aim, kind, from, side, true), depth);
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
        } else {
            let (kind, quality) = attack::best_technique(player, from);
            (kind, over(kind, SPIKE_DEPTH), quality)
        };
        let distance = from.with_y(0.0).distance(target.with_y(0.0));
        let (seconds, spread) = if kind.is_attack() {
            (attack::flight_seconds(kind, distance, quality), attack::wobble(kind, quality))
        } else {
            (flight_seconds(kind, distance) + spec.hang, spec.wobble)
        };
        let seconds = seconds * (1.0 - BOOST_SPEED * boost(player.aim));
        Plan { preview: HitPreview { kind, target, spread, quality }, seconds }
    }

    /// Launches a planned hit, landing somewhere within its spread.
    fn hit(&mut self, hitter: usize, plan: Plan, from: Vec3, events: &mut Vec<Event>) {
        let HitPreview { kind, target, spread, quality } = plan.preview;
        let target = target + wobble(spread, self.tick, hitter);
        self.last_hit = Some(kind);
        self.last_hitter = Some(hitter);
        self.ball = Ball::InFlight(Flight::to_target(from, target, plan.seconds, self.tick));
        self.touch_lockout_until = self.tick + TOUCH_LOCKOUT_TICKS;
        events.push(Event::Touched { player: hitter, kind, quality });
    }

    fn update_serve(&mut self, server: usize, events: &mut Vec<Event>) {
        let Some(id) = self.players[server].spend(self.tick) else {
            return;
        };
        let from = held_ball_position(&self.players[server]);
        let plan = self.plan_hit(server, id, 1, from);
        self.touches = Touches { count: 1, last: Some(server), ..Touches::new(self.players[server].team) };
        self.hit(server, plan, from, events);
    }

    fn update_flight(&mut self, flight: Flight, events: &mut Vec<Event>) {
        let before = flight.elapsed(self.tick - 1);
        let now = flight.elapsed(self.tick);
        let landing = flight.landing_time();

        if flight.wall_bounces(now) > flight.wall_bounces(before) && now < landing {
            events.push(Event::WallBounce { at: flight.position_at_time(now) });
        }
        if let Some(crossing) = flight.next_net_crossing(before)
            && crossing <= now
            && crossing < landing
        {
            let at = flight.position_at_time(crossing);
            let v = flight.velocity_at_time(crossing);
            // Just clear of the net on the side the ball came from.
            let back = -v.x.signum() * (BALL_RADIUS + 0.01);
            if self.last_hit == Some(HitKind::Dunk) {
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
                self.ball = Ball::InFlight(Flight { origin: Vec3::new(x, at.y, at.z), velocity, start_tick: self.tick });
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
                self.ball = Ball::InFlight(Flight { origin, velocity, start_tick: self.tick });
                events.push(Event::HitNet { at });
                return;
            }
        }

        if now >= landing {
            // Walls keep everything in: wherever it lands, that side loses the point.
            let at = flight.landing_point();
            events.push(Event::Landed { at, velocity: flight.velocity_at_time(landing) });
            self.award_point(1 - self.team_on(at.x), PointReason::LandedIn, at, events);
            return;
        }

        if self.tick < self.touch_lockout_until {
            return;
        }
        let ball = flight.position_at(self.tick);
        let next_ball = flight.position_at(self.tick + 1);
        let hitter = (0..self.players.len())
            .filter(|&i| {
                let player = &self.players[i];
                player.active_move(self.tick).is_some_and(|id| {
                    player.reaches(id, ball)
                        && (matches!(id.spec().touch, Touch::Keep(_)) || attack::hits_now(player, id, ball, next_ball))
                })
            })
            .min_by(|&a, &b| {
                let da = self.players[a].position.distance_squared(ball);
                let db = self.players[b].position.distance_squared(ball);
                da.total_cmp(&db)
            });
        if let Some(hitter) = hitter {
            self.touch(hitter, ball, events);
        }
    }

    fn touch(&mut self, hitter: usize, ball: Vec3, events: &mut Vec<Event>) {
        let Some(id) = self.players[hitter].spend(self.tick) else {
            return;
        };
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
        let plan = self.plan_hit(hitter, id, self.touches.count, ball);
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

fn held_ball_position(player: &Player) -> Vec3 {
    player.position + Vec3::new(-player.side * 0.35, 1.9, 0.0)
}

/// Palmed high overhead, just in front: where a carried ball rides.
fn carried_ball_position(player: &Player) -> Vec3 {
    player.position + Vec3::new(-player.side * 0.3, 2.0, 0.0)
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
