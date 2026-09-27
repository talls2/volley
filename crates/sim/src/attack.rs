//! Attacks in the air. An armed attack hits the ball however the body can
//! reach it: a spike with the hand, a volley kick for a low ball, or a bicycle
//! kick for one behind. Each technique has a sweet spot relative to the body,
//! and the farther the ball is from it, the weaker and wilder the hit: it
//! flies slower and lands farther from where it was aimed.

use glam::{Vec2, Vec3};

use crate::player::{PLAYER_GRAVITY, Player};
use crate::{DT, HitKind, MoveId};

/// The ways an armed attack can hit the ball.
pub const TECHNIQUES: [HitKind; 3] = [HitKind::Spike, HitKind::Volley, HitKind::Bicycle];

/// Where the ball is relative to the body: up from the feet, forward toward the
/// net, and sideways.
#[derive(Clone, Copy, Debug)]
struct Relative {
    up: f32,
    forward: f32,
    sideways: f32,
}

impl Relative {
    fn of(player: &Player, ball: Vec3) -> Self {
        let offset = ball - player.position;
        Self { up: offset.y, forward: -player.side * offset.x, sideways: offset.z }
    }

    fn distance(self, other: Self) -> f32 {
        Vec3::new(self.up - other.up, self.forward - other.forward, self.sideways - other.sideways).length()
    }
}

/// Each technique's sweet spot. A spike meets the ball above and just in front
/// of the head; a volley kick, at hip height out in front; a bicycle kick, over
/// and behind the head as the body flips back.
fn sweet_spot(kind: HitKind) -> Relative {
    match kind {
        HitKind::Volley => Relative { up: 0.9, forward: 0.6, sideways: 0.0 },
        HitKind::Bicycle => Relative { up: 1.7, forward: -0.5, sideways: 0.0 },
        _ => Relative { up: 2.1, forward: 0.3, sideways: 0.0 },
    }
}

/// Within this of the sweet spot is a perfect contact...
const SWEET_RADIUS: f32 = 0.3;
/// ...and this far off is the worst one that still connects.
const WORST_MISS: f32 = 1.4;

/// How cleanly `kind` would meet a ball at `ball`: 1 perfectly, 0 barely.
pub fn quality(player: &Player, ball: Vec3, kind: HitKind) -> f32 {
    let miss = Relative::of(player, ball).distance(sweet_spot(kind));
    (1.0 - (miss - SWEET_RADIUS) / (WORST_MISS - SWEET_RADIUS)).clamp(0.0, 1.0)
}

/// The technique that meets a ball at `ball` best, and how well.
pub fn best_technique(player: &Player, ball: Vec3) -> (HitKind, f32) {
    TECHNIQUES
        .into_iter()
        .map(|kind| (kind, quality(player, ball, kind)))
        // Ties go to the earlier technique: a spike if it's as good as a kick.
        .fold((HitKind::Spike, -1.0), |best, next| if next.1 > best.1 { next } else { best })
}

/// Whether an armed attack `id` should hit a ball at `ball` now, rather than
/// wait for it at `next_ball` a tick later: it waits while the contact is
/// getting better and the ball stays in reach, so it meets the ball at its best.
pub fn hits_now(player: &Player, id: MoveId, ball: Vec3, next_ball: Vec3) -> bool {
    let mut later = *player;
    later.vertical_velocity -= PLAYER_GRAVITY * DT;
    later.position += Vec3::new(later.velocity.x, later.vertical_velocity, later.velocity.y) * DT;
    later.position.y = later.position.y.max(0.0);
    let improving = best_technique(&later, next_ball).1 > best_technique(player, ball).1;
    !(improving && !later.grounded() && later.reaches(id, next_ball))
}

/// Where the body should be, horizontally, to meet a ball at `ball` in the
/// sweet spot of the technique that fits it best right now: under and behind
/// it for a spike or volley, in front of it for a bicycle kick. Armed attacks
/// steer toward it in the air.
pub fn steer_position(player: &Player, ball: Vec3) -> Vec2 {
    let (kind, _) = best_technique(player, ball);
    Vec2::new(ball.x + player.side * sweet_spot(kind).forward, ball.z)
}

/// Seconds an attack of `kind` takes to travel `distance`: slower the worse the
/// contact. Spikes are the fastest, kicks a little slower.
pub fn flight_seconds(kind: HitKind, distance: f32, quality: f32) -> f32 {
    let miss = 1.0 - quality;
    let clean = match kind {
        HitKind::Dunk => 0.22 + 0.016 * distance,
        HitKind::Volley => 0.4 + 0.024 * distance,
        HitKind::Bicycle => 0.35 + 0.022 * distance,
        _ => 0.3 + 0.02 * distance,
    };
    clean * (1.0 + 0.8 * miss)
}

/// How far from the aim an attack can land, in meters. Kicks are never quite
/// as precise as a hand.
pub fn wobble(kind: HitKind, quality: f32) -> f32 {
    let miss = 1.0 - quality;
    let base = match kind {
        HitKind::Volley => 0.8,
        HitKind::Bicycle => 1.2,
        _ => 0.0,
    };
    base + 3.5 * miss
}
