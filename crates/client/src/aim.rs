//! Aiming, Rematch-style: hits go where the center of the screen points. Turn
//! the camera to pick a direction; tilt it up or down to aim farther or shorter.
//! A marker on the floor shows where your next hit would land.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use volley_sim::{Ball, court};

use crate::Match;
use crate::input::{LOCAL_TEAM, LocalDriver};

/// Aiming at or above the horizon reaches this far.
const MAX_DISTANCE: f32 = 30.0;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, draw_aim_marker);
}

/// Where the camera's center line meets the floor, as world (x, z).
pub fn floor_point(camera: &Transform) -> Vec2 {
    let forward = camera.forward();
    let horizontal = Vec2::new(forward.x, forward.z);
    // The ray drops `height` after `height / -forward.y` of its length, which
    // covers `horizontal.length()` times that across the floor.
    let reach = if forward.y < 0.0 {
        camera.translation.y / -forward.y * horizontal.length()
    } else {
        f32::INFINITY
    };
    let from = Vec2::new(camera.translation.x, camera.translation.z);
    from + horizontal.normalize_or_zero() * reach.min(MAX_DISTANCE)
}

/// Shows where your next hit would go while the ball is yours to play: white
/// if it lands in, red if out.
fn draw_aim_marker(game: Res<Match>, driver: Res<LocalDriver>, mut gizmos: Gizmos) {
    if *driver != LocalDriver::Human {
        return;
    }
    let sim = &game.current;
    let me = sim.player_index(LOCAL_TEAM, 0);
    let yours = match sim.ball {
        Ball::Held { by } => by == me,
        Ball::InFlight(flight) => {
            sim.team_on(flight.landing_point().x) == LOCAL_TEAM && !sim.must_not_touch(me)
        }
        Ball::Dead { .. } => false,
    };
    if !yours {
        return;
    }
    let (_, spot) = sim.preview_hit(me);
    let color = if court::is_inside(spot) { Color::srgb(1.0, 0.95, 0.4) } else { Color::srgb(1.0, 0.25, 0.2) };
    let at = spot.with_y(0.03);
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    // Big enough to read at the far end of the court.
    gizmos.circle(Isometry3d::new(at, flat), 1.0, color);
    gizmos.circle(Isometry3d::new(at, flat), 0.9, color);
    gizmos.line(at - Vec3::X * 0.7, at + Vec3::X * 0.7, color);
    gizmos.line(at - Vec3::Z * 0.7, at + Vec3::Z * 0.7, color);
}
