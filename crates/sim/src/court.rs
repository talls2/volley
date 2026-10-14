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
/// The part of a standing body a ball can hit, from the feet (m).
pub const BODY_BOTTOM: f32 = 0.1;
pub const BODY_TOP: f32 = 1.85;

/// The walls are glass panels this long (m): a ball cracks a panel, the
/// next breaks it, and through a broken panel the ball is out.
pub const WALL_PANEL: f32 = 6.0;
/// Panels across each end wall, along each side wall, and in all. Numbered:
/// the end wall at -x, then +x (across z from -z), then the side wall at
/// -z, then +z (along x from -x).
pub const END_PANELS: usize = (2.0 * HALF_WIDTH / WALL_PANEL) as usize;
pub const SIDE_PANELS: usize = (2.0 * HALF_LENGTH / WALL_PANEL) as usize;
pub const PANELS: usize = 2 * END_PANELS + 2 * SIDE_PANELS;

/// The panel a ball at `at` hits: on an end wall (`end`) or a side wall.
pub fn wall_panel(at: glam::Vec3, end: bool) -> usize {
    if end {
        let i = (((at.z + HALF_WIDTH) / WALL_PANEL) as usize).min(END_PANELS - 1);
        if at.x < 0.0 { i } else { END_PANELS + i }
    } else {
        let i = (((at.x + HALF_LENGTH) / WALL_PANEL) as usize).min(SIDE_PANELS - 1);
        2 * END_PANELS + if at.z < 0.0 { i } else { SIDE_PANELS + i }
    }
}

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
