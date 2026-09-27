//! What the hero select and HUD say about each hero, and who plays whom. The
//! heroes themselves, their kits, live in `volley_sim::moves`.

use bevy::prelude::*;
use volley_sim::moves::{ALL_ROUNDER, CROSS, HEROES};
use volley_sim::{MatchConfig, Sim};

use crate::input::{ActiveDevice, LOCAL_TEAM};

pub struct HeroInfo {
    /// Where they come from.
    pub title: &'static str,
    pub blurb: &'static str,
    /// Passives, ability and ultimate, one line each. `{ability}` and
    /// `{ultimate}` become the buttons for the device in use.
    pub lines: &'static [&'static str],
    pub color: Color,
}

/// In the same order as [`HEROES`].
pub const INFO: [HeroInfo; HEROES.len()] = [
    HeroInfo {
        title: "Beach volleyball pro",
        blurb: "Solid everywhere, no tricks and no weak spots.",
        lines: &["Foot save: a leg out for balls too low for the arms", "No ability or ultimate: pure volleyball"],
        color: Color::srgb(0.95, 0.85, 0.55),
    },
    HeroInfo {
        title: "Pro basketball superstar",
        blurb: "Quick, springy, and the best ball-handler on the sand. Bends the rules his way, but has no foot save.",
        lines: &[
            "Passive, Dribble: once per possession, touch the ball twice in a row without a fault",
            "Passive, No-look: defenders read your hits late",
            "Crossover {ability}: in the air, palm the ball across your body past the block, then spike",
            "Ultimate, Posterizer {ultimate}: charged by touches; a huge hanging leap and a dunk that flattens any block",
        ],
        color: Color::srgb(1.0, 0.55, 0.15),
    },
];

/// A hero's lines with the buttons filled in.
pub fn lines(hero: &HeroInfo, device: ActiveDevice) -> Vec<String> {
    let (ability, ultimate) = buttons(device);
    hero.lines.iter().map(|line| line.replace("{ability}", ability).replace("{ultimate}", ultimate)).collect()
}

/// The ability and ultimate buttons, as shown to the player.
pub fn buttons(device: ActiveDevice) -> (&'static str, &'static str) {
    match device {
        ActiveDevice::Keyboard => ("[R]", "[G]"),
        ActiveDevice::Gamepad => ("[X]", "[Y]"),
    }
}

/// A new match with you as `choice` and an All-rounder teammate, against a
/// Cross and an All-rounder, so there's always a Cross to face.
pub fn new_match(choice: usize) -> Sim {
    let mut sim = Sim::new(MatchConfig::default());
    let lineup = [[HEROES[choice], ALL_ROUNDER], [CROSS, ALL_ROUNDER]];
    for (team, kits) in lineup.into_iter().enumerate() {
        let team = if team == 0 { LOCAL_TEAM } else { 1 - LOCAL_TEAM };
        for (slot, kit) in kits.into_iter().enumerate() {
            sim.set_kit(sim.player_index(team, slot), kit);
        }
    }
    sim
}
