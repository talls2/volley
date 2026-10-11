//! Plays the bot match that the C interface starts, as fast as it can, and
//! prints the score and a checksum of the state every 600 ticks (10 s), so a
//! client running the DLL can be checked against it tick for tick:
//!
//!     cargo run -p volley_sim_ffi --release --example checksum [TICKS]

use volley_sim::{Event, PlayerInput, TICK_HZ};
use volley_sim_ffi::{checksum, new_match, step};

fn main() {
    let ticks: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(3600);
    let mut sim = new_match();
    let mut touches = 0;
    while sim.tick < ticks {
        touches += step(&mut sim, None, PlayerInput::default())
            .iter()
            .filter(|event| matches!(event, Event::Touched { .. }))
            .count();
        if sim.tick % (10 * TICK_HZ) == 0 {
            println!(
                "tick {:6}  score {:2}-{:2}  sets {}-{}  touches {touches:4}  checksum {:016x}",
                sim.tick,
                sim.score[0],
                sim.score[1],
                sim.sets[0],
                sim.sets[1],
                checksum(&sim),
            );
        }
    }
}
