use glam::Vec3;

use crate::DT;
use crate::court::{BALL_GRAVITY, BALL_RADIUS};

const GRAVITY: Vec3 = Vec3::new(0.0, -BALL_GRAVITY, 0.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ball {
    /// The server holds the ball before serving.
    Held { by: usize },
    InFlight(Flight),
    /// The rally is over; the ball rests where it ended.
    Dead { at: Vec3 },
}

/// A ballistic path. Between touches the ball's entire future is known from
/// the moment it was hit, so positions are computed, never integrated. This is
/// what lets the network send touches instead of streaming ball positions.
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

    pub fn position_at_time(&self, t: f32) -> Vec3 {
        self.origin + self.velocity * t + 0.5 * GRAVITY * t * t
    }

    pub fn position_at(&self, tick: u32) -> Vec3 {
        self.position_at_time(self.elapsed(tick))
    }

    pub fn velocity_at_time(&self, t: f32) -> Vec3 {
        self.velocity + GRAVITY * t
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

    /// When the ball crosses the net plane (x = 0), if it's heading that way.
    pub fn net_crossing_time(&self) -> Option<f32> {
        if self.velocity.x == 0.0 {
            return None;
        }
        let t = -self.origin.x / self.velocity.x;
        (t > 0.0).then_some(t)
    }
}
