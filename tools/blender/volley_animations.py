"""Authors Volley's volleyball animations and exports them as a glTF library.

The free Quaternius animation library has no volleyball moves, so this script
poses its skeleton into them. Each clip is a list of key poses in character
space: where the wrists and ankles go (solved with IK), how the hips, spine and
head turn, and so on. The poses are interpolated smoothly, baked to plain bone
keys, and exported to `crates/client/assets/animations/Volley.glb`, which the
game loads next to the Quaternius library.

Run from the repository root:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \\
        -P tools/blender/volley_animations.py -- [--preview DIR] [--only NAME,...]

`--preview DIR` also renders a contact sheet per clip into DIR: the clip at
evenly spaced moments, from the side and from the front, with the ball where
the clip expects to meet it. `--only` limits the run to some clips (and skips
the export, so a partial run never replaces the library).
"""

import json
import math
import struct
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Matrix, Quaternion, Vector

REPO = Path(__file__).resolve().parents[2]
ASSETS = REPO / "crates/client/assets"
LIBRARY = ASSETS / "animations/UAL1_Standard.glb"
OUTPUT = ASSETS / "animations/Volley.glb"
FPS = 30


# Character space ------------------------------------------------------------
#
# The skeleton faces -Y in Blender with its right side toward -X. Poses are
# written in character terms instead: right, forward, up, in meters from the
# spot between the feet.


def at(right, forward, up):
    return Vector((-right, -forward, up))


def rot(pitch=0.0, turn=0.0, lean=0.0):
    """A rotation in degrees: `pitch` tips the top forward, `turn` faces left,
    `lean` tips the top to the right. Applied lean, then pitch, then turn."""
    return (
        Quaternion((0, 0, 1), math.radians(turn))
        @ Quaternion((1, 0, 0), math.radians(pitch))
        @ Quaternion((0, -1, 0), math.radians(lean))
    )


# Where things are in the rest pose.
ANKLE_HEIGHT = 0.104
HIP_HEIGHT = 0.917
SHOULDER_HEIGHT = 1.441

# A pose: everything a key sets. Keys list only what differs from the pose
# before them; the first key starts from NEUTRAL.
NEUTRAL = dict(
    # Pelvis offset from rest, and its rotation.
    hips=at(0, 0, 0),
    hips_rot=rot(),
    # The whole spine's bend on top of the hips, spread over its three bones.
    spine=rot(),
    # Neck and head together, on top of the spine.
    head=rot(),
    # Shoulders: shrugging or reaching.
    clavicle_l=rot(),
    clavicle_r=rot(),
    # Wrist positions and elbow directions (points the elbows bend toward).
    hand_l=at(-0.3, 0.05, 0.85),
    hand_r=at(0.3, 0.05, 0.85),
    elbow_l=at(-0.8, -0.6, 1.0),
    elbow_r=at(0.8, -0.6, 1.0),
    # Wrist bends relative to the forearm.
    wrist_l=rot(),
    wrist_r=rot(),
    # Ankle positions, knee directions, and foot rotations.
    foot_l=at(-0.1, 0, ANKLE_HEIGHT),
    foot_r=at(0.1, 0, ANKLE_HEIGHT),
    knee_l=at(-0.15, 2.0, 0.5),
    knee_r=at(0.15, 2.0, 0.5),
    foot_rot_l=rot(),
    foot_rot_r=rot(),
    # Finger curl at each knuckle, in radians: 0 flat, about 1.4 a fist.
    fingers_l=0.4,
    fingers_r=0.4,
)

# Parts that trail the motion driving them, in seconds. Letting the chest follow
# the hips, the head follow the chest, and the wrists and fingers follow the
# arms makes motion flow through the body instead of moving it all at once.
LAG = dict(spine=0.03, head=0.07, clavicle_l=0.03, clavicle_r=0.03, wrist_l=0.05, wrist_r=0.05, fingers_l=0.06, fingers_r=0.06)


def mirror(pose):
    """The same pose with left and right swapped."""
    flip = lambda v: Vector((-v.x, v.y, v.z))
    flip_rot = lambda q: Quaternion((q.w, q.x, -q.y, -q.z))
    out = {}
    for name, value in pose.items():
        other = name[:-1] + ("r" if name.endswith("_l") else "l") if name[-2:] in ("_l", "_r") else name
        if isinstance(value, Vector):
            out[other] = flip(value)
        elif isinstance(value, Quaternion):
            out[other] = flip_rot(value)
        else:
            out[other] = value
    return out


# Clips ------------------------------------------------------------------------
#
# Each clip: key poses at times in seconds, whether it loops, and, for hits,
# where and when the ball is met (for the preview, and for the game's timing).


def wrist_back(degrees, side):
    """Bends a hand back (fingers up, as for a set) at the wrist."""
    return Quaternion((0, 1, 0), math.radians(-degrees if side == "l" else degrees))


