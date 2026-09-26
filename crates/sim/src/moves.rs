//! Moves: everything a player can do to the ball, described as data. A kit is
//! a set of moves plus physical stats, so heroes and their special maneuvers
//! are new data rather than new rules.

use crate::HitKind;

/// The buttons a move can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Pass,
    Spike,
    Dive,
    Kick,
}

/// Where the player has to be to start a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Ground,
    Air,
    Either,
}

/// What a touch with the move does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touch {
    /// Keeps the ball on your side for a teammate, or sends it over on the
    /// team's last touch.
    Keep(HitKind),
    /// Hits it hard over the net, with whatever technique reaches the ball.
    Attack,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub name: &'static str,
    pub button: Button,
    pub stance: Stance,
    /// Ticks before the move can touch the ball, then how long it can, then how
    /// long the player is stuck recovering.
    pub windup: u32,
    pub active: u32,
    pub recovery: u32,
    /// The ball can be touched within `reach` of the body's center line, between
    /// `low` and `high` above the feet.
    pub reach: f32,
    pub low: f32,
    pub high: f32,
    /// Speed of the body's burst during windup and active, like a dive's lunge.
    pub lunge: Option<f32>,
    pub touch: Touch,
    /// Touches land up to this far from where they were aimed.
    pub wobble: f32,
    /// Extra seconds in the air, so scrambled balls go up high enough to reach.
    pub hang: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveId {
    Pass,
    Spike,
    Dive,
    FootSave,
}

impl MoveId {
    /// Whether the player steers toward the ball in the air while this move is
    /// armed: attacks do, so a jump a little off still meets the ball.
    pub fn steers(self) -> bool {
        self.spec().touch == Touch::Attack
    }

    pub fn spec(self) -> &'static Move {
        match self {
            MoveId::Pass => &PASS,
            MoveId::Spike => &SPIKE,
            MoveId::Dive => &DIVE,
            MoveId::FootSave => &FOOT_SAVE,
        }
    }
}

/// A press waits this long for the ball to come in reach, so timing doesn't have
/// to be frame-perfect.
const HIT_WINDOW: u32 = 8;

/// Can't bump a ball below the knees: low balls need a foot save or a dive.
const PASS: Move = Move {
    name: "Pass",
    button: Button::Pass,
    stance: Stance::Either,
    windup: 0,
    active: HIT_WINDOW,
    recovery: 0,
    reach: 1.0,
    low: 0.5,
    high: 2.4,
    lunge: None,
    touch: Touch::Keep(HitKind::Pass),
    wobble: 0.0,
    hang: 0.0,
};

/// Armed for the rest of the jump (air moves end on landing), with a wide zone
/// that takes the ball with a hand or a foot, whichever reaches it: see
/// [`crate::attack`].
const SPIKE: Move = Move {
    name: "Spike",
    button: Button::Spike,
    stance: Stance::Air,
    active: 2 * crate::TICK_HZ,
    reach: 1.5,
    low: 0.2,
    high: 2.9,
    touch: Touch::Attack,
    ..PASS
};

/// A lunge along the floor that digs any low ball it reaches, then time on the
/// ground getting back up.
const DIVE: Move = Move {
    name: "Dive",
    button: Button::Dive,
    stance: Stance::Ground,
    windup: 0,
    active: 24,
    recovery: 30,
    reach: 1.5,
    low: 0.0,
    high: 1.3,
    lunge: Some(9.0),
    touch: Touch::Keep(HitKind::Dig),
    wobble: 0.0,
    hang: 0.3,
};

/// The save that keeps you on your feet: a leg shot out to a ball too low for
/// the arms, almost instantly and farther than a pass reaches. The kick is
/// rough but goes up high, giving a teammate time to get under it, and the
/// stumble after is short. A dive reaches farther but leaves you on the ground.
const FOOT_SAVE: Move = Move {
    name: "Foot save",
    button: Button::Kick,
    stance: Stance::Ground,
    windup: 1,
    active: 12,
    recovery: 15,
    reach: 2.2,
    low: 0.0,
    high: 0.9,
    lunge: Some(3.0),
    touch: Touch::Keep(HitKind::Kick),
    wobble: 1.2,
    hang: 0.6,
};

/// A player's moves and physical stats. Heroes will each have their own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kit {
    pub name: &'static str,
    pub run_speed: f32,
    pub jump_speed: f32,
    pub moves: &'static [MoveId],
}

impl Kit {
    /// The move `button` does in this kit, standing or in the air.
    pub fn move_for(&self, button: Button, grounded: bool) -> Option<MoveId> {
        self.moves.iter().copied().find(|id| {
            let spec = id.spec();
            spec.button == button
                && match spec.stance {
                    Stance::Ground => grounded,
                    Stance::Air => !grounded,
                    Stance::Either => true,
                }
        })
    }
}

pub const ALL_ROUNDER: Kit = Kit {
    name: "All-rounder",
    run_speed: 6.5,
    jump_speed: 7.0,
    moves: &[MoveId::Pass, MoveId::Spike, MoveId::Dive, MoveId::FootSave],
};
