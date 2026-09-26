//! The match simulation: every rule that decides what happens in a match.
//!
//! Pure Rust with no engine, rendering or networking, so the server, the client
//! and the tests all run exactly the same rules. Advance it with [`Sim::step`]
//! at [`TICK_HZ`]; the same state and inputs always produce the same result.

mod ball;
pub mod bot;
pub mod court;
pub mod moves;
mod player;

pub use ball::{Ball, Flight};
pub use glam::{Vec2, Vec3};
pub use moves::{Kit, Move, MoveId};
pub use player::{Action, MovePhase, Player};

use moves::Touch;
use court::{BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HEIGHT, NET_HALF_WIDTH, RUNOFF};

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

const MAX_TOUCHES: u32 = 3;
/// After a touch nobody can touch the ball for this long, so one swing never counts twice.
const TOUCH_LOCKOUT_TICKS: u32 = 10;
const POINT_PAUSE_TICKS: u32 = 90;
const SET_PAUSE_TICKS: u32 = 240;

/// Default distance past the net for unaimed shots over it. Spikes go deep:
/// their flat path would otherwise catch the net unless hit from right beside it.
pub(crate) const OVER_DEPTH: f32 = HALF_LENGTH * 0.5;
const SPIKE_DEPTH: f32 = HALF_LENGTH * 0.6;
/// Default spot for a team's first touch: mid-court, for a teammate to set.
const RECEIVE_DEPTH: f32 = 3.0;
/// Default spot for a set: close to the net, for a teammate to spike.
pub(crate) const SET_DEPTH: f32 = 1.3;

/// Seconds a hit takes to travel `distance` meters: a base plus a little per
/// meter, so short shots are quick and flat and long ones stay playable.
pub(crate) fn flight_seconds(kind: HitKind, distance: f32) -> f32 {
    match kind {
        HitKind::Serve => 0.9 + 0.035 * distance,
        HitKind::Spike => 0.3 + 0.02 * distance,
        HitKind::Lob => 0.8 + 0.035 * distance,
        // Scrambles like digs and kicks add the move's own hang time on top.
        HitKind::Pass | HitKind::Dig | HitKind::Kick => 1.2 + 0.05 * distance,
    }
}

