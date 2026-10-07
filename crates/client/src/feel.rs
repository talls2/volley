//! Game feel: a split-second freeze when a hit or stuff block connects
//! (hit-stop) and camera shake to go with it, so big hits land with weight.
//! The harder the ball leaves, the longer the freeze (as in Lethal League):
//! soft passes don't stop at all, spikes and kicks do, and the hitter shakes
//! through it (as in Smash). See docs/animation/experiments.md.
//!
//! The freeze slows the whole game clock, so it only works while the match
//! runs locally; online it will need to be purely visual.

use bevy::prelude::*;
use volley_sim::{Ball, Event, HitKind};

use crate::{Match, SimEvent};

/// How slow the game runs during a hit-stop.
const FROZEN_SPEED: f32 = 0.05;
/// A hit freezes the game `STOP_PER_SPEED` seconds for each m/s the ball
/// leaves faster than `SOFT_SPEED`, up to `MAX_STOP`: about 55 ms for a serve,
/// 80 for a spike, 110 for the hardest kicks, nothing for a pass.
const SOFT_SPEED: f32 = 10.0;
const STOP_PER_SPEED: f32 = 0.006;
const MAX_STOP: f32 = 0.12;
/// How far the hitter shakes at the start of a freeze (m), and how fast.
const HITTER_SHAKE: f32 = 0.05;
const HITTER_SHAKE_HZ: f32 = 30.0;
/// How fast camera shake fades, per real second.
const SHAKE_DECAY: f32 = 1.6;

pub fn plugin(app: &mut App) {
    app.init_resource::<HitStop>()
        .init_resource::<Shake>()
        .init_resource::<BaseSpeed>()
        .add_message::<Froze>()
        .add_systems(Update, (react_to_hits, recover).chain());
}

/// How fast the game clock runs outside hit-stops: 1, or slower while the
/// bench films.
#[derive(Resource)]
pub struct BaseSpeed(pub f32);

impl Default for BaseSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A touch's hit-stop: how fast the ball left (m/s) and how long the game
/// froze for it (real seconds; 0 for none). For measuring (`bench`).
#[derive(Message, Clone, Copy)]
pub struct Froze {
    pub kind: HitKind,
    pub speed: f32,
    pub seconds: f32,
}

/// The freeze under way: who hit (if a player did), and when it started and
/// ends, in real seconds.
#[derive(Resource, Default)]
pub struct HitStop {
    hitter: Option<usize>,
    start: f32,
    until: Option<f32>,
}

impl HitStop {
    /// How far `player` is shaken right now, as a fraction of the shake at
    /// its strongest: a wobble that fades out over the freeze. Shaken across
    /// the facing on the ground, up and down in the air.
    pub fn shake(&self, player: usize, now: f32) -> Option<f32> {
        let until = self.until.filter(|_| self.hitter == Some(player))?;
        let left = ((until - now) / (until - self.start)).clamp(0.0, 1.0);
        Some(HITTER_SHAKE * left * (now * HITTER_SHAKE_HZ * std::f32::consts::TAU).sin())
    }
}

/// Camera shake, from 0 to 1. The camera shakes by its square, so small hits
/// barely register and big ones really move it.
#[derive(Resource, Default)]
pub struct Shake(pub f32);

fn react_to_hits(
    game: Res<Match>,
    base: Res<BaseSpeed>,
    mut froze: MessageWriter<Froze>,
    mut events: MessageReader<SimEvent>,
    real: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut hit_stop: ResMut<HitStop>,
    mut shake: ResMut<Shake>,
) {
    for SimEvent(event) in events.read() {
        let speed = if let Ball::InFlight(flight) = game.current.ball { flight.velocity.length() } else { 0.0 };
        let (freeze, trauma) = match *event {
            // The harder the ball leaves, the longer the stop; a clean attack hits harder.
            Event::Touched { kind, quality, .. } => {
                let stop = ((speed - SOFT_SPEED) * STOP_PER_SPEED).clamp(0.0, MAX_STOP);
                if kind.is_attack() { (stop * (0.5 + 0.5 * quality), 0.15 + 0.35 * quality) } else { (stop, 0.0) }
            }
            Event::Blocked { stuffed: true, .. } => (0.09, 0.6),
            // The catch of a crossover hangs for a beat; a dunk through the block shakes the beach.
            Event::Carried { .. } => (0.04, 0.1),
            Event::Posterized { .. } => (0.12, 0.9),
            Event::Split { .. } => (0.1, 0.5),
            // A chain spike shakes harder; its speed already stretches the stop.
            Event::Chained { .. } => (0.0, 0.35),
            Event::Landed { velocity, .. } if velocity.length() > 15.0 => (0.0, 0.3),
            _ => continue,
        };
        if let Event::Touched { kind, .. } = *event {
            froze.write(Froze { kind, speed, seconds: freeze });
        }
        shake.0 = (shake.0 + trauma).min(1.0);
        if freeze > 0.0 {
            clock.set_relative_speed(FROZEN_SPEED * base.0);
            let now = real.elapsed_secs();
            let hitter = match *event {
                Event::Touched { player, .. } | Event::Blocked { player, .. } | Event::Carried { player } => Some(player),
                _ => None,
            };
            *hit_stop = HitStop { hitter, start: now, until: Some(now + freeze) };
        }
    }
}

fn recover(
    real: Res<Time<Real>>,
    base: Res<BaseSpeed>,
    mut clock: ResMut<Time<Virtual>>,
    mut hit_stop: ResMut<HitStop>,
    mut shake: ResMut<Shake>,
) {
    if hit_stop.until.is_some_and(|until| real.elapsed_secs() >= until) {
        clock.set_relative_speed(base.0);
        *hit_stop = HitStop::default();
    }
    shake.0 = (shake.0 - SHAKE_DECAY * real.delta_secs()).max(0.0);
}
