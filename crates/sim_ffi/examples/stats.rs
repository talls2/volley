//! Plays the bot match that the C interface starts and prints how it went:
//! points and rally length, who hits the net and with what, and how
//! receptions (passes, digs and foot saves) were met. For checking a rules
//! change against the match before it:
//!
//!     cargo run -p volley_sim_ffi --release --example stats [TICKS]

use std::collections::BTreeMap;

use volley_sim::{Ball, Event, HitKind, PlayerInput, PointReason, TICK_HZ};
use volley_sim_ffi::{new_match, step};

fn main() {
    let ticks: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(36_000);
    let mut sim = new_match();
    let mut last: Option<(usize, HitKind)> = None;
    let mut touches = 0u32;
    let mut points = 0u32;
    let mut reasons: BTreeMap<String, u32> = BTreeMap::new();
    let mut attacks: BTreeMap<String, u32> = BTreeMap::new();
    let mut speeds: BTreeMap<String, f32> = BTreeMap::new();
    let mut nets: BTreeMap<String, u32> = BTreeMap::new();
    let mut receptions: BTreeMap<String, [u32; 4]> = BTreeMap::new();
    while sim.tick < ticks {
        for event in step(&mut sim, None, PlayerInput::default()) {
            match event {
                Event::Touched { player, kind, quality } => {
                    touches += 1;
                    let who = format!("{} {kind:?}", sim.players[player].kit.name);
                    if kind.is_attack() {
                        *attacks.entry(who.clone()).or_default() += 1;
                        if let Ball::InFlight(flight) = sim.ball {
                            *speeds.entry(who.clone()).or_default() += flight.velocity.length();
                        }
                    }
                    if matches!(kind, HitKind::Pass | HitKind::Dig | HitKind::Kick) {
                        // Clean, a little off, wild, shanked: 1 - error.
                        let bucket = if quality >= 0.9 { 0 } else if quality >= 0.7 { 1 } else if quality >= 0.4 { 2 } else { 3 };
                        receptions.entry(format!("{kind:?}")).or_default()[bucket] += 1;
                    }
                    last = Some((player, kind));
                }
                Event::HitNet { .. } => {
                    if let Some((player, kind)) = last {
                        *nets.entry(format!("{} {kind:?}", sim.players[player].kit.name)).or_default() += 1;
                    }
                }
                Event::Point { reason, .. } => {
                    points += 1;
                    let reason = match reason {
                        PointReason::LandedIn => "landed in",
                        PointReason::TooManyTouches => "four touches",
                        PointReason::DoubleTouch => "double touch",
                    };
                    *reasons.entry(reason.to_string()).or_default() += 1;
                }
                _ => {}
            }
        }
    }
    let minutes = ticks as f32 / TICK_HZ as f32 / 60.0;
    println!("{minutes:.0} min: {points} points ({:.1}/min), {:.1} touches a point", points as f32 / minutes, touches as f32 / points.max(1) as f32);
    println!("points: {reasons:?}");
    println!("attacks into the net, of attacks, and how fast they leave (m/s):");
    for (who, count) in &attacks {
        let into = nets.get(who).copied().unwrap_or(0);
        let speed = speeds.get(who).copied().unwrap_or(0.0) / *count as f32;
        println!("  {who:24} {into:4} / {count:4}  ({:3.0}%)  {speed:5.1}", 100.0 * into as f32 / *count as f32);
    }
    let other: u32 = nets.iter().filter(|(who, _)| !attacks.contains_key(*who)).map(|(_, n)| n).sum();
    println!("  other touches into the net: {other}");
    println!("receptions: clean / a little off / wild / shanked");
    for (kind, [a, b, c, d]) in &receptions {
        println!("  {kind:6} {a:5} {b:5} {c:5} {d:5}");
    }
}
