//! The match simulation: every rule that decides what happens in a match.
//!
//! Pure Rust with no engine, rendering or networking, so the server, the client
//! and the tests all run exactly the same rules. Advance it with [`Sim::step`]
//! at [`TICK_HZ`]; the same state and inputs always produce the same result.

mod ball;
pub mod bot;
pub mod court;
mod player;

pub use ball::{Ball, Flight};
pub use glam::{Vec2, Vec3};
pub use player::{Dive, HitRequest, Player};

use court::{BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HEIGHT, NET_HALF_WIDTH};

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

const MAX_TOUCHES: u32 = 3;
/// After a touch nobody can touch the ball for this long, so one swing never counts twice.
const TOUCH_LOCKOUT_TICKS: u32 = 10;
const POINT_PAUSE_TICKS: u32 = 90;

// Flight times per kind of hit. Shorter = faster and flatter.
const SERVE_SECONDS: f32 = 1.6;
const PASS_SECONDS: f32 = 1.6;
/// A dig is a scramble, so it goes up higher, giving teammates time to get there.
const DIG_SECONDS: f32 = 1.9;
const LOB_SECONDS: f32 = 1.35;
const SPIKE_SECONDS: f32 = 0.5;

/// Default distance past the net that shots over it aim for. Spikes aim deep:
/// their flat path would otherwise catch the net unless hit from right beside it.
const OVER_DEPTH: f32 = HALF_LENGTH * 0.5;
const SPIKE_DEPTH: f32 = HALF_LENGTH * 0.6;
/// Where a team's first touch goes: mid-court, for a teammate to set.
const RECEIVE_DEPTH: f32 = 3.0;
/// Where a set goes: close to the net, for a teammate to spike.
const SET_DEPTH: f32 = 1.3;