def ready_pose(dip=0.0):
    """Volleyball ready stance: feet wide, knees bent, weight forward, forearms
    out in front. `dip` sinks lower."""
    drop = 0.2 + dip
    return dict(
        hips=at(0, 0.04, -drop),
        hips_rot=rot(pitch=14),
        spine=rot(pitch=12),
        head=rot(pitch=-20),
        hand_l=at(-0.26, 0.5, 0.92 - dip),
        hand_r=at(0.26, 0.5, 0.92 - dip),
        elbow_l=at(-0.5, -0.2, 0.2),
        elbow_r=at(0.5, -0.2, 0.2),
        wrist_l=rot(),
        wrist_r=rot(),
        foot_l=at(-0.27, 0.02, ANKLE_HEIGHT),
        foot_r=at(0.27, 0.02, ANKLE_HEIGHT),
        knee_l=at(-0.4, 2.0, 0.5),
        knee_r=at(0.4, 2.0, 0.5),
        foot_rot_l=rot(turn=12),
        foot_rot_r=rot(turn=-12),
        fingers_l=0.45,
        fingers_r=0.45,
    )


def ready_sway(side, dip=0.0):
    """The ready stance with the weight shifted toward one foot (+1 right, -1
    left), the chest turning a little against it."""
    pose = ready_pose(dip)
    pose["hips"] = pose["hips"] + at(0.035 * side, 0, 0)
    pose["hips_rot"] = rot(pitch=14, lean=-3 * side, turn=2 * side)
    pose["spine"] = rot(pitch=12, lean=2 * side, turn=-3 * side)
    return pose


def bump_platform(height, reach):
    """Both arms straight and together in front, wrists at `height`."""
    return dict(
        hand_l=at(-0.03, reach, height),
        hand_r=at(0.03, reach, height),
        elbow_l=at(-0.3, 0.2, height - 0.8),
        elbow_r=at(0.3, 0.2, height - 0.8),
        clavicle_l=rot(turn=-10, pitch=8),
        clavicle_r=rot(turn=10, pitch=8),
        # Flat hands, one cupped over the other.
        fingers_l=0.15,
        fingers_r=0.3,
    )


def set_hands(height, reach=0.22):
    """Hands above the forehead, fingers up, making a window for the ball."""
    return dict(
        hand_l=at(-0.13, reach, height),
        hand_r=at(0.13, reach, height),
        elbow_l=at(-1.0, 0.3, height - 0.3),
        elbow_r=at(1.0, 0.3, height - 0.3),
        wrist_l=wrist_back(55, "l"),
        wrist_r=wrist_back(55, "r"),
        # Soft, open fingers shaped around the ball.
        fingers_l=0.3,
        fingers_r=0.3,
    )


def airborne(tuck=0.0):
    """In the air: legs hanging a little bent, knees tucked by `tuck` (0 to 1)."""
    return dict(
        hips=at(0, 0, 0),
        hips_rot=rot(),
        foot_l=at(-0.12, -0.05 + 0.15 * tuck, 0.2 + 0.25 * tuck),
        foot_r=at(0.12, -0.12 + 0.15 * tuck, 0.17 + 0.25 * tuck),
        knee_l=at(-0.2, 2.0, 0.6),
        knee_r=at(0.2, 2.0, 0.6),
        foot_rot_l=rot(pitch=35),
        foot_rot_r=rot(pitch=35),
    )


def arms_down():
    return dict(
        hand_l=at(-0.3, 0.05, 0.85),
        hand_r=at(0.3, 0.05, 0.85),
        elbow_l=at(-0.8, -0.6, 1.0),
        elbow_r=at(0.8, -0.6, 1.0),
        wrist_l=rot(),
        wrist_r=rot(),
        clavicle_l=rot(),
        clavicle_r=rot(),
        fingers_l=0.45,
        fingers_r=0.45,
    )


def upright():
    return dict(hips=at(0, 0, 0), hips_rot=rot(), spine=rot(), head=rot())


SPIKE_BALL = at(0.12, 0.35, 2.12)

