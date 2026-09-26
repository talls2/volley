//! Computer players. A bot receives, sets for a teammate, jumps and spikes on
//! the third touch, dives for balls it can't run down, and otherwise gets back
//! into position. Good enough to fill a team and to practice against.

use glam::{Vec2, Vec3};

fn default<T: Default>() -> T {
    T::default()
}

use crate::moves::MoveId;
use crate::player::{BLOCK_DISTANCE, PLAYER_GRAVITY};
use crate::court::{BALL_RADIUS, HALF_LENGTH, HALF_WIDTH, NET_HEIGHT};
use crate::{Ball, DT, Flight, HitKind, OVER_DEPTH, PlayerInput, SET_DEPTH, Sim, TICK_HZ, dice, flight_seconds};

/// How long a bot waits before serving.
const SERVE_DELAY_TICKS: u32 = TICK_HZ;
/// How long after each hit a bot takes to react, like a person. Fast balls
/// take longer to read. Without this bots return every spike and rallies never end.
const REACTION_TICKS: u32 = 15;
/// Fast balls take this long plus up to `REACTION_SPREAD_TICKS` more, varying
/// ball to ball like a person's would; a fixed delay makes every spike either
/// always dug or never dug.
const FAST_BALL_REACTION_TICKS: u32 = 18;
const REACTION_SPREAD_TICKS: u32 = 7;
/// Balls faster than this (m/s, as hit) count as fast: spikes, mostly.
const FAST_BALL_SPEED: f32 = 15.0;
/// Ball height (center) a bot meets a pass at.
const PASS_HEIGHT: f32 = 1.2;
/// Ball height a bot spikes at: near the top of its reach when jumping.
const SPIKE_HEIGHT: f32 = 3.1;
/// Only balls coming down this close to the net are worth spiking.
const SPIKE_RANGE: f32 = 4.5;
/// Where an attacker waits for the set.
const APPROACH_DEPTH: f32 = 3.5;
/// Where a blocker stands, from the net.
const BLOCK_SPOT: f32 = 0.6;
/// One attack in this many goes unblocked, varying with the set.
const BLOCK_SKIP_EVERY: u32 = 3;

pub fn input_for(sim: &Sim, me: usize) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    let team = player.team;
    let side = sim.side(team);

    match sim.ball {
        Ball::Held { by } if by == me => {
            if sim.tick >= sim.rally_start_tick + SERVE_DELAY_TICKS {
                // Vary serves between left, middle and right.
                let z = ((sim.rally % 3) as f32 - 1.0) * HALF_WIDTH * 0.6;
                input.aim = Some(Vec2::new(-side * OVER_DEPTH, z));
                input.pass = true;
            }
            return input;
        }
        Ball::InFlight(flight) if sim.tick >= flight.start_tick + reaction_ticks(&flight) => {
            let attack = sim.team_touches(team) == 2 && attack_point(&flight, side).is_some();
            let meet_at = if attack { SPIKE_HEIGHT } else { PASS_HEIGHT };
            let intercept = flight
                .descending_time_at_height(meet_at)
                .map(|t| (t, flight.position_at_time(t)))
                .filter(|(_, at)| sim.team_on(at.x) == team);
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

    if let Some(input) = block(sim, me) {
        return input;
    }

    // Not our ball: get into position. After our first touch, get ready to
    // attack. While a teammate blocks, cover the side the block leaves open.
    let spot = if sim.team_touches(team) == 1 && matches!(sim.ball, Ball::InFlight(_)) {
        sim.home_position(me).with_x(side * APPROACH_DEPTH)
    } else if let Some(attack) = incoming_attack(sim, team) {
        Vec3::new(side * HALF_LENGTH * 0.55, 0.0, (-attack.z).clamp(-3.0, 3.0))
    } else {
        sim.home_position(me)
    };
    input.movement = walk_to(player.position, spot);
    input
}

fn reaction_ticks(flight: &Flight) -> u32 {
    if flight.velocity.length() > FAST_BALL_SPEED {
        // The hit's start tick stands in for randomness, keeping the simulation deterministic.
        FAST_BALL_REACTION_TICKS + dice(flight.start_tick, 0) % REACTION_SPREAD_TICKS
    } else {
        REACTION_TICKS
    }
}

/// Whether the ball is close enough to press for a hit. Presses are held for a
/// few ticks, so pressing a little early connects the moment the ball arrives;
/// waiting for "in reach right now" is always a tick late and misses balls that
/// are only briefly in reach.
fn ball_close(sim: &Sim, me: usize, flight: &Flight) -> bool {
    let player = &sim.players[me];
    let ball = flight.position_at(sim.tick);
    let horizontal = Vec2::new(ball.x - player.position.x, ball.z - player.position.z).length();
    horizontal < 1.8 && ball.y - player.position.y < 3.2
}

fn pass(sim: &Sim, me: usize, flight: &Flight, to_ball: Vec2, seconds_left: f32) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    let landing = flight.landing_point();
    let direction = Vec2::new(landing.x - player.position.x, landing.z - player.position.z).normalize_or_zero();
    // Coming in too low for the arms: a foot gets it, if it's in reach.
    if player.grounded()
        && !would_connect(sim, me, flight, MoveId::Pass, direction)
        && would_connect(sim, me, flight, MoveId::FootSave, direction)
    {
        return PlayerInput { kick: true, movement: direction, ..input };
    }
    if ball_close(sim, me, flight) {
        input.pass = true;
        input.aim = pass_aim(sim, me);
        input.movement = (to_ball / 0.5).clamp_length_max(1.0);
        return input;
    }
    // Can't run there in time: stick a foot out, or dive, if that would get it.
    let run_reach = player.kit.run_speed * seconds_left.max(0.0);
    if to_ball.length() > run_reach + 0.3 && player.grounded() {
        if would_connect(sim, me, flight, MoveId::FootSave, direction) {
            return PlayerInput { kick: true, movement: direction, ..input };
        }
        if would_connect(sim, me, flight, MoveId::Dive, direction) {
            return PlayerInput { dive: true, movement: direction, ..input };
        }
    }
    input.movement = (to_ball / 0.5).clamp_length_max(1.0);
    input
}

