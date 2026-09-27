use glam::Vec3;

use crate::DT;
use crate::court::{BALL_GRAVITY, BALL_RADIUS, HALF_LENGTH, HALF_WIDTH};

const GRAVITY: Vec3 = Vec3::new(0.0, -BALL_GRAVITY, 0.0);

/// How far the ball's center gets from the middle before it touches the walls.
const REACH_X: f32 = HALF_LENGTH - BALL_RADIUS;
const REACH_Z: f32 = HALF_WIDTH - BALL_RADIUS;

/// Where a ball moving freely to `u` really is, bouncing off walls at
/// `±half`: `u` folded back and forth between them, like light between two
/// mirrors. Also which way it's heading there: +1 if the same way as `u`
/// grows, -1 if bounced back.
fn fold(u: f32, half: f32) -> (f32, f32) {
    let m = (u + half).rem_euclid(4.0 * half);
    if m < 2.0 * half { (m - half, 1.0) } else { (3.0 * half - m, -1.0) }
}

/// How many times a ball moving freely to `u` has hit the walls at `±half`.
fn bounces(u: f32, half: f32) -> u32 {
    ((u + half) / (2.0 * half)).floor().abs() as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ball {
    /// The server holds the ball before serving.
    Held { by: usize },
    InFlight(Flight),
    /// Carried in the air by a hero allowed to, until it's released as an attack.
    Carried { by: usize, release_tick: u32 },
    /// The rally is over; the ball rests where it ended.
    Dead { at: Vec3 },
}

/// A ballistic path. Between touches the ball's entire future is known from
/// the moment it was hit, so positions are computed, never integrated. This is
/// what lets the network send touches instead of streaming ball positions.
///
/// The arena's walls bounce the ball back without slowing it, so bouncing
/// only folds the free path back into the arena (see [`fold`]) and the whole
/// flight, bounces included, stays one formula.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub origin: Vec3,
    pub velocity: Vec3,
    pub start_tick: u32,
}

impl Flight {
    /// Launches from `origin` so the ball arrives at `target` after `seconds`.
    pub fn to_target(origin: Vec3, target: Vec3, seconds: f32, start_tick: u32) -> Self {
        let velocity = (target - origin - 0.5 * GRAVITY * seconds * seconds) / seconds;
        Self { origin, velocity, start_tick }
    }

    /// Seconds since launch at `tick`.
    pub fn elapsed(&self, tick: u32) -> f32 {
        tick.saturating_sub(self.start_tick) as f32 * DT
    }

    /// Where the ball would be at `t` with no walls.
    fn free(&self, t: f32) -> Vec3 {
        self.origin + self.velocity * t + 0.5 * GRAVITY * t * t
    }

    pub fn position_at_time(&self, t: f32) -> Vec3 {
        let free = self.free(t);
        Vec3::new(fold(free.x, REACH_X).0, free.y, fold(free.z, REACH_Z).0)
    }

    pub fn position_at(&self, tick: u32) -> Vec3 {
        self.position_at_time(self.elapsed(tick))
    }

    pub fn velocity_at_time(&self, t: f32) -> Vec3 {
        let (free, v) = (self.free(t), self.velocity + GRAVITY * t);
        Vec3::new(v.x * fold(free.x, REACH_X).1, v.y, v.z * fold(free.z, REACH_Z).1)
    }

    /// How many times the ball has bounced off the walls by `t`.
    pub fn wall_bounces(&self, t: f32) -> u32 {
        let free = self.free(t);
        bounces(free.x, REACH_X) + bounces(free.z, REACH_Z)
    }

    /// When the ball, on its way down, passes through `height` (ball center).
    pub fn descending_time_at_height(&self, height: f32) -> Option<f32> {
        let vy = self.velocity.y;
        let discriminant = vy * vy + 2.0 * BALL_GRAVITY * (self.origin.y - height);
        (discriminant >= 0.0).then(|| (vy + discriminant.sqrt()) / BALL_GRAVITY)
    }

    /// When the bottom of the ball touches the floor.
    pub fn landing_time(&self) -> f32 {
        self.descending_time_at_height(BALL_RADIUS).unwrap_or(0.0)
    }

    pub fn landing_point(&self) -> Vec3 {
        self.position_at_time(self.landing_time())
    }

    /// When the ball first crosses the net plane (x = 0), if it does.
    pub fn net_crossing_time(&self) -> Option<f32> {
        self.next_net_crossing(0.0)
    }

    /// The first time after `after` that the ball crosses the net plane. A
    /// ball bouncing off an end wall can come back across.
    pub fn next_net_crossing(&self, after: f32) -> Option<f32> {
        let vx = self.velocity.x;
        if vx == 0.0 {
            return None;
        }
        // The folded x is 0 wherever the free x is a multiple of 2 * REACH_X.
        let period = 2.0 * REACH_X;
        let free = (self.origin.x + vx * after) / period;
        let k = if vx > 0.0 { free.floor() + 1.0 } else { free.ceil() - 1.0 };
        Some((k * period - self.origin.x) / vx)
    }
}
