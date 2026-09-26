use glam::{Vec2, Vec3};

use crate::court::{self, HALF_LENGTH, HALF_WIDTH, RUNOFF};
use crate::moves::{ALL_ROUNDER, Button, Kit, MoveId, Stance};
use crate::{DT, PlayerInput, attack};

/// How quickly players speed up and slow down, in m/s². On the ground they reach
/// running speed or stop in about an eighth of a second; in the air they steer less.
const ACCELERATION: f32 = 55.0;
const AIR_ACCELERATION: f32 = 15.0;
/// An armed attack pulls the body toward a ball within this far (horizontally),
/// this hard, so a jump that's a little off still meets it.
const STEER_RANGE: f32 = 3.0;
const STEER_ACCELERATION: f32 = 35.0;
/// Steering aims to close the gap in about this long.
const STEER_SECONDS: f32 = 0.15;
/// Stronger than real gravity so jumps feel snappy. Apex ≈ 1.2 m.
pub(crate) const PLAYER_GRAVITY: f32 = 20.0;

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

/// A move in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Action {
    pub id: MoveId,
    pub start_tick: u32,
    /// World XZ direction of the move's lunge, unit length.
    pub direction: Vec2,
    /// Already touched the ball: the move plays out but can't touch it again.
    pub spent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovePhase {
    Windup,
    Active,
    Recovery,
}

impl Action {
    /// Where the move is at `tick`, or `None` once it's over.
    pub fn phase(&self, tick: u32) -> Option<MovePhase> {
        let spec = self.id.spec();
        let elapsed = tick.saturating_sub(self.start_tick);
        if elapsed < spec.windup {
            Some(MovePhase::Windup)
        } else if elapsed < spec.windup + spec.active {
            Some(MovePhase::Active)
        } else if elapsed < spec.windup + spec.active + spec.recovery {
            Some(MovePhase::Recovery)
        } else {
            None
        }
    }

    /// Moves with a lunge or recovery commit the body: nothing else can start
    /// until they finish.
    fn committed(&self) -> bool {
        let spec = self.id.spec();
        spec.lunge.is_some() || spec.recovery > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub team: usize,
    /// The half this player's team is on: -1 or +1.
    pub side: f32,
    pub kit: Kit,
    /// Position of the feet.
    pub position: Vec3,
    pub vertical_velocity: f32,
    /// Horizontal velocity as world (x, z).
    pub velocity: Vec2,
    /// Where the player's next hit goes, from their latest input.
    pub aim: Option<Vec2>,
    pub action: Option<Action>,
    /// Hands raised to block, until landing.
    pub(crate) hands_up: bool,
}

impl Player {
    pub fn new(team: usize, side: f32, position: Vec3) -> Self {
        Self {
            team,
            side,
            kit: ALL_ROUNDER,
            position,
            vertical_velocity: 0.0,
            velocity: Vec2::ZERO,
            aim: None,
            action: None,
            hands_up: false,
        }
    }

    pub fn grounded(&self) -> bool {
        self.position.y <= 0.0
    }

    /// The move that could touch the ball right now, if any.
    pub fn active_move(&self, tick: u32) -> Option<MoveId> {
        self.action.filter(|action| !action.spent && action.phase(tick) == Some(MovePhase::Active)).map(|action| action.id)
    }

    /// The direction of the body's lunge, while a lunging move is underway.
    pub fn lunge(&self, tick: u32) -> Option<Vec2> {
        let action = self.action?;
        let lunging = action.id.spec().lunge.is_some()
            && matches!(action.phase(tick), Some(MovePhase::Windup | MovePhase::Active));
        lunging.then_some(action.direction)
    }

    /// Whether the move `id` could touch a ball at `ball`.
    pub fn reaches(&self, id: MoveId, ball: Vec3) -> bool {
        let spec = id.spec();
        let horizontal = Vec2::new(ball.x - self.position.x, ball.z - self.position.z).length();
        let height = ball.y - self.position.y;
        // No reaching across the net into the other half.
        let on_our_side = self.side * ball.x > -court::BALL_RADIUS;
        horizontal <= spec.reach && (spec.low..=spec.high).contains(&height) && on_our_side
    }