/// One player's controls for one tick. Buttons mean "pressed this tick", not "held".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// World-space XZ direction, length at most 1.
    pub movement: Vec2,
    /// The spot on the floor (world XZ) to send the next hit to. `None` uses
    /// the default spot for that kind of hit.
    pub aim: Option<Vec2>,
    pub jump: bool,
    pub pass: bool,
    pub spike: bool,
    pub dive: bool,
    pub kick: bool,
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
            players_per_team: 2,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointReason {
    LandedIn,
    LandedOut,
    TooManyTouches,
    DoubleTouch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A move began: a press, a dive, a foot save.
    MoveStarted { player: usize, id: MoveId },
    Touched { player: usize, kind: HitKind },
    /// A block at the net. `stuffed`: sent straight back down on the attackers;
    /// otherwise softened, popping up on the blocker's side.
    Blocked { player: usize, stuffed: bool },
    HitNet { at: Vec3 },
    /// `velocity` is the ball's as it hit the floor.
    Landed { at: Vec3, velocity: Vec3, inside: bool },
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
    /// The kind of the latest hit.
    last_hit: Option<HitKind>,
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
            touches: Touches { team: 0, count: 0, last: None },
            touch_lockout_until: 0,
            last_hit: None,
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
            if let Some(id) = self.players[i].update(input, self.tick, serving, ball) {
                events.push(Event::MoveStarted { player: i, id });
            }
        }

        match self.ball {
            Ball::Held { by } => self.update_serve(by, &mut events),
            Ball::InFlight(flight) => self.update_flight(flight, &mut events),
            Ball::Dead { .. } => {}
        }
        events
    }

    pub fn ball_position(&self) -> Vec3 {
        match self.ball {
            Ball::Held { by } => held_ball_position(&self.players[by]),
            Ball::InFlight(flight) => flight.position_at(self.tick),
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
        self.config.players_per_team > 1 && self.touches.last == Some(player)
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

    /// Where a player lines up at the start of a rally: mid-way back, spread
    /// across the court.
    pub fn home_position(&self, player: usize) -> Vec3 {
        let per_team = self.config.players_per_team;
        let slot = player % per_team;
        let z = if per_team == 1 {
            0.0
        } else {
            HALF_WIDTH * (slot as f32 / (per_team - 1) as f32 - 0.5)
        };
        Vec3::new(self.side(self.players[player].team) * HALF_LENGTH * 0.5, 0.0, z)
    }

    fn start_rally(&mut self) {
        self.rally += 1;
        self.rally_start_tick = self.tick;
        self.phase = Phase::Rally;
        self.touches = Touches { team: self.serving_team, count: 0, last: None };
        self.touch_lockout_until = 0;
        self.last_hit = None;
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
        self.players[server].position = Vec3::new(self.side(self.serving_team) * (HALF_LENGTH + 1.0), 0.0, 0.0);
        self.ball = Ball::Held { by: server };
    }

    /// What `player` would do if they hit the ball right now: the kind of hit and
    /// where it would land. For showing an aim marker; changes nothing.
    pub fn preview_hit(&self, player: usize) -> (HitKind, Vec3) {
        let p = &self.players[player];
        let id = if p.grounded() { MoveId::Pass } else { MoveId::Spike };
        let touches = self.team_touches(p.team) + 1;
        let (kind, flight) = self.plan_hit(player, id, touches, self.ball_position());
        (kind, flight.landing_point())
    }

    /// The hit `hitter` makes with move `id` from `from`, given the team's touch
    /// count including this one.
    fn plan_hit(&self, hitter: usize, id: MoveId, touches: u32, from: Vec3) -> (HitKind, Flight) {
        let player = &self.players[hitter];
        let side = player.side;
        let spec = id.spec();
        let (kind, target) = if self.ball == (Ball::Held { by: hitter }) {
            (HitKind::Serve, over_net_target(side, player.aim, OVER_DEPTH))
        } else if let Touch::Keep(kind) = spec.touch {
            if touches >= MAX_TOUCHES {
                // The team's last touch has to go over.
                let kind = if kind == HitKind::Pass { HitKind::Lob } else { kind };
                (kind, over_net_target(side, player.aim, OVER_DEPTH))
            } else {
                let depth = if touches == 1 { RECEIVE_DEPTH } else { SET_DEPTH };
                (kind, own_side_target(side, player.aim, depth))
            }
        } else {
            (HitKind::Spike, over_net_target(side, player.aim, SPIKE_DEPTH))
        };
        let target = target + wobble(spec.wobble, self.tick, hitter);
        let seconds = flight_seconds(kind, from.with_y(0.0).distance(target.with_y(0.0))) + spec.hang;
        (kind, Flight::to_target(from, target, seconds, self.tick))
    }

    fn update_serve(&mut self, server: usize, events: &mut Vec<Event>) {
        let Some(id) = self.players[server].spend(self.tick) else {
            return;
        };
        let (kind, flight) = self.plan_hit(server, id, 1, held_ball_position(&self.players[server]));
        self.touches = Touches { team: self.players[server].team, count: 1, last: Some(server) };
        self.launch(server, kind, flight, events);
    }

    fn update_flight(&mut self, flight: Flight, events: &mut Vec<Event>) {
        let before = flight.elapsed(self.tick - 1);
        let now = flight.elapsed(self.tick);
        let landing = flight.landing_time();

        if let Some(crossing) = flight.net_crossing_time()
            && crossing > before
            && crossing <= now
            && crossing < landing
        {
            let at = flight.position_at_time(crossing);
            let v = flight.velocity_at_time(crossing);
            // Just clear of the net on the side the ball came from.
            let back = -v.x.signum() * (BALL_RADIUS + 0.01);
            if let Some((blocker, stuffed)) = self.blocker_for(at, v) {
                let (x, velocity) = if stuffed {
                    (back, Vec3::new(-v.x * 0.45, v.y.min(0.0) - 2.0, v.z * 0.5))
                } else {
                    (-back, Vec3::new(v.x * 0.2, 4.0, v.z * 0.4))
                };
                self.ball = Ball::InFlight(Flight { origin: Vec3::new(x, at.y, at.z), velocity, start_tick: self.tick });
                // A block isn't one of the team's three touches, and the blocker may play the ball again.
                self.touches = Touches { team: self.players[blocker].team, count: 0, last: None };
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
            let at = flight.landing_point();
            let inside = court::is_inside(at);
            events.push(Event::Landed { at, velocity: flight.velocity_at_time(landing), inside });
            if inside {
                self.award_point(1 - self.team_on(at.x), PointReason::LandedIn, at, events);
            } else {
                self.award_point(1 - self.touches.team, PointReason::LandedOut, at, events);
            }
            return;
        }

        if self.tick < self.touch_lockout_until {
            return;
        }
        let ball = flight.position_at(self.tick);
        let hitter = (0..self.players.len())
            .filter(|&i| {
                self.players[i].active_move(self.tick).is_some() && self.players[i].can_reach(ball, self.tick)
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
            self.touches = Touches { team, count: 0, last: None };
        }
        if self.must_not_touch(hitter) {
            self.award_point(1 - team, PointReason::DoubleTouch, ball, events);
            return;
        }
        self.touches.count += 1;
        self.touches.last = Some(hitter);
        if self.touches.count > MAX_TOUCHES {
            self.award_point(1 - team, PointReason::TooManyTouches, ball, events);
            return;
        }

        let (kind, flight) = self.plan_hit(hitter, id, self.touches.count, ball);
        self.launch(hitter, kind, flight, events);
    }

    /// Who blocks a ball crossing the net at `at` moving at `velocity`, and
    /// whether squarely. Serves can't be blocked, and balls going into the net
    /// aren't blocks.
    fn blocker_for(&self, at: Vec3, velocity: Vec3) -> Option<(usize, bool)> {
        if self.last_hit == Some(HitKind::Serve) || at.y < NET_HEIGHT {
            return None;
        }
        // The team on the side the ball is heading into.
        let defending = self.team_on(velocity.x);
        let contacts = (0..self.players.len())
            .filter(|&i| self.players[i].team == defending)
            .filter_map(|i| self.players[i].block_contact(at).map(|stuffed| (i, stuffed)));
        // A square block beats a glancing one.
        contacts.max_by_key(|&(_, stuffed)| stuffed)
    }

    fn launch(&mut self, hitter: usize, kind: HitKind, flight: Flight, events: &mut Vec<Event>) {
        self.last_hit = Some(kind);
        self.ball = Ball::InFlight(flight);
        self.touch_lockout_until = self.tick + TOUCH_LOCKOUT_TICKS;
        events.push(Event::Touched { player: hitter, kind });
    }

    fn award_point(&mut self, team: usize, reason: PointReason, ball_at: Vec3, events: &mut Vec<Event>) {
        self.score[team] += 1;
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

/// Where a shot over the net from the half at `side` lands: the aimed spot, kept
/// on the other half, or `depth` past the net if unaimed. Aiming outside the
/// lines lands out.
pub(crate) fn over_net_target(side: f32, aim: Option<Vec2>, depth: f32) -> Vec3 {
    let spot = aim.unwrap_or(Vec2::new(-side * depth, 0.0));
    let (min_x, max_x) = court::x_range(-side, 1.0, HALF_LENGTH + RUNOFF);
    let max_z = HALF_WIDTH + RUNOFF;
    Vec3::new(spot.x.clamp(min_x, max_x), BALL_RADIUS, spot.y.clamp(-max_z, max_z))
}

/// Where a pass on the half at `side` lands: the aimed spot, kept inside that
/// half, or `depth` from the net if unaimed.
fn own_side_target(side: f32, aim: Option<Vec2>, depth: f32) -> Vec3 {
    let spot = aim.unwrap_or(Vec2::new(side * depth, 0.0));
    let (min_x, max_x) = court::x_range(side, 0.8, HALF_LENGTH - 0.5);
    let max_z = HALF_WIDTH - 0.5;
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
