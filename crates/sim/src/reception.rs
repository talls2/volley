//! Receiving the ball: how hard it is to control and how well it's met decide
//! where a pass, dig or foot save goes.
//!
//! Control comes from timing and position. Pressing just before the touch is
//! on time; pressing early (or holding the pass long) sets the platform too
//! soon to adjust, and meeting the ball at full stretch is shakier still.
//! Difficulty comes from the ball: how fast it comes, how steeply it drops,
//! how sharply it has to be turned toward the aim, and whether a hero's
//! ability or ultimate sent it.
//!
//! A soft ball met on time goes where it was aimed. A hard one met badly
//! sprays wider, and past a point it shanks: it carries on roughly the way it
//! came, deflected at random, lower and faster, as a real volleyball does off
//! a badly angled platform.

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use crate::court::BALL_RADIUS;
use crate::player::Player;
use crate::{MoveId, Passive, dice};

/// Pressed within this many ticks of the touch is on time; each tick earlier
/// loses `CONTROL_PER_EARLY_TICK` of control, down to `EARLIEST_CONTROL`.
pub(crate) const ON_TIME_TICKS: u32 = 4;
const CONTROL_PER_EARLY_TICK: f32 = 1.0 / 16.0;
const EARLIEST_CONTROL: f32 = 0.2;
/// Within this share of a move's reach the ball is met comfortably; at full
/// stretch control is down by `STRETCH_COST`.
const COMFORT_SHARE: f32 = 0.55;
const STRETCH_COST: f32 = 0.5;
/// A dive is a scramble, and a foot is no platform (unless it's a great one):
/// the best control each manages.
const DIG_CONTROL: f32 = 0.7;
const FOOT_CONTROL: f32 = 0.75;
const GREAT_FOOT_CONTROL: f32 = 0.95;

/// Balls slower than `EASY_SPEED` (m/s) are easy to control; by `HARD_SPEED`
/// as hard as they come. Steepness and turning add to it, from these angles.
const EASY_SPEED: f32 = 10.0;
const HARD_SPEED: f32 = 28.0;
const STEEP_FROM: f32 = 20.0 * PI / 180.0;
const STEEP_SPAN: f32 = 50.0 * PI / 180.0;
const TURN_FROM: f32 = 45.0 * PI / 180.0;
const TURN_SPAN: f32 = 120.0 * PI / 180.0;
/// A hero's ability or ultimate is this much harder to receive.
const POWER_DIFFICULTY: f32 = 0.35;

/// Error spreads a reception's landing up to this many meters around its aim.
const ERROR_SPREAD: f32 = 3.0;
/// From `SHANK_FROM` error a reception starts to shank, fully by `SHANK_FULL`.
const SHANK_FROM: f32 = 0.3;
const SHANK_FULL: f32 = 0.7;
/// A shank deflects up to this far either side of the ball's path, flies
/// between these distances (m), and this much quicker than a pass.
const SHANK_ANGLE: f32 = 80.0 * PI / 180.0;
const SHANK_NEAR: f32 = 2.0;
const SHANK_FAR: f32 = 10.0;
pub(crate) const SHANK_SPEED: f32 = 0.35;

/// How a reception went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reception {
    /// How well the ball was met, 0 to 1.
    pub control: f32,
    /// How hard the ball was to receive, 0 to 1.7.
    pub difficulty: f32,
}

impl Reception {
    /// How far off the reception goes: hard balls met badly.
    pub fn error(&self) -> f32 {
        self.difficulty * (1.0 - self.control)
    }

    /// How far (m) around its aim it can land.
    pub fn spread(&self) -> f32 {
        ERROR_SPREAD * self.error()
    }

    /// How much it shanks, 0 (not at all) to 1 (wholly).
    pub fn shank(&self) -> f32 {
        let x = ((self.error() - SHANK_FROM) / (SHANK_FULL - SHANK_FROM)).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    }
}

/// Judges `player`'s reception with move `id`, pressed `pressed_ticks_ago`, of
/// a ball at `ball` coming in at `incoming`, aimed toward `out` (world XZ).
/// `power`: sent by a hero's ability or ultimate.
pub fn judge(player: &Player, id: MoveId, pressed_ticks_ago: u32, ball: Vec3, incoming: Vec3, out: Vec2, power: bool) -> Reception {
    // Timing: on time, or early.
    let early = pressed_ticks_ago.saturating_sub(ON_TIME_TICKS) as f32;
    let timing = (1.0 - early * CONTROL_PER_EARLY_TICK).max(EARLIEST_CONTROL);
    // Position: comfortable, or stretching.
    let horizontal = Vec2::new(ball.x - player.position.x, ball.z - player.position.z).length();
    let stretch = ((horizontal / player.reach_of(id) - COMFORT_SHARE) / (1.0 - COMFORT_SHARE)).clamp(0.0, 1.0);
    let best = match id {
        MoveId::Dive => DIG_CONTROL,
        MoveId::FootSave if player.kit.has(Passive::Feet) => GREAT_FOOT_CONTROL,
        MoveId::FootSave => FOOT_CONTROL,
        _ => 1.0,
    };
    let control = best * timing * (1.0 - STRETCH_COST * stretch);

    // The ball: fast, steep, turned sharply, or a hero's power.
    let speed = incoming.length();
    let fast = ((speed - EASY_SPEED) / (HARD_SPEED - EASY_SPEED)).clamp(0.0, 1.0);
    let across = Vec2::new(incoming.x, incoming.z);
    let drop = (-incoming.y).atan2(across.length());
    let steep = ((drop - STEEP_FROM) / STEEP_SPAN).clamp(0.0, 1.0);
    // Sending it back the way it came is easiest; turning it hardest.
    let turn = match (across.try_normalize(), out.try_normalize()) {
        (Some(from), Some(to)) => (-from).angle_to(to).abs(),
        _ => 0.0,
    };
    let turned = ((turn - TURN_FROM) / TURN_SPAN).clamp(0.0, 1.0);
    let difficulty = fast * (0.75 + 0.25 * steep + 0.35 * turned) + if power { POWER_DIFFICULTY } else { 0.0 };
    Reception { control, difficulty }
}

/// Where a shanked ball goes: on along its path, deflected at random, a
/// varying distance.
pub fn shank_target(ball: Vec3, incoming: Vec3, out: Vec2, tick: u32, hitter: usize) -> Vec3 {
    let path = Vec2::new(incoming.x, incoming.z).try_normalize().or_else(|| out.try_normalize()).unwrap_or(Vec2::X);
    let roll = dice(tick, (hitter as u32).wrapping_add(0x5A17));
    let angle = ((roll & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * SHANK_ANGLE;
    let distance = SHANK_NEAR + (SHANK_FAR - SHANK_NEAR) * (roll >> 16) as f32 / 65535.0;
    let way = Vec2::from_angle(angle).rotate(path);
    Vec3::new(ball.x + way.x * distance, BALL_RADIUS, ball.z + way.y * distance)
}