CLIPS = [
    dict(
        name="Ready_Loop",
        loop=True,
        keys=[
            (0.0, ready_sway(1)),
            (0.4, ready_sway(0, dip=0.04)),
            (0.8, ready_sway(-1)),
            (1.2, ready_sway(0, dip=0.04)),
            (1.6, ready_sway(1)),
        ],
    ),
    dict(
        name="Bump",
        # Holds at the end of the wind-up until the ball arrives; the ball is
        # met at `contact`.
        wind_up=0.15,
        contact=0.25,
        ball=at(0, 0.45, 0.8),
        keys=[
            (0.0, ready_pose()),
            # Sink, platform formed low.
            (0.15, {**ready_pose(dip=0.12), **bump_platform(0.66, 0.5), "spine": rot(pitch=24), "head": rot(pitch=-22)}),
            # Legs drive up through the ball; the platform barely moves.
            (0.25, {**ready_pose(dip=0.05), **bump_platform(0.7, 0.52), "spine": rot(pitch=18), "head": rot(pitch=-18)}),
            (0.4, {**ready_pose(dip=-0.1), **bump_platform(0.88, 0.5), "spine": rot(pitch=8), "head": rot(pitch=-8)}),
            (0.7, ready_pose()),
        ],
    ),
    dict(
        name="Set",
        wind_up=0.15,
        contact=0.25,
        ball=at(0, 0.3, 1.78),
        keys=[
            (0.0, ready_pose()),
            # Under the ball, knees bent, hands up at the forehead.
            (0.15, {**ready_pose(dip=0.1), **set_hands(1.5), "hips_rot": rot(pitch=2), "spine": rot(pitch=-4), "head": rot(pitch=-30)}),
            # Push up through the legs and arms together.
            (0.25, {**ready_pose(dip=0.0), **set_hands(1.72, 0.25), "hips_rot": rot(pitch=0), "spine": rot(pitch=-6), "head": rot(pitch=-28)}),
            (0.38, {**ready_pose(dip=-0.16), **set_hands(1.95, 0.3), "hips_rot": rot(pitch=0), "spine": rot(pitch=-4), "head": rot(pitch=-20), "wrist_l": wrist_back(10, "l"), "wrist_r": wrist_back(10, "r")}),
            (0.7, ready_pose()),
        ],
    ),
    dict(
        name="Spike",
        # In the air, the game lifts the whole body; this is the swing.
        wind_up=0.24,
        contact=0.32,
        ball=SPIKE_BALL,
        keys=[
            (0.0, {**airborne(0.2), **arms_down()}),
            # Both arms swing up.
            (0.12, {**airborne(0.5), "spine": rot(pitch=-6), "hand_l": at(-0.2, 0.3, 1.8), "hand_r": at(0.3, 0.1, 1.85),
                    "elbow_l": at(-0.6, 0.5, 1.4), "elbow_r": at(0.9, -0.3, 1.8)}),
            # Bow and arrow: hitting elbow high and back, the other arm pointing at the ball.
            (0.24, {**airborne(0.8), "spine": rot(pitch=-16, turn=-28), "head": rot(pitch=-20, turn=20),
                    "hand_l": at(-0.1, 0.45, 1.95), "elbow_l": at(-0.5, 0.2, 1.5),
                    "hand_r": at(0.3, -0.22, 1.72), "elbow_r": at(0.7, -0.4, 2.3),
                    "wrist_r": wrist_back(40, "r"), "clavicle_r": rot(pitch=-10, lean=-10), "fingers_r": 0.2, "fingers_l": 0.2}),
            # Whip: arm straight up and in front, snapping over the ball.
            (0.32, {**airborne(0.4), "spine": rot(pitch=6, turn=12), "head": rot(pitch=-18),
                    "hand_l": at(-0.3, 0.3, 1.05), "elbow_l": at(-0.8, -0.3, 1.2),
                    "hand_r": at(0.15, 0.3, 2.02), "elbow_r": at(0.8, -0.2, 1.8),
                    "wrist_r": rot(), "clavicle_r": rot(lean=-12), "fingers_r": 0.1}),
            # Follow through down across the body, jackknifing forward.
            (0.46, {**airborne(0.7), "hips_rot": rot(pitch=10), "spine": rot(pitch=24, turn=22), "head": rot(pitch=-10),
                    "hand_r": at(-0.15, 0.45, 0.95), "elbow_r": at(0.6, 0.3, 0.9),
                    "hand_l": at(-0.35, 0.0, 0.9), "clavicle_r": rot(), "fingers_r": 0.4, "fingers_l": 0.45}),
            (0.75, {**airborne(0.1), **arms_down(), **upright()}),
        ],
    ),
    dict(
        name="Block",
        keys=[
            (0.0, {**ready_pose()}),
            # Arms shoot up as the legs push off.
            (0.12, {**airborne(0.0), "spine": rot(pitch=-2), "head": rot(pitch=-10),
                    "hand_l": at(-0.32, 0.4, 1.65), "hand_r": at(0.32, 0.4, 1.65),
                    "elbow_l": at(-0.9, 0.0, 1.2), "elbow_r": at(0.9, 0.0, 1.2), "fingers_l": 0.15, "fingers_r": 0.15}),
            # Hands high and over the net, fingers spread wide, shoulders pushed up.
            (0.24, {**airborne(0.1), "spine": rot(pitch=4), "head": rot(pitch=-12),
                    "hand_l": at(-0.2, 0.3, 1.98), "hand_r": at(0.2, 0.3, 1.98),
                    "elbow_l": at(-0.8, -0.5, 1.6), "elbow_r": at(0.8, -0.5, 1.6),
                    "wrist_l": wrist_back(-15, "l"), "wrist_r": wrist_back(-15, "r"),
                    "clavicle_l": rot(lean=15), "clavicle_r": rot(lean=-15), "fingers_l": 0.05, "fingers_r": 0.05}),
        ],
    ),
    dict(
        name="Serve",
        # Held at the start while waiting to serve: ball up in the left hand.
        wind_up=0.0,
        contact=0.22,
        ball=at(0.05, 0.35, 1.9),
        keys=[
            (0.0, {**upright(), "spine": rot(pitch=-4, turn=-20), "head": rot(pitch=-12, turn=18),
                   "hand_l": at(-0.02, 0.33, 1.72), "elbow_l": at(-0.6, 0.3, 1.2), "wrist_l": wrist_back(40, "l"),
                   "hand_r": at(0.32, -0.2, 1.62), "elbow_r": at(0.7, -0.4, 2.2), "wrist_r": wrist_back(30, "r"),
                   "foot_l": at(-0.12, 0.2, ANKLE_HEIGHT), "foot_r": at(0.18, -0.2, ANKLE_HEIGHT), "foot_rot_r": rot(turn=-25),
                   "fingers_l": 0.55, "fingers_r": 0.2}),
            (0.12, {"spine": rot(pitch=-8, turn=-26), "hand_l": at(-0.1, 0.35, 1.6), "hand_r": at(0.32, -0.28, 1.7)}),
            # Strike: arm straight up, weight onto the front foot.
            (0.22, {"spine": rot(pitch=6, turn=10), "head": rot(pitch=-14),
                    "hand_r": at(0.1, 0.32, 1.98), "elbow_r": at(0.8, -0.2, 1.7), "wrist_r": rot(),
                    "hand_l": at(-0.3, 0.2, 1.0), "elbow_l": at(-0.8, -0.4, 1.2), "hips": at(0, 0.08, -0.03)}),
            (0.4, {"spine": rot(pitch=16, turn=18), "hand_r": at(-0.15, 0.4, 1.0), "elbow_r": at(0.6, 0.3, 1.0),
                   "hand_l": at(-0.3, 0.0, 0.9), "hips": at(0, 0.12, -0.06), "head": rot(pitch=-8)}),
            (0.8, ready_pose()),
        ],
    ),
    dict(
        name="Dive",
        keys=[
            (0.0, ready_pose(dip=0.05)),
            # Launch: body tips forward and low, arms reaching.
            (0.1, {**ready_pose(dip=0.3), "hips_rot": rot(pitch=45), "spine": rot(pitch=15), "head": rot(pitch=-40),
                   "hand_l": at(-0.2, 0.9, 0.7), "hand_r": at(0.2, 0.9, 0.7),
                   "elbow_l": at(-0.6, 0.5, 2.0), "elbow_r": at(0.6, 0.5, 2.0),
                   "foot_r": at(0.2, -0.4, 0.2)}),
            # Flat out, arms stretched ahead, sliding on the chest.
            (0.25, {"hips": at(0, 0.05, -0.62), "hips_rot": rot(pitch=84), "spine": rot(pitch=4), "head": rot(pitch=-65),
                    "fingers_l": 0.15, "fingers_r": 0.15,
                    "hand_l": at(-0.2, 1.25, 0.28), "hand_r": at(0.2, 1.25, 0.28),
                    "elbow_l": at(-0.6, 0.8, 2.0), "elbow_r": at(0.6, 0.8, 2.0),
                    "foot_l": at(-0.2, -0.75, 0.2), "foot_r": at(0.2, -0.8, 0.28),
                    "knee_l": at(-0.3, -0.4, -1.0), "knee_r": at(0.3, -0.4, -1.0),
                    "foot_rot_l": rot(pitch=100), "foot_rot_r": rot(pitch=100)}),
            (0.45, {"hand_l": at(-0.25, 1.2, 0.12), "hand_r": at(0.25, 1.2, 0.12), "head": rot(pitch=-55)}),
            # Push up: hands under the shoulders, knees in.
            (0.65, {"hips": at(0, 0.0, -0.45), "hips_rot": rot(pitch=60), "spine": rot(pitch=10), "head": rot(pitch=-40),
                    "hand_l": at(-0.28, 0.55, 0.1), "hand_r": at(0.28, 0.55, 0.1),
                    "elbow_l": at(-0.8, 0.0, 0.8), "elbow_r": at(0.8, 0.0, 0.8),
                    "foot_l": at(-0.25, -0.35, 0.12), "foot_r": at(0.25, -0.3, 0.12),
                    "knee_l": at(-0.4, 2.0, 0.3), "knee_r": at(0.4, 2.0, 0.3),
                    "foot_rot_l": rot(pitch=40), "foot_rot_r": rot(pitch=40)}),
            (0.9, ready_pose()),
        ],
    ),
    dict(
        name="Foot_Save",
        keys=[
            (0.0, ready_pose()),
            # Drop onto the left leg and shoot the right one out, low.
            (0.08, {**ready_pose(dip=0.18), "hips": at(0.05, -0.05, -0.36), "hips_rot": rot(pitch=-5, turn=10, lean=-8),
                    "spine": rot(pitch=18, lean=10), "head": rot(pitch=-30),
                    "foot_l": at(-0.2, -0.15, ANKLE_HEIGHT), "knee_l": at(-0.4, 2.0, 0.6),
                    "foot_r": at(0.45, 0.85, 0.18), "knee_r": at(0.6, 0.6, 2.0), "foot_rot_r": rot(pitch=-30, turn=-40),
                    "hand_l": at(-0.65, 0.2, 1.0), "hand_r": at(0.55, -0.2, 0.95),
                    "elbow_l": at(-1.0, -0.4, 1.0), "elbow_r": at(1.0, -0.4, 1.0)}),
            (0.2, {"foot_r": at(0.5, 0.9, 0.2)}),
            (0.45, ready_pose()),
        ],
    ),
    dict(
        name="Volley_Kick",
        wind_up=0.1,
        contact=0.18,
        ball=at(0.12, 0.62, 0.95),
        keys=[
            (0.0, {**airborne(0.3), **arms_down()}),
            # Kicking leg cocked back, arms out for balance.
            (0.1, {"spine": rot(pitch=-4, turn=-15), "foot_r": at(0.15, -0.35, 0.55), "knee_r": at(0.3, 2.0, 0.3),
                   "foot_rot_r": rot(pitch=45),
                   "hand_l": at(-0.6, 0.25, 1.25), "hand_r": at(0.55, -0.25, 1.2),
                   "elbow_l": at(-1.0, -0.4, 1.4), "elbow_r": at(1.0, -0.4, 1.4)}),
            # Strike: leg straight through the ball at hip height, body leaning back.
            (0.18, {"hips_rot": rot(pitch=-18), "spine": rot(pitch=-6, turn=12), "head": rot(pitch=20),
                    "foot_r": at(0.1, 0.72, 0.82), "knee_r": at(0.2, 1.0, 2.0), "foot_rot_r": rot(pitch=20),
                    "foot_l": at(-0.12, -0.15, 0.25)}),
            (0.32, {"hips_rot": rot(pitch=-24), "foot_r": at(0.0, 0.55, 1.1)}),
            (0.55, {**airborne(0.2), **arms_down(), **upright()}),
        ],
    ),
    dict(
        name="Bicycle_Kick",
        wind_up=0.12,
        contact=0.24,
        ball=at(0.08, -0.4, 1.75),
        keys=[
            (0.0, {**airborne(0.3), **arms_down()}),
            # Tipping back, the other leg swings up first (the scissor).
            (0.12, {"hips_rot": rot(pitch=-45), "spine": rot(pitch=10), "head": rot(pitch=30),
                    "foot_l": at(-0.12, 0.35, 1.2), "knee_l": at(-0.2, 2.0, 1.5),
                    "foot_r": at(0.12, 0.25, 0.4), "knee_r": at(0.2, 2.0, 1.0),
                    "hand_l": at(-0.6, -0.1, 1.2), "hand_r": at(0.6, -0.1, 1.2),
                    "elbow_l": at(-1.0, 0.3, 1.5), "elbow_r": at(1.0, 0.3, 1.5)}),
            # Flat on the back in the air; the kicking leg snaps over the head.
            (0.24, {"hips_rot": rot(pitch=-105), "spine": rot(pitch=15), "head": rot(pitch=45),
                    "foot_r": at(0.1, -0.3, 1.72), "knee_r": at(0.2, 0.5, 2.5), "foot_rot_r": rot(pitch=-150),
                    "foot_l": at(-0.12, 0.45, 0.55), "knee_l": at(-0.2, 1.0, 1.5),
                    "hand_l": at(-0.65, 0.3, 0.9), "hand_r": at(0.65, 0.3, 0.9)}),
            (0.42, {"hips_rot": rot(pitch=-80), "foot_r": at(0.12, 0.2, 1.5), "foot_l": at(-0.12, 0.4, 0.5)}),
            (0.75, {**airborne(0.2), **arms_down(), **upright()}),
        ],
    ),
    dict(
        name="Cheer_Loop",
        loop=True,
        keys=[
            (0.0, {**upright(), "hand_l": at(-0.45, 0.1, 2.0), "hand_r": at(0.45, 0.1, 2.0),
                   "elbow_l": at(-1.0, -0.3, 1.5), "elbow_r": at(1.0, -0.3, 1.5), "head": rot(pitch=-15),
                   "fingers_l": 1.4, "fingers_r": 1.4}),
            (0.3, {"hips": at(0, 0, -0.12), "hand_l": at(-0.4, 0.2, 1.7), "hand_r": at(0.4, 0.2, 1.7),
                   "head": rot(pitch=5)}),
            (0.6, {"hips": at(0, 0, 0), "hand_l": at(-0.45, 0.1, 2.0), "hand_r": at(0.45, 0.1, 2.0),
                   "head": rot(pitch=-15)}),
        ],
    ),
]


