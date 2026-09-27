//! Arena geometry. The net sits on the plane x = 0, splitting the arena into
//! the negative-x half (side -1) and the positive-x half (side +1), and walls
//! all around keep the ball in play: nothing is ever out. Teams switch sides
//! during a match; see `Sim::side`. Units are meters.

// Twice a real 18 x 9 court's size and more, 3 a side: room to dash, leap and
// use powers.

/// Distance from the net to an end wall.
pub const HALF_LENGTH: f32 = 24.0;
/// Distance from the center line to a side wall.
pub const HALF_WIDTH: f32 = 12.0;

pub const NET_HEIGHT: f32 = 2.43;
/// The net spans the arena, wall to wall.
pub const NET_HALF_WIDTH: f32 = HALF_WIDTH;

/// A little bigger than a real ball (0.105) so it reads well from behind a player.
pub const BALL_RADIUS: f32 = 0.13;
pub const BALL_GRAVITY: f32 = 9.81;

/// How close players get to the walls and the net.
pub const BODY_RADIUS: f32 = 0.4;

pub const TEAM_NAMES: [&str; 2] = ["Red", "Blue"];

/// Which half an x coordinate is on, as a side: -1 or +1.
pub fn half_of(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// The x range `[min, max]` spanning distances `near..far` from the net on the half at `side`.
pub fn x_range(side: f32, near: f32, far: f32) -> (f32, f32) {
    let (a, b) = (side * near, side * far);
    (a.min(b), a.max(b))
}
