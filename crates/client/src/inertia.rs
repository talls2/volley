//! Inertialization (Gears of War 4; David Bollo, GDC 2018): instead of
//! crossfading from one animation into the next, which plays both at partial
//! weight and smears their poses together, cut to the new animation at once
//! and carry the difference from the old pose as an offset that fades out.
//! The new motion shows from its first frame, there's no pop, and only one
//! animation plays at a time.
//!
//! Off by default: measured against Volley's crossfades it didn't do better
//! (docs/animation/experiments.md, 06 and 07). `VOLLEY_BLEND=inertia` turns
//! it on, for trying it again.

use bevy::app::AnimationSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

pub fn plugin(app: &mut App) {
    let style = if std::env::var("VOLLEY_BLEND").as_deref() == Ok("inertia") { Blending::Inertia } else { Blending::Crossfade };
    app.insert_resource(style)
        .add_systems(PostUpdate, inertialize.in_set(Inertialize).after(AnimationSystems).before(TransformSystems::Propagate));
}

/// Where cuts are blended, right after the animation is applied.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Inertialize;

/// How characters move from one animation into the next.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blending {
    Crossfade,
    Inertia,
}

/// On a character's armature: the bones it carries an offset for, and the
/// offset fading out after a cut.
#[derive(Component, Default)]
pub struct Inertia {
    bones: Vec<Entity>,
    /// Each bone's pose as it was last shown, and as the animation alone made
    /// it: when the animation leaves a bone alone, its transform still holds
    /// what was shown, so the animation's own pose comes from here.
    shown: Vec<(Quat, Vec3)>,
    animated: Vec<(Quat, Vec3)>,
    offset: Vec<(Quat, Vec3)>,
    elapsed: f32,
    duration: f32,
    /// A cut was asked for, fading over this long (seconds): its offset is
    /// taken once the new animation has posed the bones.
    pending: Option<f32>,
}

impl Inertia {
    /// Carries the pose shown now into the next animation, fading out over
    /// `seconds`.
    pub fn cut(&mut self, seconds: f32) {
        self.pending = Some(seconds);
    }

    /// Adds a skeleton (`root` and every node below it) to carry offsets for.
    pub fn track(&mut self, root: Entity, children: &Query<&Children>) {
        self.bones.extend(std::iter::once(root).chain(children.iter_descendants(root)));
        self.shown.clear();
        self.animated.clear();
    }
}

/// Fades a cut's offset out: full at the cut, gone after its duration, with
/// no sudden start or stop. (Fading fast at first, `(1 - x)³`, doubled the
/// frames where hands snap; experiment 07.)
fn fade(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - x * x * (3.0 - 2.0 * x)
}

fn inertialize(time: Res<Time>, mut rigs: Query<&mut Inertia>, mut transforms: Query<&mut Transform>) {
    for mut inertia in &mut rigs {
        let inertia = &mut *inertia;
        let fresh = inertia.shown.len() != inertia.bones.len();
        // What the animation made of each bone this frame.
        let mut now = Vec::with_capacity(inertia.bones.len());
        for (i, &bone) in inertia.bones.iter().enumerate() {
            let Ok(transform) = transforms.get(bone) else {
                now.push((Quat::IDENTITY, Vec3::ZERO));
                continue;
            };
            let current = (transform.rotation, transform.translation);
            let untouched = !fresh && current == inertia.shown[i];
            now.push(if untouched { inertia.animated[i] } else { current });
        }
        if let Some(seconds) = inertia.pending.take()
            && !fresh
        {
            inertia.offset = inertia.shown.iter().zip(&now).map(|(&(rs, ts), &(ra, ta))| (rs * ra.inverse(), ts - ta)).collect();
            inertia.elapsed = 0.0;
            inertia.duration = seconds;
        }
        let weight = if inertia.duration > 0.0 {
            inertia.elapsed += time.delta_secs();
            let w = fade(inertia.elapsed / inertia.duration);
            if inertia.elapsed >= inertia.duration {
                inertia.duration = 0.0;
            }
            w
        } else {
            0.0
        };
        inertia.shown.clear();
        for (i, &bone) in inertia.bones.iter().enumerate() {
            let (rotation, translation) = now[i];
            let (rotation, translation) = if weight > 0.0 && let Some(&(r, t)) = inertia.offset.get(i) {
                (Quat::IDENTITY.slerp(r, weight) * rotation, translation + t * weight)
            } else {
                (rotation, translation)
            };
            if let Ok(mut transform) = transforms.get_mut(bone) {
                transform.rotation = rotation;
                transform.translation = translation;
            }
            inertia.shown.push((rotation, translation));
        }
        inertia.animated = now;
    }
}