/// Whether starting move `id` now, lunging toward `direction`, would touch
/// the ball: plays the move forward tick by tick against the ball's flight.
fn would_connect(sim: &Sim, me: usize, flight: &Flight, id: MoveId, direction: Vec2) -> bool {
    let spec = id.spec();
    let speed = spec.lunge.unwrap_or_default();
    let mut body = sim.players[me];
    (1..=spec.windup + spec.active).any(|tick| {
        body.position += Vec3::new(direction.x, 0.0, direction.y) * speed * DT;
        tick > spec.windup && body.reaches(id, flight.position_at(sim.tick + tick))
    })
}

fn spike(sim: &Sim, me: usize, flight: &Flight, to_ball: Vec2, seconds_left: f32) -> PlayerInput {
    let player = &sim.players[me];
    let mut input = PlayerInput::default();
    input.movement = (to_ball / 0.4).clamp_length_max(1.0);
    if !player.grounded() {
        // Arm the attack right away; it steers the rest of the way.
        input.spike = player.active_move(sim.tick).is_none();
        input.aim = Some(spike_aim(sim, player.team, flight.position_at(sim.tick)));
        return input;
    }
    // Leave the ground so the top of the jump meets the ball. The armed
    // attack's steering covers the last couple of meters.
    let rise_time = player.kit.jump_speed / PLAYER_GRAVITY;
    input.jump = to_ball.length() < 2.5 && seconds_left <= rise_time;
    input
}

/// When the other team has the ball, one teammate blocks: they walk to the net
/// across from the player who will attack, then press block (pass at the net)
/// just before the spike, so their hands are highest as it crosses. Some
/// attacks go unblocked and some blocks are a little off, as people misjudge.
fn block(sim: &Sim, me: usize) -> Option<PlayerInput> {
    let Ball::InFlight(flight) = sim.ball else { return None };
    let player = &sim.players[me];
    let side = sim.side(player.team);
    // Already up: keep drifting to the net with hands up until landing,
    // instead of chasing the spike and leaving the block.
    if player.blocking() {
        return Some(PlayerInput { movement: walk_to(player.position, player.position.with_x(side * BLOCK_SPOT)), ..default() });
    }
    let attackers = 1 - player.team;
    let seed = dice(flight.start_tick, sim.rally);
    if seed % BLOCK_SKIP_EVERY == 0 {
        return None;
    }
    // Before the set, line up with whoever will attack: the receiver, since
    // nobody touches twice in a row. After it, with where the spike will be hit.
    let (attack_z, until_spike) = match sim.team_touches(attackers) {
        1 => (sim.players[sim.last_toucher(attackers)?].position.z, None),
        2 => {
            let attack = attack_point(&flight, sim.side(attackers))?;
            (attack.z, Some(flight.descending_time_at_height(SPIKE_HEIGHT)? - flight.elapsed(sim.tick)))
        }
        _ => return None,
    };
    let blocker = (0..sim.config.players_per_team)
        .map(|slot| sim.player_index(player.team, slot))
        .min_by(|&a, &b| (sim.players[a].position.z - attack_z).abs().total_cmp(&(sim.players[b].position.z - attack_z).abs()))?;
    if blocker != me {
        return None;
    }
    // Lining up exactly every time would stuff every spike.
    let misjudge = (seed / BLOCK_SKIP_EVERY % 3) as f32 * 0.4 - 0.4;
    let spot = Vec3::new(side * BLOCK_SPOT, 0.0, attack_z + misjudge);
    let rise_time = player.kit.jump_speed / PLAYER_GRAVITY;
    let in_range = player.position.x.abs() < BLOCK_DISTANCE;
    Some(PlayerInput {
        movement: walk_to(player.position, spot),
        // The spike crosses the net about 0.1 s after it's hit, so jump just before.
        pass: player.grounded() && in_range && until_spike.is_some_and(|t| t <= rise_time - 0.1),
        ..default()
    })
}

