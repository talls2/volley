//! Aiming, Rematch-style: hits go the way you're moving (hold back to send it
//! behind you), or the way the camera looks when you stand still, and holding
//! the hit button longer sends it farther. A marker on the floor shows where
//! your next hit would land, and a line points from you to it.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use volley_sim::Ball;

use crate::Match;
use crate::input::{LOCAL_TEAM, LocalDriver};
use crate::scene::player_feet;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, draw_aim_marker);
}

const MARKER_COLOR: Color = Color::srgb(1.0, 0.95, 0.4);

/// Shows where your next hit would go while the ball is yours to play. An
/// outer ring shows how far off a hit from a bad position may stray.
fn draw_aim_marker(game: Res<Match>, driver: Res<LocalDriver>, fixed: Res<Time<Fixed>>, mut gizmos: Gizmos) {
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
        Ball::Carried { by, .. } => by == me,
        Ball::Dead { .. } => false,
    };
    if !yours {
        return;
    }
    let preview = sim.preview_hit(me);
    let at = preview.target.with_y(0.03);
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    // Big enough to read at the far end of the arena.
    gizmos.circle(Isometry3d::new(at, flat), 1.0, MARKER_COLOR);
    gizmos.circle(Isometry3d::new(at, flat), 0.9, MARKER_COLOR);
    gizmos.line(at - Vec3::X * 0.7, at + Vec3::X * 0.7, MARKER_COLOR);
    gizmos.line(at - Vec3::Z * 0.7, at + Vec3::Z * 0.7, MARKER_COLOR);
    if preview.spread > 1.1 {
        gizmos.circle(Isometry3d::new(at, flat), preview.spread, MARKER_COLOR.with_alpha(0.5));
    }
    let feet = player_feet(&game, &fixed, me).with_y(0.03);
    gizmos.line(feet, at, MARKER_COLOR.with_alpha(0.35));
}
