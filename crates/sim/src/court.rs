//! Court geometry. The net sits on the plane x = 0, splitting the court into
//! the negative-x half (side -1) and the positive-x half (side +1). Teams switch
//! sides during a match; see `Sim::side`. Units are meters.

use glam::Vec3;

// A third bigger than a real 18 x 9 court in each direction, for more running and diving.

/// Distance from the net to an end line.
pub const HALF_LENGTH: f32 = 12.0;
/// Distance from the center line to a sideline.
pub const HALF_WIDTH: f32 = 6.0;
/// Distance from the net to each attack line.
pub const ATTACK_LINE: f32 = 4.0;
/// How far past the lines players may run.
pub const RUNOFF: f32 = 3.0;

pub const NET_HEIGHT: f32 = 2.43;
/// The net spans |z| <= this.
pub const NET_HALF_WIDTH: f32 = 6.5;

/// A little bigger than a real ball (0.105) so it reads well from behind a player.
pub const BALL_RADIUS: f32 = 0.13;
pub const BALL_GRAVITY: f32 = 9.81;

pub const TEAM_NAMES: [&str; 2] = ["Red", "Blue"];

/// Which half an x coordinate is on, as a side: -1 or +1.
pub fn half_of(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// Whether a landed ball counts as in. Touching a line is in.
pub fn is_inside(landed: Vec3) -> bool {
    landed.x.abs() <= HALF_LENGTH + BALL_RADIUS && landed.z.abs() <= HALF_WIDTH + BALL_RADIUS
}

/// The x range `[min, max]` spanning distances `near..far` from the net on the half at `side`.
pub fn x_range(side: f32, near: f32, far: f32) -> (f32, f32) {
    let (a, b) = (side * near, side * far);
    (a.min(b), a.max(b))
}