# Interpolation ------------------------------------------------------------------


def log_q(q):
    """A rotation as axis times angle."""
    q = q.normalized()
    if q.w < 0:
        q = -q
    axis, angle = q.to_axis_angle()
    return Vector(axis) * angle


def exp_q(v):
    angle = v.length
    return Quaternion(v.normalized(), angle) if angle > 1e-8 else Quaternion()


def full_keys(clip):
    """Keys with every pose field filled in from the one before."""
    pose = dict(NEUTRAL)
    keys = []
    for time, changes in clip["keys"]:
        pose = {**pose, **changes}
        keys.append((time, dict(pose)))
    return keys


def sample(keys, time, loop):
    """The pose at `time`, with trailing parts sampled a little earlier."""
    length = keys[-1][0]
    pose = {}
    for name in keys[0][1]:
        at_time = time - LAG.get(name, 0.0)
        at_time = at_time % length if loop else max(at_time, 0.0)
        pose[name] = sample_field(keys, name, at_time, loop)
    return pose


def sample_field(keys, name, time, loop):
    """One field at `time`: a smooth curve through the keys (cubic Hermite with
    Catmull-Rom tangents, easing at the ends of one-off clips)."""
    times = [t for t, _ in keys]
    if time <= times[0]:
        return keys[0][1][name]
    if time >= times[-1]:
        return keys[-1][1][name]
    i = max(j for j in range(len(times) - 1) if times[j] <= time)
    t0, t1 = times[i], times[i + 1]
    s = (time - t0) / (t1 - t0)
    h00, h10, h01, h11 = 2 * s**3 - 3 * s**2 + 1, s**3 - 2 * s**2 + s, -2 * s**3 + 3 * s**2, s**3 - s**2

    def tangent(j, values):
        if 0 < j < len(keys) - 1:
            return (values[j + 1] - values[j - 1]) / (times[j + 1] - times[j - 1])
        if loop and len(keys) > 2:
            # The ends are the same pose: use the slope through the wrap.
            before, after = values[-2], values[1]
            span = (times[-1] - times[-2]) + (times[1] - times[0])
            return (after - before) / span
        return values[j] * 0.0

    value = keys[0][1][name]
    if isinstance(value, Quaternion):
        values = [log_q(k[name]) for _, k in keys]
        wrap = exp_q
    else:
        values = [k[name] for _, k in keys]
        wrap = lambda v: v
    m0 = tangent(i, values) * (t1 - t0)
    m1 = tangent(i + 1, values) * (t1 - t0)
    return wrap(h00 * values[i] + h10 * m0 + h01 * values[i + 1] + h11 * m1)


