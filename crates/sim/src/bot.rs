//! A minimal computer player: runs under the ball and keeps it alive. It never
//! jumps or spikes. Good enough to practice against and to fill empty slots.

use glam::Vec2;

use crate::{Ball, PlayerInput, Sim, TICK_HZ, court};

/// How long a bot waits before serving.
const SERVE_DELAY_TICKS: u32 = TICK_HZ;
/// The ball height (center) the bot tries to meet the ball at.
const HIT_HEIGHT: f32 = 1.2;

pub fn input_for(sim: &Sim, me: usize) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();

    match sim.ball {
        Ball::Held { by } if by == me => {
            input.pass = sim.tick >= sim.rally_start_tick + SERVE_DELAY_TICKS;
        }
        Ball::InFlight(flight) => {
            let Some(t) = flight.descending_time_at_height(HIT_HEIGHT) else {
                return input;
            };
            let intercept = flight.position_at_time(t);
            let chaser = closest_allowed_teammate(sim, player.team, intercept.x, intercept.z);
            if court::half_owner(intercept.x) != player.team || chaser != Some(me) {
                return input;
            }
            let to_intercept = Vec2::new(intercept.x - player.position.x, intercept.z - player.position.z);
            // Full speed when far, easing off over the last half meter.
            input.movement = (to_intercept / 0.5).clamp_length_max(1.0);
            input.pass = player.can_reach(flight.position_at(sim.tick));
        }
        _ => {}
    }
    input
}

/// The teammate who should play the ball at a spot: the nearest one allowed to
/// touch it. Picking exactly one keeps bots from crowding the ball.
fn closest_allowed_teammate(sim: &Sim, team: usize, x: f32, z: f32) -> Option<usize> {
    let distance = |i: usize| Vec2::new(sim.players[i].position.x - x, sim.players[i].position.z - z).length();
    (0..sim.config.players_per_team)
        .map(|slot| sim.player_index(team, slot))
        .filter(|&i| !sim.must_not_touch(i))
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
}
