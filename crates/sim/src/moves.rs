//! Moves: everything a player can do to the ball, described as data. A kit is
//! a hero's moves, passives and physical stats, so heroes and their special
//! maneuvers are new data rather than new rules.

use crate::HitKind;

/// The buttons a move can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Pass,
    Spike,
    Dive,
    Kick,
    Ability,
    Ultimate,
}

/// Where the player has to be to start a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Ground,
    Air,
    Either,
}

/// What a touch with the move does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Touch {
    /// Keeps the ball on your side for a teammate, or sends it over on the
    /// team's last touch.
    Keep(HitKind),
    /// Hits it hard over the net, with whatever technique reaches the ball.
    Attack,
    /// Catches the ball (a carry, which only some heroes get away with) and
    /// holds it for `ticks` while the body shifts `shift` meters sideways, then
    /// attacks from there.
    Carry { ticks: u32, shift: f32 },
    /// A slam dunk: an attack faster than a spike that blocks can't stop, and
    /// that knocks down anyone who tries.
    Dunk,
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
    /// Ticks after starting the move before it can start again.
    pub cooldown: u32,
    /// Needs a full ultimate charge, and uses it up.
    pub ultimate: bool,
    /// Starting the move jumps, this many times as fast as a normal jump. The
    /// move lasts until landing.
    pub leap: Option<f32>,
    /// Gravity while the move is underway, as a fraction of normal: below 1
    /// hangs in the air.
    pub gravity: f32,
    /// While armed in the air, the body steers toward a ball within this many
    /// meters so a jump a little off still meets it.
    pub steer: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveId {
    Pass,
    Spike,
    Dive,
    FootSave,
    Crossover,
    Posterizer,
}

impl MoveId {
    pub const ALL: [MoveId; 6] =
        [MoveId::Pass, MoveId::Spike, MoveId::Dive, MoveId::FootSave, MoveId::Crossover, MoveId::Posterizer];

    pub fn spec(self) -> &'static Move {
        match self {
            MoveId::Pass => &PASS,
            MoveId::Spike => &SPIKE,
            MoveId::Dive => &DIVE,
            MoveId::FootSave => &FOOT_SAVE,
            MoveId::Crossover => &CROSSOVER,
            MoveId::Posterizer => &POSTERIZER,
        }
    }

    /// A small number for indexing per-move state, like cooldowns.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A press waits this long for the ball to come in reach, so timing doesn't have
/// to be frame-perfect.
const HIT_WINDOW: u32 = 8;

/// Can't bump a ball at the ankles: those need a foot save or a dive. Stays
/// armed while pass is held (see [`crate::PlayerInput::pass_held`]), then for
/// the hit window after letting go.
const PASS: Move = Move {
    name: "Pass",
    button: Button::Pass,
    stance: Stance::Either,
    windup: 0,
    active: HIT_WINDOW,
    recovery: 0,
    reach: 1.3,
    low: 0.35,
    high: 2.4,
    lunge: None,
    touch: Touch::Keep(HitKind::Pass),
    wobble: 0.0,
    hang: 0.0,
    cooldown: 0,
    ultimate: false,
    leap: None,
    gravity: 1.0,
    steer: None,
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
    steer: Some(3.0),
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
    lunge: Some(11.0),
    touch: Touch::Keep(HitKind::Dig),
    hang: 0.3,
    ..PASS
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
    ..PASS
};

/// Cross's ability. Armed in the air like an attack; when the ball comes in he
/// palms it, swings it across his body while hanging and shifting a meter
/// sideways, then spikes it from there. Blockers who read him jump at the
/// wrong spot, at the wrong moment.
const CROSSOVER: Move = Move {
    name: "Crossover",
    button: Button::Ability,
    touch: Touch::Carry { ticks: 10, shift: 1.2 },
    cooldown: 7 * crate::TICK_HZ,
    ..SPIKE
};

/// Cross's ultimate. A slam-dunk leap about twice as high with hang time,
/// steering to the ball from far away, and a dunk that goes through blocks:
/// best hammered down on the ball from above, on the way down.
const POSTERIZER: Move = Move {
    name: "Posterizer",
    button: Button::Ultimate,
    stance: Stance::Ground,
    reach: 1.8,
    high: 3.0,
    touch: Touch::Dunk,
    ultimate: true,
    leap: Some(1.05),
    gravity: 0.6,
    steer: Some(6.0),
    ..SPIKE
};

/// Traits a hero always has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passive {
    /// Once per possession, two touches in a row without a double-touch fault.
    Dribble,
    /// Hits go somewhere other than where the body faces: defenders react late.
    NoLook,
}

/// A hero: moves, passives and physical stats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kit {
    pub name: &'static str,
    pub run_speed: f32,
    pub jump_speed: f32,
    /// Speed of a dash's burst along the ground.
    pub dash_speed: f32,
    pub moves: &'static [MoveId],
    pub passives: &'static [Passive],
}

impl Kit {
    pub fn has(&self, passive: Passive) -> bool {
        self.passives.contains(&passive)
    }

    pub fn has_move(&self, id: MoveId) -> bool {
        self.moves.contains(&id)
    }

    /// The kit's move with a cooldown or needing a charge: its ability and ultimate.
    pub fn move_on(&self, button: Button) -> Option<MoveId> {
        self.moves.iter().copied().find(|id| id.spec().button == button)
    }

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
    run_speed: 8.5,
    jump_speed: 7.0,
    dash_speed: 15.0,
    moves: &[MoveId::Pass, MoveId::Spike, MoveId::Dive, MoveId::FootSave],
    passives: &[],
};

/// A pro basketball superstar: quicker, a higher jumper and the best ball
/// handler, but no foot save, so low balls are trouble.
pub const CROSS: Kit = Kit {
    name: "Cross",
    run_speed: 9.2,
    jump_speed: 7.6,
    dash_speed: 16.0,
    moves: &[MoveId::Pass, MoveId::Spike, MoveId::Dive, MoveId::Crossover, MoveId::Posterizer],
    passives: &[Passive::Dribble, Passive::NoLook],
};

/// Every hero, in the order the hero select shows them.
pub const HEROES: [Kit; 2] = [ALL_ROUNDER, CROSS];
