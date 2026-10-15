//! What the hero select and HUD say about each hero, and who plays whom. The
//! heroes themselves, their kits, live in `volley_sim::moves`.

use bevy::prelude::*;
use volley_sim::moves::{HEROES, RIVALS, team_with};
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
    HeroInfo {
        title: "Soccer superstar",
        blurb: "Does everything with his feet, bends shots around the block, and reads the walls. Can't block.",
        lines: &[
            "Passive, Feet: foot saves reach waist-high and go clean; volley and bicycle kicks hit like spikes",
            "Passive, Wall Pass: balls coming off a wall are easier to reach",
            "Banana Kick {ability}: a kick with spin that bends around the block on its way over",
            "Ultimate, Chilena {ultimate}: a leaping bicycle kick that splits the ball in two; one is a decoy",
            "Weakness: can't block",
        ],
        color: Color::srgb(0.2, 0.85, 0.45),
    },
    HeroInfo {
        title: "Figure-skating champion",
        blurb: "The fastest on the sand, gliding on blades of ice. Can't stop dead, jumps lowest, and has no dive.",
        lines: &[
            "Passive, Glide: accelerate fastest and carve wide turns, but stops and reversals slide",
            "Flash Freeze {ability}: in the air, the ball you hit freezes as it crosses the net, then drops on its way",
            "Ultimate, Triple Axel {ultimate}: a spinning leap into an ice spike that smashes through blocks and leaves a ring of ice",
            "Weakness: no dive",
        ],
        color: Color::srgb(0.45, 0.9, 1.0),
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

/// A new match with you as `choice` and the other two heroes as your
/// teammates, against one of each hero, so there's always every hero to face.
pub fn new_match(choice: usize) -> Sim {
    let mut sim = Sim::new(MatchConfig::default());
    let lineup = [team_with(HEROES[choice]), RIVALS];
    for (team, kits) in lineup.into_iter().enumerate() {
        let team = if team == 0 { LOCAL_TEAM } else { 1 - LOCAL_TEAM };
        for (slot, kit) in kits.into_iter().enumerate() {
            sim.set_kit(sim.player_index(team, slot), kit);
        }
    }
    sim
}