    /// Whether the move underway could touch a ball at `ball` (a pass, if none is).
    pub fn can_reach(&self, ball: Vec3, tick: u32) -> bool {
        self.reaches(self.active_move(tick).unwrap_or(MoveId::Pass), ball)
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

    /// Uses up the active move on a touch, returning which move made it.
    pub(crate) fn spend(&mut self, tick: u32) -> Option<MoveId> {
        let id = self.active_move(tick)?;
        let action = self.action.as_mut()?;
        action.spent = true;
        if !action.committed() {
            self.action = None;
        }
        Some(id)
    }

    pub(crate) fn reset(&mut self, position: Vec3) {
        *self = Player { kit: self.kit, ..Self::new(self.team, self.side, position) };
    }

    /// Applies one tick of input. Returns a move that just started (not one
    /// merely re-pressed). A server holding the ball must stay behind the end line.
    pub(crate) fn update(&mut self, input: &PlayerInput, tick: u32, serving: bool, ball: Vec3) -> Option<MoveId> {
        if self.action.is_some_and(|action| action.phase(tick).is_none()) {
            self.action = None;
        }
        self.aim = input.aim;

        let committed = self.action.is_some_and(|action| action.committed());
        if input.pass && !committed && self.position.x.abs() < BLOCK_DISTANCE && self.side * ball.x < 0.0 {
            // Block: hands up, jumping first if still on the ground.
            self.hands_up = true;
            if self.grounded() {
                self.vertical_velocity = self.kit.jump_speed;
            }
        }

        let mut started = None;
        let presses = [(input.dive, Button::Dive), (input.kick, Button::Kick), (input.spike, Button::Spike), (input.pass, Button::Pass)];
        for (pressed, button) in presses {
            if !pressed || committed || (serving && button != Button::Pass) {
                continue;
            }
            let Some(id) = self.kit.move_for(button, self.grounded()) else {
                continue;
            };
            let repressed = self.action.is_some_and(|action| action.id == id && !action.spent);
            let toward_ball = Vec2::new(ball.x - self.position.x, ball.z - self.position.z);
            let direction = [input.movement, toward_ball]
                .into_iter()
                .find(|d| d.length() > 0.1)
                .unwrap_or(Vec2::new(-self.side, 0.0))
                .normalize();
            self.action = Some(Action { id, start_tick: tick, direction, spent: false });
            if !repressed {
                started = Some(id);
            }
            break;
        }

        let phase = self.action.and_then(|action| action.phase(tick).map(|phase| (action, phase)));
        match phase {
            Some((action, MovePhase::Windup | MovePhase::Active)) if action.id.spec().lunge.is_some() => {
                self.velocity = action.direction * action.id.spec().lunge.unwrap_or_default();
            }
            Some((action, MovePhase::Recovery)) if action.id.spec().recovery > 0 => self.velocity = Vec2::ZERO,
            _ => {
                let (wanted, acceleration) = match self.steering(tick, ball) {
                    Some(to_spot) => ((to_spot / STEER_SECONDS).clamp_length_max(self.kit.run_speed), STEER_ACCELERATION),
                    None if self.grounded() => (input.movement.clamp_length_max(1.0) * self.kit.run_speed, ACCELERATION),
                    None => (input.movement.clamp_length_max(1.0) * self.kit.run_speed, AIR_ACCELERATION),
                };
                self.velocity = move_towards(self.velocity, wanted, acceleration * DT);
                if input.jump && self.grounded() {
                    self.vertical_velocity = self.kit.jump_speed;
                }
            }
        }
        self.position.x += self.velocity.x * DT;
        self.position.z += self.velocity.y * DT;

        let (min_x, max_x) = if serving {
            court::x_range(self.side, HALF_LENGTH + 0.3, HALF_LENGTH + RUNOFF)
        } else {
            court::x_range(self.side, 0.4, HALF_LENGTH + RUNOFF)
        };
        let max_z = HALF_WIDTH + RUNOFF;
        let unclamped = self.position;
        self.position.x = self.position.x.clamp(min_x, max_x);
        self.position.z = self.position.z.clamp(-max_z, max_z);
        // Running into the net or the edge of the sand stops you.
        if self.position.x != unclamped.x {
            self.velocity.x = 0.0;
        }
        if self.position.z != unclamped.z {
            self.velocity.y = 0.0;
        }

        if !self.grounded() || self.vertical_velocity > 0.0 {
            self.vertical_velocity -= PLAYER_GRAVITY * DT;
            self.position.y += self.vertical_velocity * DT;
            if self.position.y <= 0.0 {
                self.position.y = 0.0;
                self.vertical_velocity = 0.0;
                self.hands_up = false;
                // Air moves last until landing.
                if self.action.is_some_and(|action| action.id.spec().stance == Stance::Air) {
                    self.action = None;
                }
            }
        }
        started
    }

    /// While an armed attack is in the air near the ball on our side, the
    /// horizontal offset to where the body should be to hit it.
    fn steering(&self, tick: u32, ball: Vec3) -> Option<Vec2> {
        let armed = !self.grounded() && self.active_move(tick).is_some_and(MoveId::steers);
        let to_spot = attack::steer_position(self, ball) - Vec2::new(self.position.x, self.position.z);
        let near = Vec2::new(ball.x - self.position.x, ball.z - self.position.z).length() < STEER_RANGE;
        (armed && near && self.side * ball.x > 0.0).then_some(to_spot)
    }
}

/// `from` moved toward `to` by at most `step`.
fn move_towards(from: Vec2, to: Vec2, step: f32) -> Vec2 {
    let delta = to - from;
    if delta.length() <= step { to } else { from + delta.normalize() * step }
}
