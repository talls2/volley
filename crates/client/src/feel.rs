//! Game feel: a split-second freeze when a spike or stuff block connects
//! (hit-stop) and camera shake to go with it, so big hits land with weight.
//!
//! The freeze slows the whole game clock, so it only works while the match
//! runs locally; online it will need to be purely visual.

use bevy::prelude::*;
use volley_sim::Event;

use crate::SimEvent;

/// How slow the game runs during a hit-stop.
const FROZEN_SPEED: f32 = 0.05;
/// How fast camera shake fades, per real second.
const SHAKE_DECAY: f32 = 1.6;

pub fn plugin(app: &mut App) {
    app.init_resource::<HitStop>().init_resource::<Shake>().add_systems(Update, (react_to_hits, recover).chain());
}

/// When the current freeze ends, in real seconds.
#[derive(Resource, Default)]
struct HitStop(Option<f32>);

/// Camera shake, from 0 to 1. The camera shakes by its square, so small hits
/// barely register and big ones really move it.
#[derive(Resource, Default)]
pub struct Shake(pub f32);

fn react_to_hits(
    mut events: MessageReader<SimEvent>,
    real: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut hit_stop: ResMut<HitStop>,
    mut shake: ResMut<Shake>,
) {
    for SimEvent(event) in events.read() {
        let (freeze, trauma) = match *event {
            // A clean attack hits harder.
            Event::Touched { kind, quality, .. } if kind.is_attack() => (0.03 + 0.05 * quality, 0.15 + 0.35 * quality),
            Event::Blocked { stuffed: true, .. } => (0.09, 0.6),
            // The catch of a crossover hangs for a beat; a dunk through the block shakes the beach.
            Event::Carried { .. } => (0.04, 0.1),
            Event::Posterized { .. } => (0.12, 0.9),
            Event::Landed { velocity, .. } if velocity.length() > 15.0 => (0.0, 0.3),
            _ => continue,
        };
        shake.0 = (shake.0 + trauma).min(1.0);
        if freeze > 0.0 {
            clock.set_relative_speed(FROZEN_SPEED);
            hit_stop.0 = Some(real.elapsed_secs() + freeze);
        }
    }
}

fn recover(real: Res<Time<Real>>, mut clock: ResMut<Time<Virtual>>, mut hit_stop: ResMut<HitStop>, mut shake: ResMut<Shake>) {
    if hit_stop.0.is_some_and(|until| real.elapsed_secs() >= until) {
        clock.set_relative_speed(1.0);
        hit_stop.0 = None;
    }
    shake.0 = (shake.0 - SHAKE_DECAY * real.delta_secs()).max(0.0);
}