# The rig ------------------------------------------------------------------------

SPINE = ["spine_01", "spine_02", "spine_03"]
FINGERS = ["index", "middle", "ring", "pinky"]
# In the rest pose the palms face down.
PALM = Vector((0, 0, -1))
LIMBS = {
    # IK bone, pole field, target field
    "lowerarm_l": ("elbow_l", "hand_l"),
    "lowerarm_r": ("elbow_r", "hand_r"),
    "calf_l": ("knee_l", "foot_l"),
    "calf_r": ("knee_r", "foot_r"),
}


class Rig:
    def __init__(self):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=str(LIBRARY))
        self.arm = bpy.data.objects["Armature"]
        self.arm.animation_data_clear()
        for action in list(bpy.data.actions):
            bpy.data.actions.remove(action)
        for ob in list(bpy.data.objects):
            if ob.type == "MESH" and ob.parent != self.arm:
                bpy.data.objects.remove(ob)
        scene = bpy.context.scene
        scene.render.fps = FPS
        self.bones = self.arm.pose.bones
        self.rest = {b.name: b.matrix_local.to_quaternion() for b in self.arm.data.bones}
        self.targets = {}
        for bone, (pole, target) in LIMBS.items():
            ik = self.bones[bone].constraints.new("IK")
            ik.target = self.empty(target)
            ik.pole_target = self.empty(pole)
            ik.chain_count = 2
            ik.use_tail = True
            ik.pole_angle = 0.0
            self.targets[target] = ik.target
            self.targets[pole] = ik.pole_target
        # Feet keep the rotation they're given, whatever the legs do.
        for side in "lr":
            copy = self.bones[f"foot_{side}"].constraints.new("COPY_ROTATION")
            copy.target = self.empty(f"foot_rot_{side}")
            self.targets[f"foot_rot_{side}"] = copy.target
        self.calibrate_poles()

    def empty(self, name):
        ob = bpy.data.objects.new(name, None)
        bpy.context.scene.collection.objects.link(ob)
        ob.rotation_mode = "QUATERNION"
        return ob

    def calibrate_poles(self):
        """Finds each limb's pole angle: the one that bends the joint toward its pole."""
        for bone, (pole, target) in LIMBS.items():
            ik = self.bones[bone].constraints["IK"]
            parent = self.bones[bone].parent
            root = parent.head.copy()
            # Bend the limb halfway, pole off to one side.
            reach = (Vector(self.bones[bone].tail) - root).length
            direction = Vector((0, -1, 0)) if bone.startswith("lower") else Vector((0, 0, -1))
            side = Vector((0, 0, 1)) if bone.startswith("lower") else Vector((0, -1, 0))
            self.targets[target].location = root + direction * reach * 0.6
            self.targets[pole].location = root + direction * reach * 0.3 + side * 1.0
            best = None
            for angle in (0, 90, -90, 180):
                ik.pole_angle = math.radians(angle)
                bpy.context.view_layer.update()
                joint = Vector(self.bones[bone].head)
                wanted = (self.targets[pole].location - root).normalized()
                error = (joint - root).normalized().dot(wanted)
                if best is None or error > best[0]:
                    best = (error, angle)
            ik.pole_angle = math.radians(best[1])
            print(f"pole angle {bone}: {best[1]}")

    def pose(self, pose):
        """Poses the rig: FK bones directly, limbs through their IK targets."""
        for name in ("hand_l", "hand_r", "elbow_l", "elbow_r", "foot_l", "foot_r", "knee_l", "knee_r"):
            self.targets[name].location = pose[name]
        for side in "lr":
            self.targets[f"foot_rot_{side}"].rotation_quaternion = pose[f"foot_rot_{side}"] @ self.rest[f"foot_{side}"]

        # World rotations of the FK bones, as changes from rest.
        hips = pose["hips_rot"]
        world = {"root": Quaternion(), "pelvis": hips}
        for i, bone in enumerate(SPINE):
            world[bone] = hips @ exp_q(log_q(pose["spine"]) * ((i + 1) / len(SPINE)))
        chest = world[SPINE[-1]]
        world["neck_01"] = chest @ exp_q(log_q(pose["head"]) * 0.5)
        world["Head"] = chest @ pose["head"]
        for side in "lr":
            world[f"clavicle_{side}"] = chest @ pose[f"clavicle_{side}"]

        for name, change in world.items():
            bone = self.bones[name]
            parent_change = world.get(bone.parent.name, Quaternion()) if bone.parent else Quaternion()
            rest = self.rest[name]
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = rest.inverted() @ parent_change.inverted() @ change @ rest
        # The pelvis moves in its own rest frame.
        self.bones["pelvis"].location = self.rest["pelvis"].inverted() @ pose["hips"]
        # Wrists bend relative to the forearm.
        for side in "lr":
            bone = self.bones[f"hand_{side}"]
            bone.rotation_mode = "QUATERNION"
            rest = self.rest[bone.name]
            bone.rotation_quaternion = rest.inverted() @ pose[f"wrist_{side}"] @ rest
            self.curl_fingers(side, pose[f"fingers_{side}"])
        bpy.context.view_layer.update()

    def curl_fingers(self, side, curl):
        """Bends every knuckle toward the palm by `curl` radians, and the thumb
        across toward the middle knuckle, a little less."""
        bones = self.arm.data.bones
        across = (bones[f"middle_01_{side}"].head_local - bones[f"thumb_01_{side}"].head_local).normalized()
        for finger in FINGERS + ["thumb"]:
            amount = curl * (0.7 if finger == "thumb" else 1.0)
            for joint in (1, 2, 3):
                name = f"{finger}_0{joint}_{side}"
                bone = bones[name]
                along = (bone.tail_local - bone.head_local).normalized()
                axis = along.cross(across if finger == "thumb" else PALM)
                if axis.length < 1e-4:
                    continue
                rest = self.rest[name]
                pose_bone = self.bones[name]
                pose_bone.rotation_mode = "QUATERNION"
                pose_bone.rotation_quaternion = rest.inverted() @ Quaternion(axis.normalized(), amount) @ rest

    def visual_basis(self):
        """Every bone's local transform as posed, constraints included."""
        out = {}
        for bone in self.bones:
            m = self.arm.convert_space(pose_bone=bone, matrix=bone.matrix, from_space="POSE", to_space="LOCAL")
            out[bone.name] = (m.to_translation(), m.to_quaternion())
        return out

    def set_constraints(self, enabled):
        for bone in self.bones:
            for constraint in bone.constraints:
                constraint.enabled = enabled

    def reset(self):
        for bone in self.bones:
            bone.location = (0, 0, 0)
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = Quaternion()
            bone.scale = (1, 1, 1)

    def bake(self, clip):
        """Samples the clip every frame and keys the result as a new action."""
        keys = full_keys(clip)
        loop = clip.get("loop", False)
        length = keys[-1][0]
        frames = round(length * FPS)
        self.set_constraints(True)
        samples = []
        for frame in range(frames + 1):
            self.pose(sample(keys, frame / FPS, loop))
            samples.append(self.visual_basis())

        self.set_constraints(False)
        self.reset()
        action = bpy.data.actions.new(clip["name"])
        action.use_fake_user = True
        self.arm.animation_data_create()
        self.arm.animation_data.action = action
        previous = {}
        for frame, basis in enumerate(samples):
            for name, (location, rotation) in basis.items():
                if name in previous and previous[name].dot(rotation) < 0:
                    rotation = -rotation
                previous[name] = rotation
                bone = self.bones[name]
                bone.location = location
                bone.rotation_quaternion = rotation
                bone.keyframe_insert("location", frame=frame, group=name)
                bone.keyframe_insert("rotation_quaternion", frame=frame, group=name)
        self.arm.animation_data.action = None
        self.reset()
        return action


