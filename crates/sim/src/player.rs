use glam::{Vec2, Vec3};

use crate::court::{self, HALF_LENGTH, HALF_WIDTH, RUNOFF};
use crate::{DT, PlayerInput};

pub(crate) const RUN_SPEED: f32 = 6.5;
/// Stronger than real gravity so jumps feel snappy. Apex ≈ 1.2 m.
pub(crate) const PLAYER_GRAVITY: f32 = 20.0;
pub(crate) const JUMP_SPEED: f32 = 7.0;

/// How far from the player's center line the ball can be touched.
const REACH_RADIUS: f32 = 1.0;
/// Ball-center heights above the feet that can be touched.
const REACH_LOW: f32 = 0.2;
const REACH_HIGH: f32 = 2.4;

/// A dive is a lunge along the floor, then time on the ground getting back up.
pub(crate) const DIVE_SPEED: f32 = 9.0;
pub(crate) const DIVE_LUNGE_TICKS: u32 = 24;
const DIVE_RECOVERY_TICKS: u32 = 30;
/// Reach while lunging: wider, and down to the floor.
const DIVE_REACH_RADIUS: f32 = 1.5;
const DIVE_REACH_HIGH: f32 = 1.3;

/// Within this distance of the net, while the ball is on the other side, pass
/// means block: a jump with the hands up.
pub(crate) const BLOCK_DISTANCE: f32 = 1.2;
/// Blocking hands cover this far to each side of the player's center...
const BLOCK_HALF_WIDTH: f32 = 0.6;
/// ...and squarely enough to stuff the ball within this far.
const STUFF_HALF_WIDTH: f32 = 0.3;
/// Heights above the feet the blocking hands cover, from forearms to fingertips.
const BLOCK_LOW: f32 = 1.3;
const BLOCK_HIGH: f32 = 2.4;

