use glam::{Vec2, Vec3};

use crate::court::{self, BODY_RADIUS, HALF_LENGTH, HALF_WIDTH};
use crate::moves::{ALL_ROUNDER, Button, Kit, MoveId, Passive, Stance};
use crate::{Aim, DT, PlayerInput, attack};

/// How quickly players speed up and slow down, in m/s². On the ground they reach
/// running speed or stop in about an eighth of a second; in the air they steer less.
const ACCELERATION: f32 = 70.0;
const AIR_ACCELERATION: f32 = 18.0;
/// A server may stand anywhere this deep from the end wall.
const SERVE_ZONE: f32 = 4.0;
/// An armed attack pulls the body toward a ball within its move's steering
/// range this hard, so a jump that's a little off still meets it.
const STEER_ACCELERATION: f32 = 45.0;
/// Gravity while carrying the ball, as a fraction of normal: the carrier hangs.
const CARRY_GRAVITY: f32 = 0.25;
/// Steering aims to close the gap in about this long.
const STEER_SECONDS: f32 = 0.15;
/// A running jump goes higher: takeoff speed grows by up to this fraction at
/// full running speed, about a third more height.
const APPROACH_BONUS: f32 = 0.15;
/// Letting go of jump while still rising cuts the climb to this speed: tap for
/// a short hop, hold for a full jump.
const SHORT_HOP_SPEED: f32 = 3.5;
/// A dash lasts this long, and can't be repeated until this long after it starts.
pub(crate) const DASH_TICKS: u32 = 11;
const DASH_COOLDOWN_TICKS: u32 = 40;
/// Landing faster than this (m/s, horizontally) skids in the sand: less grip for a moment.
const SKID_SPEED: f32 = 3.0;
const SKID_TICKS: u32 = 9;
const SKID_ACCELERATION: f32 = 18.0;
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
/// With great feet, a foot save reaches this much farther, and this high.
const FEET_REACH: f32 = 0.4;
const FEET_HIGH: f32 = 1.5;

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

    /// Moves with a lunge, a leap or recovery commit the body: nothing else can
    /// start until they finish.
    fn committed(&self) -> bool {
        let spec = self.id.spec();
        spec.lunge.is_some() || spec.leap.is_some() || spec.recovery > 0
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
    pub aim: Option<Aim>,
    pub action: Option<Action>,
    /// Hands raised to block, until landing.
    pub(crate) hands_up: bool,
    /// The dash underway, or the last one.
    pub dash: Option<Dash>,
    /// Rising from a jump, which letting go of jump can cut short.
    rising: bool,
    /// Skidding on landing until this tick.
    skid_until: u32,
    /// When each move (by [`MoveId::index`]) can start again.
    ready_at: [u32; MoveId::ALL.len()],
    /// Ultimate charge, from 0 to 1.
    pub charge: f32,
    /// Carrying the ball: shifting sideways until the carry ends.
    pub carry: Option<Carry>,
    /// Knocked down (by a dunk through the block) until this tick.
    pub stunned_until: u32,
    /// The latest movement input, world XZ.
    pub(crate) movement: Vec2,
}

/// The body's shift while carrying the ball.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carry {
    pub until_tick: u32,
    /// World XZ velocity of the shift.
    pub velocity: Vec2,
}

/// A quick burst along the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
    pub start_tick: u32,
    /// World XZ, unit length.
    pub direction: Vec2,
}

