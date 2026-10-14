//! Plays the bot match that the C interface starts and prints when the heroes'
//! moments happen (ability moves, crossovers, posterizers, splits, decoys,
//! dribbles, chain spikes, banana kicks and dunks, and glass cracking, body
//! hits, shoves and wall shots), to find them for filming:
//!
//!     cargo run -p volley_sim_ffi --release --example events [TICKS]

use volley_sim::{Event, HitKind, MoveId, PlayerInput};
use volley_sim_ffi::{new_match, step};

fn main() {
    let ticks: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(7200);
    let mut sim = new_match();
    while sim.tick < ticks {
        for event in step(&mut sim, None, PlayerInput::default()) {
            let what = match event {
                Event::MoveStarted { player, id }
                    if matches!(id, MoveId::Crossover | MoveId::Posterizer | MoveId::BananaKick | MoveId::Chilena) =>
                {
                    format!("player {player} starts {id:?}")
                }
                Event::Touched { player, kind: kind @ (HitKind::Dunk | HitKind::Curve), .. } => format!("player {player} hits a {kind:?}"),
                Event::Carried { player }
                | Event::Posterized { player }
                | Event::Split { player }
                | Event::DecoyPopped { player }
                | Event::Dribbled { player }
                | Event::Chained { player }
                | Event::WallShot { player }
                | Event::Shoved { player, .. }
                | Event::BodyHit { player, .. } => format!("{event:?} (player {player})"),
                Event::WallCracked { .. } | Event::WallBroken { .. } | Event::Out { .. } => format!("{event:?}"),
                _ => continue,
            };
            println!("tick {:6}  {what}", sim.tick);
        }
    }
}