/// A hit press stays active this long, waiting for the ball to come in reach.
/// Without it, players must press on the exact tick, which feels unresponsive.
const HIT_BUFFER_TICKS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitRequest {
    Pass,
    Spike,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dive {
    /// World-space XZ direction, unit length.
    pub direction: Vec2,
    pub start_tick: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub team: usize,
    /// Position of the feet.
    pub position: Vec3,
    pub vertical_velocity: f32,
    /// Where the player's next hit goes, from their latest input.
    pub aim: Option<Vec2>,
    pub dive: Option<Dive>,
    /// Hands raised to block, until landing.
    pub(crate) hands_up: bool,
    pending_hit: Option<(HitRequest, u32)>,
}

impl Player {
    pub fn new(team: usize, position: Vec3) -> Self {
        Self { team, position, vertical_velocity: 0.0, aim: None, dive: None, hands_up: false, pending_hit: None }
    }

    pub fn grounded(&self) -> bool {
        self.position.y <= 0.0
    }

    /// Whether the player is in the lunge part of a dive.
    pub fn lunging(&self, tick: u32) -> bool {
        self.dive.is_some_and(|dive| tick < dive.start_tick + DIVE_LUNGE_TICKS)
    }

    pub fn can_reach(&self, ball: Vec3, tick: u32) -> bool {
        let (radius, low, high) = if self.lunging(tick) {
            (DIVE_REACH_RADIUS, 0.0, DIVE_REACH_HIGH)
        } else {
            (REACH_RADIUS, REACH_LOW, REACH_HIGH)
        };
        let horizontal = Vec2::new(ball.x - self.position.x, ball.z - self.position.z).length();
        let height = ball.y - self.position.y;
        // No reaching across the net into the other half.
        let on_our_side = court::side(self.team) * ball.x > -court::BALL_RADIUS;
        horizontal <= radius && (low..=high).contains(&height) && on_our_side
    }

    /// In the air right by the net, hands up.
    pub fn blocking(&self) -> bool {
        self.hands_up && !self.grounded() && self.position.x.abs() < BLOCK_DISTANCE
    }

    /// Whether this player's block catches a ball crossing the net at `at`:
    /// `Some(true)` squarely (a stuff block), `Some(false)` with the edge of the hands.
    pub(crate) fn block_contact(&self, at: Vec3) -> Option<bool> {
        let sideways = (at.z - self.position.z).abs();
        let height = at.y - self.position.y;
        let covered = self.blocking() && sideways <= BLOCK_HALF_WIDTH && (BLOCK_LOW..=BLOCK_HIGH).contains(&height);
        covered.then_some(sideways <= STUFF_HALF_WIDTH)
    }

    /// The pending hit, if one was pressed recently enough.
    pub fn pending_hit(&self, tick: u32) -> Option<HitRequest> {
        self.pending_hit.filter(|&(_, expires)| tick <= expires).map(|(hit, _)| hit)
    }

    pub(crate) fn take_hit(&mut self, tick: u32) -> Option<HitRequest> {
        let hit = self.pending_hit(tick);
        self.pending_hit = None;
        hit
    }

    pub(crate) fn reset(&mut self, position: Vec3) {
        *self = Self::new(self.team, position);
    }

    /// Applies one tick of input and returns whether a dive started. A server
    /// holding the ball must stay behind the end line.
    pub(crate) fn update(&mut self, input: &PlayerInput, tick: u32, serving: bool, ball: Vec3) -> bool {
        if self.dive.is_some_and(|dive| tick >= dive.start_tick + DIVE_LUNGE_TICKS + DIVE_RECOVERY_TICKS) {
            self.dive = None;
        }

        self.aim = input.aim;
        let mut dove = false;
        if input.dive && self.dive.is_none() && self.grounded() && !serving {
            let toward_ball = Vec2::new(ball.x - self.position.x, ball.z - self.position.z);
            let direction = [input.movement, toward_ball]
                .into_iter()
                .find(|d| d.length() > 0.1)
                .unwrap_or(Vec2::new(-court::side(self.team), 0.0))
                .normalize();
            self.dive = Some(Dive { direction, start_tick: tick });
            // Diving is how you dig: it passes any ball that comes in reach during the lunge.
            self.pending_hit = Some((HitRequest::Pass, tick + DIVE_LUNGE_TICKS));
            dove = true;
        } else if input.spike {
            self.pending_hit = Some((HitRequest::Spike, tick + HIT_BUFFER_TICKS));
        } else if input.pass {
            let at_net = self.position.x.abs() < BLOCK_DISTANCE;
            let ball_across = court::side(self.team) * ball.x < 0.0;
            if at_net && ball_across && self.dive.is_none() {
                // Block: hands up, jumping first if still on the ground.
                self.hands_up = true;
                if self.grounded() {
                    self.vertical_velocity = JUMP_SPEED;
                }
            }
            self.pending_hit = Some((HitRequest::Pass, tick + HIT_BUFFER_TICKS));
        }

        if let Some(dive) = self.dive {
            if self.lunging(tick) {
                self.position.x += dive.direction.x * DIVE_SPEED * DT;
                self.position.z += dive.direction.y * DIVE_SPEED * DT;
            }
        } else {
            let movement = input.movement.clamp_length_max(1.0);
            self.position.x += movement.x * RUN_SPEED * DT;
            self.position.z += movement.y * RUN_SPEED * DT;
            if input.jump && self.grounded() {
                self.vertical_velocity = JUMP_SPEED;
            }
        }

        let (min_x, max_x) = if serving {
            court::x_range(self.team, HALF_LENGTH + 0.3, HALF_LENGTH + RUNOFF)
        } else {
            court::x_range(self.team, 0.4, HALF_LENGTH + RUNOFF)
        };
        let max_z = HALF_WIDTH + RUNOFF;
        self.position.x = self.position.x.clamp(min_x, max_x);
        self.position.z = self.position.z.clamp(-max_z, max_z);

        if !self.grounded() || self.vertical_velocity > 0.0 {
            self.vertical_velocity -= PLAYER_GRAVITY * DT;
            self.position.y += self.vertical_velocity * DT;
            if self.position.y <= 0.0 {
                self.position.y = 0.0;
                self.vertical_velocity = 0.0;
                self.hands_up = false;
            }
        }
        dove
    }
}