/// What a tick of input started.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Started {
    pub(crate) move_id: Option<MoveId>,
    pub(crate) dash: bool,
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
            dash: None,
            rising: false,
            skid_until: 0,
            ready_at: [0; MoveId::ALL.len()],
            charge: 0.0,
            carry: None,
            stunned_until: 0,
            movement: Vec2::ZERO,
        }
    }

    /// Whether `id` is off cooldown (and, for an ultimate, charged) at `tick`.
    pub fn ready(&self, id: MoveId, tick: u32) -> bool {
        tick >= self.ready_at[id.index()] && (!id.spec().ultimate || self.charge >= 1.0)
    }

    /// Seconds until `id` is off cooldown at `tick`.
    pub fn cooldown_left(&self, id: MoveId, tick: u32) -> f32 {
        self.ready_at[id.index()].saturating_sub(tick) as f32 * DT
    }

    pub fn stunned(&self, tick: u32) -> bool {
        tick < self.stunned_until
    }

    pub fn carrying(&self, tick: u32) -> bool {
        self.carry.is_some_and(|carry| tick < carry.until_tick)
    }

    /// How fast a jump leaves the ground right now: faster with a run-up.
    pub fn takeoff_speed(&self) -> f32 {
        let approach = (self.velocity.length() / self.kit.run_speed).min(1.0);
        self.kit.jump_speed * (1.0 + APPROACH_BONUS * approach)
    }

    /// Whether a dash is bursting along at `tick`.
    pub fn dashing(&self, tick: u32) -> bool {
        self.dash.is_some_and(|dash| tick < dash.start_tick + DASH_TICKS)
    }

    /// Whether a dash could start at `tick`: on the ground, off cooldown, and
    /// not in the middle of a committed move.
    pub fn can_dash(&self, tick: u32) -> bool {
        let rested = self.dash.is_none_or(|dash| tick >= dash.start_tick + DASH_COOLDOWN_TICKS);
        rested && self.grounded() && !self.action.is_some_and(|action| action.committed())
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
        self.reaches_scaled(id, ball, 1.0)
    }

    /// Whether the move `id` could touch a ball at `ball`, reaching `scale`
    /// times as far as usual.
    pub fn reaches_scaled(&self, id: MoveId, ball: Vec3, scale: f32) -> bool {
        let spec = id.spec();
        let (mut reach, mut high) = (spec.reach, spec.high);
        // Great feet get a foot to balls well up the body, and farther out.
        if id == MoveId::FootSave && self.kit.has(Passive::Feet) {
            reach += FEET_REACH;
            high = FEET_HIGH;
        }
        let horizontal = Vec2::new(ball.x - self.position.x, ball.z - self.position.z).length();
        let height = ball.y - self.position.y;
        // No reaching across the net into the other half.
        let on_our_side = self.side * ball.x > -court::BALL_RADIUS;
        horizontal <= reach * scale && (spec.low..=high).contains(&height) && on_our_side
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

    /// Back in position for a new rally. Cooldowns and charge carry over.
    pub(crate) fn reset(&mut self, position: Vec3) {
        *self = Player { kit: self.kit, ready_at: self.ready_at, charge: self.charge, ..Self::new(self.team, self.side, position) };
    }

    /// Applies one tick of input. Returns what it started: a move (not one
    /// merely re-pressed), a dash. A server holding the ball must stay behind the end line.
    pub(crate) fn update(&mut self, input: &PlayerInput, tick: u32, serving: bool, ball: Vec3) -> Started {
        if self.action.is_some_and(|action| action.phase(tick).is_none()) {
            self.action = None;
        }
        // Knocked down: no control until back up.
        let idle = PlayerInput::default();
        let input = if self.stunned(tick) { &idle } else { input };
        self.movement = input.movement;
        self.aim = input.aim;

        // Holding pass keeps a waiting pass armed.
        if input.pass_held
            && let Some(action) = self.action.as_mut()
            && action.id.spec().button == Button::Pass
            && !action.spent
            && action.phase(tick) == Some(MovePhase::Active)
        {
            action.start_tick = tick;
        }
        let committed = self.action.is_some_and(|action| action.committed());
        let can_block = !self.kit.has(Passive::NoBlock);
        if input.pass && can_block && !committed && self.position.x.abs() < BLOCK_DISTANCE && self.side * ball.x < 0.0 {
            // Block: hands up, jumping first if still on the ground.
            self.hands_up = true;
            if self.grounded() {
                self.vertical_velocity = self.kit.jump_speed;
            }
        }

        let mut started = Started::default();
        let presses = [
            (input.ultimate, Button::Ultimate),
            (input.ability, Button::Ability),
            (input.dive, Button::Dive),
            (input.kick, Button::Kick),
            (input.spike, Button::Spike),
            (input.pass, Button::Pass),
        ];
        for (pressed, button) in presses {
            if !pressed || committed || (serving && button != Button::Pass) {
                continue;
            }
            let Some(id) = self.kit.move_for(button, self.grounded()).filter(|&id| self.ready(id, tick)) else {
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
                started.move_id = Some(id);
                let spec = id.spec();
                self.ready_at[id.index()] = tick + spec.cooldown;
                if spec.ultimate {
                    self.charge = 0.0;
                }
                if let Some(leap) = spec.leap {
                    self.vertical_velocity = self.takeoff_speed() * leap;
                    self.rising = false;
                }
            }
            break;
        }

        let toward_ball = Vec2::new(ball.x - self.position.x, ball.z - self.position.z);
        if input.dash && !serving && self.can_dash(tick) {
            let direction = [input.movement, toward_ball]
                .into_iter()
                .find(|d| d.length() > 0.1)
                .unwrap_or(Vec2::new(-self.side, 0.0))
                .normalize();
            self.dash = Some(Dash { start_tick: tick, direction });
            started.dash = true;
        }

        let phase = self.action.and_then(|action| action.phase(tick).map(|phase| (action, phase)));
        match phase {
            Some((action, MovePhase::Windup | MovePhase::Active)) if action.id.spec().lunge.is_some() => {
                self.velocity = action.direction * action.id.spec().lunge.unwrap_or_default();
            }
            Some((action, MovePhase::Recovery)) if action.id.spec().recovery > 0 => self.velocity = Vec2::ZERO,
            _ if self.carrying(tick) => self.velocity = self.carry.map(|carry| carry.velocity).unwrap_or_default(),
            _ => {
                let running = input.movement.clamp_length_max(1.0) * self.kit.run_speed;
                let (wanted, acceleration) = match self.steering(tick, ball) {
                    Some(to_spot) => ((to_spot / STEER_SECONDS).clamp_length_max(self.kit.run_speed), STEER_ACCELERATION),
                    None if !self.grounded() => (running, AIR_ACCELERATION),
                    None if tick < self.skid_until => (running, SKID_ACCELERATION),
                    None => (running, ACCELERATION),
                };
                self.velocity = move_towards(self.velocity, wanted, acceleration * DT);
                if let Some(dash) = self.dash.filter(|_| self.dashing(tick)) {
                    self.velocity = dash.direction * self.kit.dash_speed;
                }
                if input.jump && self.grounded() {
                    self.vertical_velocity = self.takeoff_speed();
                    self.rising = true;
                    // Jumping ends a dash's burst but keeps its speed, for a flying approach.
                    if self.dashing(tick) {
                        self.dash = self.dash.map(|dash| Dash { start_tick: tick.saturating_sub(DASH_TICKS), ..dash });
                    }
                }
            }
        }
        if input.jump_released && self.rising && self.vertical_velocity > SHORT_HOP_SPEED {
            self.vertical_velocity = SHORT_HOP_SPEED;
        }
        self.position.x += self.velocity.x * DT;
        self.position.z += self.velocity.y * DT;

        let (min_x, max_x) = if serving {
            // Serving from the back of the arena.
            court::x_range(self.side, HALF_LENGTH - SERVE_ZONE, HALF_LENGTH - BODY_RADIUS)
        } else {
            court::x_range(self.side, BODY_RADIUS, HALF_LENGTH - BODY_RADIUS)
        };
        let max_z = HALF_WIDTH - BODY_RADIUS;
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
            let gravity = if self.carrying(tick) {
                CARRY_GRAVITY
            } else {
                self.action.map_or(1.0, |action| action.id.spec().gravity)
            };
            self.vertical_velocity -= PLAYER_GRAVITY * gravity * DT;
            self.position.y += self.vertical_velocity * DT;
            if self.vertical_velocity <= 0.0 {
                self.rising = false;
            }
            if self.position.y <= 0.0 {
                self.position.y = 0.0;
                self.vertical_velocity = 0.0;
                self.hands_up = false;
                if self.velocity.length() > SKID_SPEED {
                    self.skid_until = tick + SKID_TICKS;
                }
                // Air moves, and moves that leap, last until landing.
                if self.action.is_some_and(|action| action.id.spec().stance == Stance::Air || action.id.spec().leap.is_some()) {
                    self.action = None;
                }
                self.carry = None;
            }
        }
        started
    }

    /// While an armed attack is in the air near the ball on our side, the
    /// horizontal offset to where the body should be to hit it.
    fn steering(&self, tick: u32, ball: Vec3) -> Option<Vec2> {
        let range = self.active_move(tick).filter(|_| !self.grounded()).and_then(|id| id.spec().steer)?;
        let to_spot = attack::steer_position(self, ball) - Vec2::new(self.position.x, self.position.z);
        let near = Vec2::new(ball.x - self.position.x, ball.z - self.position.z).length() < range;
        (near && self.side * ball.x > 0.0).then_some(to_spot)
    }
}

/// `from` moved toward `to` by at most `step`.
fn move_towards(from: Vec2, to: Vec2, step: f32) -> Vec2 {
    let delta = to - from;
    if delta.length() <= step { to } else { from + delta.normalize() * step }
}
