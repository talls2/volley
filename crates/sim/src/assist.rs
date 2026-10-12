//! One button for every touch on your side. Pass, foot save and dive are all
//! the pass button, and the game picks which from where the ball is:
//!
//! - coming in below the pelvis (the chest, for great feet) and in a foot's
//!   reach: a foot save;
//! - a ball a pass will meet: a pass;
//! - out of reach, landing before you could run there, while you steer
//!   toward it: a dive, if one gets there;
//! - otherwise a pass, armed and waiting as always. While it waits it turns
//!   into a foot save or a dive the moment that's what the ball needs.
//!
//! Serves and blocks stay on the pass button as before. For a person's input:
//! bots press their saves themselves.

use glam::{Vec2, Vec3};

use crate::player::{BLOCK_DISTANCE, Player};
use crate::{Ball, DT, Flight, MoveId, Passive, PlayerInput, Sim};

/// Balls below this (m over the feet) are kicked rather than passed; great
/// feet kick up to `FEET_KICK_HEIGHT`.
const PELVIS_HEIGHT: f32 = 1.0;
const FEET_KICK_HEIGHT: f32 = 1.5;
/// Steering toward the ball: the movement within this angle of it (cosine),
/// and pushed at least this far.
const TOWARD: f32 = 0.5;
const STEERING: f32 = 0.3;

/// What the pass button does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Pass,
    FootSave,
    Dive,
}

/// `input` with the pass button turned into the touch the ball needs.
pub fn one_button(sim: &Sim, me: usize, input: PlayerInput) -> PlayerInput {
    let player = &sim.players[me];
    let armed = player.active_move(sim.tick) == Some(MoveId::Pass);
    if !(input.pass || (input.pass_held && armed)) {
        return input;
    }
    match choose(sim, me, input.movement) {
        Choice::Pass => input,
        Choice::FootSave => PlayerInput { pass: false, pass_held: false, kick: true, ..input },
        Choice::Dive => PlayerInput { pass: false, pass_held: false, dive: true, ..input },
    }
}

/// What a press of the pass button would do now, moving `movement` (also
/// whether a pass already waiting should give way to a save).
pub fn choose(sim: &Sim, me: usize, movement: Vec2) -> Choice {
    let player = &sim.players[me];
    let Ball::InFlight(flight) = sim.ball else { return Choice::Pass };
    if !player.grounded() || sim.must_not_touch(me) {
        return Choice::Pass;
    }
    let ball = flight.position_at(sim.tick);
    // At the net with the ball across, pass blocks.
    if player.side * ball.x < 0.0 && player.position.x.abs() < BLOCK_DISTANCE {
        return Choice::Pass;
    }
    let toward_ball = Vec2::new(ball.x - player.position.x, ball.z - player.position.z);
    let way = if movement.length() > STEERING { movement } else { toward_ball };

    let pass = first_touch(sim, me, &flight, MoveId::Pass, way);
    let kick = player.kit.has_move(MoveId::FootSave).then(|| first_touch(sim, me, &flight, MoveId::FootSave, way)).flatten();
    let low = if player.kit.has(Passive::Feet) { FEET_KICK_HEIGHT } else { PELVIS_HEIGHT };
    // Coming in low and in a foot's reach: kick it, ahead of a low pass.
    if let Some((_, height)) = kick
        && height < low
        && pass.is_none_or(|(_, pass_height)| pass_height < low)
    {
        return Choice::FootSave;
    }
    if pass.is_some() {
        return Choice::Pass;
    }
    // Out of reach: dive for it, steering toward it, if running wouldn't get there.
    let steering = movement.length() > STEERING && movement.normalize().dot(toward_ball.normalize_or_zero()) > TOWARD;
    if steering && !runs_there(player, &flight, sim.tick) && first_touch(sim, me, &flight, MoveId::Dive, movement).is_some() {
        return Choice::Dive;
    }
    // A waiting pass keeps waiting; a press arms one.
    Choice::Pass
}

/// When move `id`, started now toward `way`, first touches the ball: ticks
/// from now and the ball's height over the feet. The body runs on at its
/// velocity, or lunges for a lunging move; a foot save's leg extends as in a
/// real touch.
fn first_touch(sim: &Sim, me: usize, flight: &Flight, id: MoveId, way: Vec2) -> Option<(u32, f32)> {
    let spec = id.spec();
    let mut body = sim.players[me];
    let lunge = spec.lunge.map(|speed| way.normalize_or_zero() * speed);
    let landing = flight.landing_time();
    for tick in 1..=spec.windup + spec.active {
        if flight.elapsed(sim.tick + tick) >= landing {
            return None;
        }
        let velocity = lunge.unwrap_or(body.velocity);
        body.position += Vec3::new(velocity.x, 0.0, velocity.y) * DT;
        if tick <= spec.windup || sim.tick + tick < sim.touch_lockout_until {
            continue;
        }
        let ball = flight.position_at(sim.tick + tick);
        if body.reaches_scaled(id, ball, Player::extension(id, tick)) {
            return Some((tick, ball.y - body.position.y));
        }
    }
    None
}

/// Whether running flat out gets a pass to the ball before it lands.
fn runs_there(player: &Player, flight: &Flight, tick: u32) -> bool {
    let left = flight.landing_time() - flight.elapsed(tick);
    let landing = flight.landing_point();
    let distance = Vec2::new(landing.x - player.position.x, landing.z - player.position.z).length();
    distance <= player.kit.run_speed * left + MoveId::Pass.spec().reach
}