/// One player's controls for one tick. Buttons mean "pressed this tick", not "held".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// World-space XZ direction, length at most 1. Also aims hits.
    pub movement: Vec2,
    pub jump: bool,
    pub pass: bool,
    pub spike: bool,
    pub dive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub players_per_team: usize,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self { players_per_team: 2 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Rally,
    PointScored { resume_tick: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Serve,
    /// Soft high ball to your own side, setting up the next touch.
    Pass,
    /// A pass made while diving.
    Dig,
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
    Dove { player: usize },
    Touched { player: usize, kind: HitKind },
    HitNet { at: Vec3 },
    Landed { at: Vec3, inside: bool },
    Point { team: usize, reason: PointReason },
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
    pub score: [u32; 2],
    pub phase: Phase,
    /// Increments each rally, so observers can tell a reset from movement.
    pub rally: u32,
    pub rally_start_tick: u32,
    serving_team: usize,
    /// Which player on each team serves next.
    serve_rotation: [usize; 2],
    touches: Touches,
    touch_lockout_until: u32,
}

impl Sim {
    pub fn new(config: MatchConfig) -> Self {
        let players = (0..2 * config.players_per_team)
            .map(|i| Player::new(i / config.players_per_team, Vec3::ZERO))
            .collect();
        let mut sim = Self {
            config,
            tick: 0,
            players,
            ball: Ball::Dead { at: Vec3::ZERO },
            score: [0, 0],
            phase: Phase::Rally,
            rally: 0,
            rally_start_tick: 0,
            serving_team: 0,
            serve_rotation: [0, 0],
            touches: Touches { team: 0, count: 0, last: None },
            touch_lockout_until: 0,
        };
        sim.start_rally();
        sim
    }

    /// Advances the match one tick. `inputs` has one entry per player.
    pub fn step(&mut self, inputs: &[PlayerInput]) -> Vec<Event> {
        assert_eq!(inputs.len(), self.players.len(), "one input per player");
        let mut events = Vec::new();
        self.tick += 1;

        if let Phase::PointScored { resume_tick } = self.phase
            && self.tick >= resume_tick
        {
            self.start_rally();
        }

        let ball = self.ball_position();
        for (i, input) in inputs.iter().enumerate() {
            let serving = self.ball == Ball::Held { by: i };
            if self.players[i].update(input, self.tick, serving, ball) {
                events.push(Event::Dove { player: i });
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

    /// Whether the rules forbid `player` from touching the ball next: nobody touches
    /// twice in a row, except a player with no teammates to pass to.
    pub fn must_not_touch(&self, player: usize) -> bool {
        self.config.players_per_team > 1 && self.touches.last == Some(player)
    }

    /// How many times `team` has touched the ball since it came to their side.
    pub fn team_touches(&self, team: usize) -> u32 {
        if self.touches.team == team { self.touches.count } else { 0 }
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
        Vec3::new(court::side(self.players[player].team) * HALF_LENGTH * 0.5, 0.0, z)
    }

    fn start_rally(&mut self) {
        self.rally += 1;
        self.rally_start_tick = self.tick;
        self.phase = Phase::Rally;
        self.touches = Touches { team: self.serving_team, count: 0, last: None };
        self.touch_lockout_until = 0;

        for i in 0..self.players.len() {
            let home = self.home_position(i);
            self.players[i].reset(home);
        }

        let server = self.player_index(self.serving_team, self.serve_rotation[self.serving_team]);
        self.players[server].position = Vec3::new(court::side(self.serving_team) * (HALF_LENGTH + 1.0), 0.0, 0.0);
        self.ball = Ball::Held { by: server };
    }

    fn update_serve(&mut self, server: usize, events: &mut Vec<Event>) {
        if self.players[server].take_hit(self.tick).is_none() {
            return;
        }
        let player = &self.players[server];
        let origin = held_ball_position(player);
        let target = opponent_target(player.team, player.aim, OVER_DEPTH);
        self.touches = Touches { team: player.team, count: 1, last: Some(server) };
        self.launch(server, HitKind::Serve, Flight::to_target(origin, target, SERVE_SECONDS, self.tick), events);
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
            if at.y < NET_HEIGHT + BALL_RADIUS && at.z.abs() <= NET_HALF_WIDTH {
                // Drops back down on the side it came from.
                let v = flight.velocity_at_time(crossing);
                let origin = Vec3::new(-v.x.signum() * (BALL_RADIUS + 0.01), at.y, at.z);
                let velocity = Vec3::new(-v.x * 0.2, v.y.min(0.0) * 0.5, v.z * 0.5);
                self.ball = Ball::InFlight(Flight { origin, velocity, start_tick: self.tick });
                events.push(Event::HitNet { at });
                return;
            }
        }

        if now >= landing {
            let at = flight.landing_point();
            let inside = court::is_inside(at);
            events.push(Event::Landed { at, inside });
            if inside {
                self.award_point(1 - court::half_owner(at.x), PointReason::LandedIn, at, events);
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
                self.players[i].pending_hit(self.tick).is_some() && self.players[i].can_reach(ball, self.tick)
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
        let request = self.players[hitter].take_hit(self.tick);
        let player = self.players[hitter];
        let team = player.team;

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

        let (kind, target, seconds) = if request == Some(HitRequest::Spike) && !player.grounded() {
            (HitKind::Spike, opponent_target(team, player.aim, SPIKE_DEPTH), SPIKE_SECONDS)
        } else if self.touches.count == MAX_TOUCHES {
            (HitKind::Lob, opponent_target(team, player.aim, OVER_DEPTH), LOB_SECONDS)
        } else {
            let depth = if self.touches.count == 1 { RECEIVE_DEPTH } else { SET_DEPTH };
            let target = own_side_target(team, player.aim, depth);
            if player.lunging(self.tick) {
                (HitKind::Dig, target, DIG_SECONDS)
            } else {
                (HitKind::Pass, target, PASS_SECONDS)
            }
        };
        self.launch(hitter, kind, Flight::to_target(ball, target, seconds, self.tick), events);
    }

    fn launch(&mut self, hitter: usize, kind: HitKind, flight: Flight, events: &mut Vec<Event>) {
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
        self.phase = Phase::PointScored { resume_tick: self.tick + POINT_PAUSE_TICKS };
        events.push(Event::Point { team, reason });
    }
}

fn held_ball_position(player: &Player) -> Vec3 {
    player.position + Vec3::new(-court::side(player.team) * 0.35, 1.9, 0.0)
}

/// A spot `depth` meters past the net in the opponent's court, moved around by `aim`.
fn opponent_target(team: usize, aim: Vec2, depth: f32) -> Vec3 {
    let (min_x, max_x) = court::x_range(1 - team, 1.0, HALF_LENGTH - 0.5);
    let x = (-court::side(team) * depth + aim.x * HALF_LENGTH * 0.35).clamp(min_x, max_x);
    let z = (aim.y * HALF_WIDTH * 0.75).clamp(-HALF_WIDTH + 0.3, HALF_WIDTH - 0.3);
    Vec3::new(x, BALL_RADIUS, z)
}

/// A spot `depth` meters from the net on your own side, for a teammate's next touch.
fn own_side_target(team: usize, aim: Vec2, depth: f32) -> Vec3 {
    let (min_x, max_x) = court::x_range(team, 0.8, HALF_LENGTH - 1.0);
    let x = (court::side(team) * depth + aim.x * 2.0).clamp(min_x, max_x);
    let z = (aim.y * HALF_WIDTH * 0.4).clamp(-HALF_WIDTH + 1.0, HALF_WIDTH - 1.0);
    Vec3::new(x, BALL_RADIUS, z)
}

#[cfg(test)]
mod tests;