# Preview ------------------------------------------------------------------------


def render_contact_sheet(rig, clip, action, directory):
    """Renders the clip at six moments, side view above front view, into one image."""
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "OBJECT"
    scene.render.resolution_x, scene.render.resolution_y = 300, 420
    scene.render.image_settings.file_format = "PNG"

    if "PreviewFloor" not in bpy.data.objects:
        bpy.ops.mesh.primitive_plane_add(size=6)
        bpy.context.object.name = "PreviewFloor"
        bpy.context.object.color = (0.8, 0.7, 0.5, 1)
        bpy.ops.mesh.primitive_uv_sphere_add(radius=0.13)
        bpy.context.object.name = "PreviewBall"
        bpy.context.object.color = (1.0, 0.85, 0.2, 1)
        camera = bpy.data.objects.new("PreviewCamera", bpy.data.cameras.new("PreviewCamera"))
        scene.collection.objects.link(camera)
        camera.data.type = "ORTHO"
        camera.data.ortho_scale = 3.0
        scene.camera = camera
    ball = bpy.data.objects["PreviewBall"]
    camera = bpy.data.objects["PreviewCamera"]
    ball.location = clip.get("ball", Vector((0, 0, -5)))

    rig.arm.animation_data_create()
    rig.arm.animation_data.action = action
    rig.set_constraints(False)
    length = full_keys(clip)[-1][0]
    moments = [length * i / 5 for i in range(6)]
    # From the character's right side, then from the front and a little to its left.
    views = [at(6, 0.4, 1.6), at(-2.5, 6, 2.0)]
    rows = []
    for location in views:
        camera.location = location
        camera.rotation_mode = "QUATERNION"
        camera.rotation_quaternion = (at(0, 0.4, 1.0) - location).to_track_quat("-Z", "Y")
        tiles = []
        for moment in moments:
            scene.frame_set(round(moment * FPS))
            ball.hide_render = "contact" in clip and abs(moment - clip["contact"]) > 0.12
            path = str(Path(directory) / "_tile.png")
            scene.render.filepath = path
            bpy.ops.render.render(write_still=True)
            image = bpy.data.images.load(path)
            pixels = np.array(image.pixels[:]).reshape(image.size[1], image.size[0], 4)
            bpy.data.images.remove(image)
            tiles.append(pixels)
        rows.append(np.concatenate(tiles, axis=1))
    # Image rows run bottom-up, so the first view goes last.
    sheet = np.concatenate(rows[::-1], axis=0)
    out = bpy.data.images.new(clip["name"], sheet.shape[1], sheet.shape[0], alpha=True)
    out.pixels = sheet.ravel()
    out.filepath_raw = str(Path(directory) / f"{clip['name']}.png")
    out.file_format = "PNG"
    out.save()
    rig.arm.animation_data.action = None
    print(f"preview {out.filepath_raw}")


