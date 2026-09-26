//! Court geometry. The net sits on the plane x = 0; team 0 plays on the
//! negative-x half, team 1 on the positive-x half. Units are meters.

use glam::Vec3;

/// Distance from the net to an end line.
pub const HALF_LENGTH: f32 = 9.0;
/// Distance from the center line to a sideline.
pub const HALF_WIDTH: f32 = 4.5;
/// Distance from the net to each attack line.
pub const ATTACK_LINE: f32 = 3.0;
/// How far past the lines players may run.
pub const RUNOFF: f32 = 3.0;

pub const NET_HEIGHT: f32 = 2.43;
/// The net spans |z| <= this.
pub const NET_HALF_WIDTH: f32 = 5.0;

/// Bigger than a real ball (0.105) so it reads well on screen.
pub const BALL_RADIUS: f32 = 0.2;
pub const BALL_GRAVITY: f32 = 9.81;

pub const TEAM_NAMES: [&str; 2] = ["Red", "Blue"];

/// -1 for team 0's half, +1 for team 1's half.
pub fn side(team: usize) -> f32 {
    if team == 0 { -1.0 } else { 1.0 }
}

/// Which team's half an x coordinate is on.
pub fn half_owner(x: f32) -> usize {
    if x < 0.0 { 0 } else { 1 }
}

/// Whether a landed ball counts as in. Touching a line is in.
pub fn is_inside(landed: Vec3) -> bool {
    landed.x.abs() <= HALF_LENGTH + BALL_RADIUS && landed.z.abs() <= HALF_WIDTH + BALL_RADIUS
}

/// The x range `[min, max]` spanning distances `near..far` from the net on `team`'s half.
pub fn x_range(team: usize, near: f32, far: f32) -> (f32, f32) {
    let s = side(team);
    let (a, b) = (s * near, s * far);
    (a.min(b), a.max(b))
}
