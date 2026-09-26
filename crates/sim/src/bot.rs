//! Computer players. A bot receives, sets for a teammate, jumps and spikes on
//! the third touch, dives for balls it can't run down, and otherwise gets back
//! into position. Good enough to fill a team and to practice against.

use glam::{Vec2, Vec3};

use crate::player::{DIVE_LUNGE_TICKS, DIVE_SPEED, JUMP_SPEED, PLAYER_GRAVITY, RUN_SPEED};
use crate::{Ball, DT, Flight, PlayerInput, SPIKE_DEPTH, Sim, TICK_HZ, court, opponent_target};

/// How long a bot waits before serving.
const SERVE_DELAY_TICKS: u32 = TICK_HZ;
/// How long after each hit a bot takes to react, like a person. Without it bots
/// return everything, including spikes, and rallies never end.
const REACTION_TICKS: u32 = 15;
/// Ball height (center) a bot meets a pass at.
const PASS_HEIGHT: f32 = 1.2;
/// Ball height a bot spikes at: near the top of its reach when jumping.
const SPIKE_HEIGHT: f32 = 3.1;
/// Only balls coming down this close to the net are worth spiking.
const SPIKE_RANGE: f32 = 4.5;
/// Where an attacker waits for the set.
const APPROACH_DEPTH: f32 = 3.5;

pub fn input_for(sim: &Sim, me: usize) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    let team = player.team;
    let side = court::side(team);

    match sim.ball {
        Ball::Held { by } if by == me => {
            if sim.tick >= sim.rally_start_tick + SERVE_DELAY_TICKS {
                // Vary serves between left, middle and right.
                input.movement = Vec2::new(0.0, (sim.rally % 3) as f32 - 1.0);
                input.pass = true;
            }
            return input;
        }
        Ball::InFlight(flight) if sim.tick >= flight.start_tick + REACTION_TICKS => {
            let attack = sim.team_touches(team) == 2 && attack_point(&flight, team).is_some();
            let meet_at = if attack { SPIKE_HEIGHT } else { PASS_HEIGHT };
            let intercept = flight
                .descending_time_at_height(meet_at)
                .map(|t| (t, flight.position_at_time(t)))
                .filter(|(_, at)| court::half_owner(at.x) == team);
            if let Some((time, at)) = intercept
                && chaser(sim, team, at) == Some(me)
            {
                let seconds_left = time - flight.elapsed(sim.tick);
                let to_ball = Vec2::new(at.x - player.position.x, at.z - player.position.z);
                return if attack {
                    spike(sim, me, &flight, to_ball, seconds_left)
                } else {
                    pass(sim, me, &flight, to_ball, seconds_left)
                };
            }
        }
        _ => {}
    }

    // Not our ball: get into position. After our first touch, get ready to attack.
    let spot = if sim.team_touches(team) == 1 && matches!(sim.ball, Ball::InFlight(_)) {
        sim.home_position(me).with_x(side * APPROACH_DEPTH)
    } else {
        sim.home_position(me)
    };
    input.movement = walk_to(player.position, spot);
    input
}

fn pass(sim: &Sim, me: usize, flight: &Flight, to_ball: Vec2, seconds_left: f32) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    let ball_now = flight.position_at(sim.tick);
    if player.can_reach(ball_now, sim.tick) {
        // Standing still keeps the pass on its default target.
        input.pass = true;
        return input;
    }
    let distance = to_ball.length();
    let run_reach = RUN_SPEED * seconds_left.max(0.0);
    let dive_reach = DIVE_SPEED * DIVE_LUNGE_TICKS as f32 * DT;
    if distance > run_reach + 0.8 && distance < dive_reach + 1.0 && seconds_left < 0.45 && player.grounded() {
        input.dive = true;
        input.movement = to_ball.normalize_or_zero();
        return input;
    }
    input.movement = (to_ball / 0.5).clamp_length_max(1.0);
    input
}

fn spike(sim: &Sim, me: usize, flight: &Flight, to_ball: Vec2, seconds_left: f32) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    if !player.grounded() {
        if player.can_reach(flight.position_at(sim.tick), sim.tick) {
            input.spike = true;
            input.movement = spike_aim(sim, player.team);
        }
        return input;
    }
    input.movement = (to_ball / 0.4).clamp_length_max(1.0);
    // Leave the ground so the top of the jump meets the ball.
    let rise_time = JUMP_SPEED / PLAYER_GRAVITY;
    input.jump = to_ball.length() < 1.2 && seconds_left <= rise_time;
    input
}

/// Where a spike would be hit, if the ball comes down close enough to the net.
fn attack_point(flight: &Flight, team: usize) -> Option<Vec3> {
    let t = flight.descending_time_at_height(SPIKE_HEIGHT)?;
    let at = flight.position_at_time(t);
    (court::side(team) * at.x > 0.0 && at.x.abs() < SPIKE_RANGE).then_some(at)
}

/// Aim for open court: rank nine spots across the opponents' half by distance
/// from the nearest defender, and pick one of the best three. Always picking the
/// very best would make every spike unreturnable.
fn spike_aim(sim: &Sim, team: usize) -> Vec2 {
    let defenders: Vec<Vec3> = (0..sim.config.players_per_team)
        .map(|slot| sim.players[sim.player_index(1 - team, slot)].position)
        .collect();
    let openness = |aim: Vec2| {
        let target = opponent_target(team, aim, SPIKE_DEPTH);
        defenders.iter().map(|d| d.with_y(0.0).distance(target.with_y(0.0))).fold(f32::MAX, f32::min)
    };
    let mut aims: Vec<Vec2> =
        [-1.0, 0.0, 1.0].into_iter().flat_map(|x| [-1.0, 0.0, 1.0].map(|z| Vec2::new(x, z))).collect();
    aims.sort_by(|&a, &b| openness(b).total_cmp(&openness(a)));
    // The tick stands in for randomness, keeping the simulation deterministic.
    aims[sim.tick as usize % 3]
}

/// The teammate who plays the ball at a spot: the nearest one allowed to touch
/// it. Picking exactly one keeps bots from crowding the ball.
fn chaser(sim: &Sim, team: usize, at: Vec3) -> Option<usize> {
    let distance = |i: usize| Vec2::new(sim.players[i].position.x - at.x, sim.players[i].position.z - at.z).length();
    (0..sim.config.players_per_team)
        .map(|slot| sim.player_index(team, slot))
        .filter(|&i| !sim.must_not_touch(i))
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
}

/// Full speed when far, easing off over the last half meter.
fn walk_to(from: Vec3, to: Vec3) -> Vec2 {
    let offset = Vec2::new(to.x - from.x, to.z - from.z);
    if offset.length() < 0.3 { Vec2::ZERO } else { (offset / 0.5).clamp_length_max(1.0) }
}
