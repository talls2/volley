//! Headless match server. For now it plays bots against each other in real time
//! and logs what happens. Networking plugs in here next: remote players' inputs
//! replace the bot inputs, and events go out to the clients.

use std::time::{Duration, Instant};

use volley_sim::court::TEAM_NAMES;
use volley_sim::{Event, MatchConfig, Sim, TICK_HZ, bot};

fn main() {
    let players_per_team = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(1);
    let mut sim = Sim::new(MatchConfig { players_per_team });
    println!("volley server: {players_per_team}v{players_per_team} bots at {TICK_HZ} Hz (Ctrl-C to stop)");

    let tick = Duration::from_secs(1) / TICK_HZ;
    let mut next_tick = Instant::now();
    loop {
        let inputs: Vec<_> = (0..sim.players.len()).map(|i| bot::input_for(&sim, i)).collect();
        for event in sim.step(&inputs) {
            let seconds = sim.tick as f32 / TICK_HZ as f32;
            match event {
                Event::Point { team, reason } => println!(
                    "[{seconds:7.2}s] point {} ({reason:?})  {} {} : {} {}",
                    TEAM_NAMES[team], TEAM_NAMES[0], sim.score[0], sim.score[1], TEAM_NAMES[1],
                ),
                other => println!("[{seconds:7.2}s] {other:?}"),
            }
        }
        next_tick += tick;
        std::thread::sleep(next_tick.saturating_duration_since(Instant::now()));
    }
}