/// Where the other team is about to spike from, if they are.
fn incoming_attack(sim: &Sim, team: usize) -> Option<Vec3> {
    let Ball::InFlight(flight) = sim.ball else { return None };
    let attackers = 1 - team;
    if sim.team_touches(attackers) != 2 {
        return None;
    }
    attack_point(&flight, sim.side(attackers))
}

/// Where a spike would be hit, if the ball comes down close enough to the net.
fn attack_point(flight: &Flight, side: f32) -> Option<Vec3> {
    let t = flight.descending_time_at_height(SPIKE_HEIGHT)?;
    let at = flight.position_at_time(t);
    (side * at.x > 0.0 && at.x.abs() < SPIKE_RANGE).then_some(at)
}

/// A set goes to the net in front of a teammate, so they can spike it. Other
/// passes go to the default spot.
fn pass_aim(sim: &Sim, me: usize) -> Option<Vec2> {
    let team = sim.players[me].team;
    if sim.team_touches(team) != 1 {
        return None;
    }
    let attacker = (0..sim.config.players_per_team).map(|slot| sim.player_index(team, slot)).find(|&i| i != me)?;
    Some(Vec2::new(sim.side(team) * SET_DEPTH, sim.players[attacker].position.z))
}

/// Aim for open court, around any block: rank nine spots across the opponents'
/// half by distance from the nearest defender, ruling out shots that would hit
/// the net or cross it through a blocker's hands, and pick one of the best three. Always picking
/// the very best would make every spike unreturnable.
fn spike_aim(sim: &Sim, team: usize, from: Vec3) -> Vec2 {
    let defenders: Vec<Vec3> = (0..sim.config.players_per_team)
        .map(|slot| sim.players[sim.player_index(1 - team, slot)].position)
        .collect();
    // Now and then an attacker doesn't read the block and hits straight into it.
    let reads_block = dice(sim.tick, sim.rally.wrapping_add(1)) % 4 != 0;
    let blocked = |spot: Vec2| {
        // Where the straight path to `spot` crosses the net, sideways.
        let share = from.x.abs() / (from.x.abs() + spot.x.abs());
        let crossing_z = from.z + (spot.y - from.z) * share;
        reads_block && defenders.iter().any(|d| d.x.abs() < BLOCK_DISTANCE && (d.z - crossing_z).abs() < 0.5)
    };
    let clears_net = |spot: Vec2| {
        let target = Vec3::new(spot.x, BALL_RADIUS, spot.y);
        let seconds = flight_seconds(HitKind::Spike, from.with_y(0.0).distance(target.with_y(0.0)));
        let flight = Flight::to_target(from, target, seconds, 0);
        flight.net_crossing_time().is_some_and(|t| flight.position_at_time(t).y > NET_HEIGHT + BALL_RADIUS + 0.1)
    };
    let openness = |spot: Vec2| {
        let open = defenders.iter().map(|d| Vec2::new(d.x, d.z).distance(spot)).fold(f32::MAX, f32::min);
        match (clears_net(spot), blocked(spot)) {
            (false, _) => open - 1000.0,
            (true, true) => open - 100.0,
            (true, false) => open,
        }
    };
    let side = sim.side(team);
    let mut aims: Vec<Vec2> = [0.35, 0.6, 0.85]
        .into_iter()
        .flat_map(|depth| [-0.7, 0.0, 0.7].map(|z| Vec2::new(-side * depth * HALF_LENGTH, z * HALF_WIDTH)))
        .collect();
    aims.sort_by(|&a, &b| openness(b).total_cmp(&openness(a)));
    aims[dice(sim.tick, sim.rally) as usize % 3]
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