# Export -----------------------------------------------------------------------------


def export(rig):
    bpy.ops.object.select_all(action="DESELECT")
    rig.arm.select_set(True)
    bpy.context.view_layer.objects.active = rig.arm
    rig.set_constraints(False)
    bpy.ops.export_scene.gltf(
        filepath=str(OUTPUT),
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_frame_range=False,
        export_def_bones=False,
    )
    check_rest_pose()


def read_gltf_json(path):
    data = path.read_bytes()
    if data[:4] == b"glTF":
        length = struct.unpack_from("<I", data, 12)[0]
        return json.loads(data[20 : 20 + length])
    return json.loads(data)


def check_rest_pose():
    """The exported skeleton must match the Quaternius library's exactly, so our
    clips and theirs pose the same bones the same way. (The library's bone
    lengths differ a little from the character models', but its clips key
    translations too, so in game every character takes the library's
    proportions, and so do ours.)"""
    exported = {n["name"]: n for n in read_gltf_json(OUTPUT)["nodes"]}
    worst = 0.0
    for node in read_gltf_json(LIBRARY)["nodes"]:
        other = exported.get(node["name"])
        if other is None or "mesh" in node:
            continue
        for field, default in (("translation", [0, 0, 0]), ("rotation", [0, 0, 0, 1])):
            a, b = node.get(field, default), other.get(field, default)
            if field == "rotation" and sum(x * y for x, y in zip(a, b)) < 0:
                b = [-x for x in b]
            worst = max(worst, max(abs(x - y) for x, y in zip(a, b)))
    print(f"rest pose difference from the library: {worst:.5f}")
    assert worst < 1e-3, "exported skeleton differs from the library's"


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    preview = args[args.index("--preview") + 1] if "--preview" in args else None
    only = args[args.index("--only") + 1].split(",") if "--only" in args else None
    rig = Rig()
    for clip in CLIPS:
        if only and clip["name"] not in only:
            continue
        action = rig.bake(clip)
        print(f"baked {clip['name']}")
        if preview:
            Path(preview).mkdir(parents=True, exist_ok=True)
            render_contact_sheet(rig, clip, action, preview)
    if only is None:
        export(rig)


main()
