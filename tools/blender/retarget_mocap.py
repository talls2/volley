"""Retargets motion capture onto Volley's skeleton: Mixamo FBX downloads,
100STYLE BVH files (movement styles), CMU captures (converted to BVH by
`tools/mocap/asf_amc_to_bvh.py`) and DeepMotion BVH exports (motion tracked
from volleyball videos).

For every frame, each of our bones turns in the world the way its
counterpart in the capture turned from its rest pose. Captures that don't
rest in the same T-pose as ours (and may face another way) are lined up
first: the whole capture is turned to face like ours, and each bone's rest
direction is matched to our bone's. Hip motion is scaled to our character's
size and kept in place; optionally the capture's turning is taken out too,
so a hero runs straight however the actor wandered around the studio.
Long takes are cut down to a seamless loop: the stretch of the chosen length
whose last frame matches its first most closely.

The clips the game uses (`CLIPS`, and heroes' own `HERO_CLIPS`) are exported
to `crates/client/assets/animations/Mocap.glb` under the game's names.

Run from the repository root:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \\
        -P tools/blender/retarget_mocap.py -- [--preview DIR] [--try NAME,...]

`--try` converts only the listed clips or `CANDIDATES` (with `--preview`,
to judge them) without exporting. Downloads live outside this repository, in
the private `talls2/volley-assets` cloned at `~/Downloads/volley-assets`
(`git sparse-checkout add mixamo mocap` there): Mixamo's terms allow using its
animations in a game but not republishing the raw files.
"""

import importlib.util
import math
import sys
from pathlib import Path

import bpy
from mathutils import Quaternion, Vector

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("volley_animations", HERE / "volley_animations.py")
volley = importlib.util.module_from_spec(spec)
spec.loader.exec_module(volley)

OUTPUT = volley.ASSETS / "animations/Mocap.glb"
ASSETS = Path("~/Downloads/volley-assets").expanduser()
MIXAMO = ASSETS / "mixamo"
MOCAP = ASSETS / "mocap"
# Leaves the hand-keyed accents off, to compare against (`--no-accents`).
NO_ACCENTS = False


def clip(path, straighten=False, loop=None, span=None, plant=False, still=False, upright=False, accents=()):
    """A capture to retarget. `loop` (min, max seconds) cuts the best
    seamless loop of that length out of it; `span` (start, end seconds) cuts
    a fixed stretch; `straighten` takes out the capture's turning (or, given a
time in seconds, turns the whole take to face forward at that moment, which
keeps the body's twist through a swing); `plant`
    holds the legs and hips in the loop's first stance, for an idle made from
    a take where the actor moved around; `still` keeps the hips at standing
    height, raised by that many meters if it's a number, for a move made in
    the air, where the game does the jumping; `accents` are hand-keyed poses
    blended over the capture (see `accent`); `upright` keeps the hips level,
    turning only about the vertical, with the spine and legs keeping their
    pose in the world: for hits the game plays on the upper body over running
    legs, where the spine is posed on the running hips instead of the
    capture's, so the capture's hips tilt would fold the body over."""
    return dict(path=Path(path), straighten=straighten, loop=loop, span=span, plant=plant, still=still, upright=upright,
                accents=accents)


def accent(time, arms, into=0.12, hold=0.03, out=0.15):
    """A hand-keyed pose layered over a capture, the way an animator pushes
    mocap: fully on from `time` (clip seconds, after any cut) for `hold`
    seconds, easing in over `into` before and out over `out` after, so it
    blends into the capture on both sides. Captures soften the big moments
    (actors don't really hit, trackers smooth the fastest motion); this puts
    the strong pose back at contact. See docs/animation/experiments.md."""
    return dict(time=time, arms=arms, into=into, hold=hold, out=out)


def arm(side, base, hand, elbow, wrist=0.0, fingers=None):
    """One arm of an accent pose, solved with the rig's arm IK: the wrist goes
    to `hand` and the elbow bends toward `elbow`, both (right, forward, up)
    meters from `base` ("shoulder" for this side's shoulder joint, or "head"
    for the base of the head, about level with the shoulders, or "hands" for
    the point between the capture's two wrists, to reshape the hands where
    the capture already holds them) as the capture has it that frame, so the pose rides
    along with the body. `wrist` bends the hand back (positive) or forward,
    in degrees; `fingers` curls them (radians), or leaves the capture's."""
    return dict(side=side, base=base, hand=Vector(hand), elbow=Vector(elbow), wrist=wrist, fingers=fingers)


LEGS = [f"{part}_{side}" for side in "lr" for part in ("thigh", "calf", "foot", "ball")]

STYLE = MOCAP / "100style"
CMU = MOCAP / "cmu"
DEEPMOTION = MOCAP / "deepmotion"


# Hand-keyed contact poses over the tracked set and spike, from volleyball
# biomechanics (docs/animation/research.md). The spike at contact: the hitting
# arm high and a little in front of the shoulder (about 130 degrees from the
# side, elbow bent about 35), the wrist starting to snap over, and the free
# arm pulled down across the body. The set: the hands brought together into a
# window where the capture holds them (the game's sets meet balls about 2 m
# up, as high as these arms reach), wrists bent back, elbows out.
SPIKE_ACCENT = accent(0.82, [
    arm("r", "shoulder", hand=(0.10, 0.22, 0.50), elbow=(1.0, -0.3, 0.3), wrist=-25, fingers=0.15),
    arm("l", "shoulder", hand=(0.30, 0.25, -0.45), elbow=(-1.0, -0.3, -0.2), fingers=0.6),
], into=0.12, hold=0.0, out=0.06)
# The spike's follow-through: the capture's hitting arm stops at the chest
# (trackers lose the fast whip), so it's carried on down across the body to
# the opposite hip, the wrist snapped over, the free arm swept back by its side.
SPIKE_FOLLOW = accent(0.97, [
    arm("r", "shoulder", hand=(-0.32, 0.3, -0.4), elbow=(1.0, 0.0, -0.3), wrist=-40, fingers=0.3),
    arm("l", "shoulder", hand=(-0.02, -0.12, -0.52), elbow=(-1.0, -0.2, 0.0), fingers=0.4),
], into=0.15, hold=0.4, out=0.15)
SET_ACCENT = accent(1.05, [
    arm("l", "hands", hand=(-0.11, 0.0, 0.0), elbow=(-1.0, 0.4, -0.4), wrist=55, fingers=0.3),
    arm("r", "hands", hand=(0.11, 0.0, 0.0), elbow=(1.0, 0.4, -0.4), wrist=55, fingers=0.3),
], into=0.25, hold=0.05, out=0.2)

# The game's clips. Tried and left out from Mixamo: "Run To Dive" (a dive into
# water, not along the sand) and "Flying Bicycle Kick" (barely moves); our
# own clips do those better.
CLIPS = {
    "Ready": clip(MIXAMO / "Goalkeeper Idle.fbx"),
    "Jog": clip(MIXAMO / "Running.fbx"),
    "Sprint": clip(MIXAMO / "Fast Run.fbx"),
    "Airborne": clip(MIXAMO / "Falling Idle.fbx"),
    "Land": clip(MIXAMO / "Landing.fbx"),
    "Dash": clip(MIXAMO / "Idle To Sprint.fbx"),
    "Knocked_Down": clip(MIXAMO / "Knocked Down.fbx"),
    "Celebrate": clip(MIXAMO / "Cheering.fbx"),
    # Volleyball tracked from videos with DeepMotion Animate 3D, from the held
    # wind-up through contact to the follow-through (the game's `Swing`
    # timings are measured with `--measure`). The set: hands rise to the
    # forehead, wait for the ball and push.
    "Set": clip(DEEPMOTION / "set_overhead.bvh", straighten=True, span=(1.4, 3.3), upright=True, accents=[SET_ACCENT]),
    # A float serve: tossing arm up and hitting arm cocked, the hit above the
    # shoulder, and the arm held out after it.
    "Serve": clip(DEEPMOTION / "serve_standing.bvh", straighten=5.0, span=(4.45, 5.75)),
    # The spike, from the top of the jump: arm drawn back like a bow, the
    # whip through the ball and down. In the air, where the game jumps; the
    # actor hunches into the hit, so the body is raised to reach the ball.
    "Spike": clip(DEEPMOTION / "spike_approach_man.bvh", straighten=5.83, span=(5.0, 6.3), still=0.2, accents=[SPIKE_ACCENT, SPIKE_FOLLOW]),
}

# Heroes' own versions of clips, for their style of moving: exported as
# `<Hero>_<Clip>`, which the game prefers for that hero.
HERO_CLIPS = {
    # A jump shot to celebrate: CMU subject 6's crossover and shot.
    "Cross": {
        "Celebrate": clip(CMU / "06_14.bvh", straighten=True, span=(1.7, 3.7)),
    },
    "Golazo": {},
}
# Tried for heroes' ready stances and left out, as worse than the shared
# goalkeeper crouch: CMU 06_13's dribbling stance (planted, it's narrow and
# hunched) and 100STYLE's idles (SwingShoulders reads as slouching). 100STYLE's
# "runs" are jogs at about 1.2 m/s, far too slow for the game's running.
RUN_LOOP = (0.55, 0.9)
IDLE_LOOP = (2.5, 4.0)
# Captures to judge before choosing: `--try all` previews every one.
CANDIDATES = {
    **{f"{style}_Idle": clip(STYLE / f"{style}_ID.bvh", straighten=True, loop=IDLE_LOOP) for style in (
        "Neutral", "Strutting", "Proud", "SwingShoulders", "Elated", "OnToesBentForward", "HighKnees")},
    **{f"{style}_Run": clip(STYLE / f"{style}_FR.bvh", straighten=True, loop=RUN_LOOP) for style in (
        "Neutral", "Strutting", "Proud", "SwingShoulders", "Elated", "OnToesBentForward", "HighKnees")},
    "CMU_Crossover": clip(CMU / "06_12.bvh", straighten=True, span=(0.0, 4.0)),
    "CMU_Through_Legs": clip(CMU / "06_13.bvh", straighten=True, span=(2.0, 6.0)),
    "CMU_Crossover_Shoot": clip(CMU / "06_14.bvh", span=(0.0, 4.0)),
    "CMU_Soccer_Kick": clip(CMU / "10_01.bvh", span=(0.0, 4.0)),
    "CMU_Soccer_Kick_2": clip(CMU / "11_01.bvh", span=(0.0, 4.0)),
    # Volleyball tracked from videos with DeepMotion Animate 3D, whole takes.
    # The beach spike and the woman's approach and jump came out too noisy.
    **{f"DM_{name.title().replace('_', '')}": clip(DEEPMOTION / f"{name}.bvh", straighten=True) for name in (
        "spike_beach", "serve_standing", "set_overhead", "spike_approach_woman", "spike_jump_woman", "spike_approach_man")},
}


# Each source's bone names -> ours. A bone runs from its joint toward its child.
def mixamo_bones():
    bones = {"Hips": "pelvis", "Spine": "spine_01", "Spine1": "spine_02", "Spine2": "spine_03", "Neck": "neck_01", "Head": "Head"}
    for side, ours in (("Left", "l"), ("Right", "r")):
        bones.update({
            f"{side}Shoulder": f"clavicle_{ours}", f"{side}Arm": f"upperarm_{ours}",
            f"{side}ForeArm": f"lowerarm_{ours}", f"{side}Hand": f"hand_{ours}",
            f"{side}UpLeg": f"thigh_{ours}", f"{side}Leg": f"calf_{ours}",
            f"{side}Foot": f"foot_{ours}", f"{side}ToeBase": f"ball_{ours}",
        })
        for finger, theirs in (("thumb", "Thumb"), ("index", "Index"), ("middle", "Middle"), ("ring", "Ring"), ("pinky", "Pinky")):
            for joint in (1, 2, 3):
                bones[f"{side}Hand{theirs}{joint}"] = f"{finger}_0{joint}_{ours}"
    return bones


def style_bones():
    bones = {"Hips": "pelvis", "Chest": "spine_01", "Chest2": "spine_02", "Chest4": "spine_03", "Neck": "neck_01", "Head": "Head"}
    for side, ours in (("Left", "l"), ("Right", "r")):
        bones.update({
            f"{side}Collar": f"clavicle_{ours}", f"{side}Shoulder": f"upperarm_{ours}",
            f"{side}Elbow": f"lowerarm_{ours}", f"{side}Wrist": f"hand_{ours}",
            f"{side}Hip": f"thigh_{ours}", f"{side}Knee": f"calf_{ours}",
            f"{side}Ankle": f"foot_{ours}", f"{side}Toe": f"ball_{ours}",
        })
    return bones


def cmu_bones():
    bones = {"root": "pelvis", "lowerback": "spine_01", "upperback": "spine_02", "thorax": "spine_03",
             "lowerneck": "neck_01", "head": "Head"}
    for side in ("l", "r"):
        bones.update({
            f"{side}clavicle": f"clavicle_{side}", f"{side}humerus": f"upperarm_{side}",
            f"{side}radius": f"lowerarm_{side}", f"{side}wrist": f"hand_{side}",
            f"{side}femur": f"thigh_{side}", f"{side}tibia": f"calf_{side}",
            f"{side}foot": f"foot_{side}", f"{side}toes": f"ball_{side}",
        })
    return bones


def deepmotion_bones():
    bones = {"hips_JNT": "pelvis", "spine_JNT": "spine_01", "spine1_JNT": "spine_02", "spine2_JNT": "spine_03",
             "neck_JNT": "neck_01", "head_JNT": "Head"}
    for side in ("l", "r"):
        bones.update({
            f"{side}_shoulder_JNT": f"clavicle_{side}", f"{side}_arm_JNT": f"upperarm_{side}",
            f"{side}_forearm_JNT": f"lowerarm_{side}", f"{side}_hand_JNT": f"hand_{side}",
            f"{side}_upleg_JNT": f"thigh_{side}", f"{side}_leg_JNT": f"calf_{side}",
            f"{side}_foot_JNT": f"foot_{side}", f"{side}_toebase_JNT": f"ball_{side}",
        })
        for finger, theirs in (("thumb", "Thumb"), ("index", "Index"), ("middle", "Middle"), ("ring", "Ring"), ("pinky", "Pinky")):
            for joint in (1, 2, 3):
                bones[f"{side}_hand{theirs}{joint}_JNT"] = f"{finger}_0{joint}_{side}"
    return bones


def plain(name):
    """A bone name without a rig prefix, like Mixamo's "mixamorig:"."""
    return name.split(":")[-1]


def load(path):
    """Imports a capture: its armature, action, everything it added, and the
    bone map for its skeleton."""
    before = set(bpy.data.objects)
    if path.suffix.lower() == ".fbx":
        bpy.ops.import_scene.fbx(filepath=str(path), automatic_bone_orientation=False, ignore_leaf_bones=True)
        bones = mixamo_bones()
    else:
        bpy.ops.import_anim.bvh(filepath=str(path), global_scale=0.01, use_fps_scale=True, update_scene_fps=False,
                                update_scene_duration=False, rotate_mode="NATIVE", axis_forward="-Z", axis_up="Y")
        bones = {"cmu": cmu_bones, "deepmotion": deepmotion_bones}.get(path.parent.name, style_bones)()
    new = [ob for ob in bpy.data.objects if ob not in before]
    armature = next(ob for ob in new if ob.type == "ARMATURE")
    action = armature.animation_data.action if armature.animation_data else None
    return armature, action, new, bones


def direction(ob, bone):
    """A bone's rest direction in the world."""
    return (ob.matrix_world.to_3x3() @ (bone.bone.tail_local - bone.bone.head_local)).normalized()


def heading(right):
    """The angle about the vertical of a sideways (left-to-right) vector."""
    return math.atan2(right.y, right.x)


def level(turn):
    """Only the part of a turn that's about the vertical (its twist about z)."""
    twist = Quaternion((turn.w, 0.0, 0.0, turn.z))
    return twist.normalized() if twist.magnitude > 1e-6 else Quaternion()


def retarget(rig, name, spec):
    source, action, imported, bone_map = load(spec["path"])
    scene = bpy.context.scene
    fps = scene.render.fps
    start, end = (round(f) for f in action.frame_range)
    pairs = [(bone, rig.bones[bone_map[plain(bone.name)]]) for bone in source.pose.bones if plain(bone.name) in bone_map]
    by_target = {target.name: bone for bone, target in pairs}

    # Face the capture like ours: our left-to-right runs along -x.
    def across(get):
        return (get(by_target["thigh_r"]) - get(by_target["thigh_l"])).to_2d()

    rest_across = across(lambda b: source.matrix_world @ b.bone.head_local)
    ours_across = (rig.arm.data.bones["thigh_r"].head_local - rig.arm.data.bones["thigh_l"].head_local).to_2d()
    face = Quaternion((0, 0, 1), heading(ours_across) - heading(rest_across))

    # Each bone's rest direction matched to ours.
    align = {}
    for bone, target in pairs:
        theirs = face @ direction(source, bone)
        mine = (target.bone.tail_local - target.bone.head_local).normalized()
        align[target.name] = mine.rotation_difference(theirs)
    source_rest = {bone.name: face @ (source.matrix_world @ bone.bone.matrix_local).to_quaternion() for bone, _ in pairs}

    # Scale by leg length, the thigh and shin: captures' rest poses don't
    # always stand on the ground, but legs are legs.
    def leg(bones, get):
        return sum((get(bones[f"{part}_l"], "tail") - get(bones[f"{part}_l"], "head")).length for part in ("thigh", "calf"))

    theirs = leg(by_target, lambda b, end: source.matrix_world @ getattr(b.bone, f"{end}_local"))
    ours = leg(rig.arm.data.bones, lambda b, end: getattr(b, f"{end}_local"))
    scale = ours / theirs
    hips = by_target["pelvis"]
    feet = [by_target["foot_l"], by_target["foot_r"]]
    our_hip = rig.arm.data.bones["pelvis"].head_local.z
    our_ankle = rig.arm.data.bones["foot_l"].head_local.z

    # Take out the capture's turning: turn it back to face its rest heading.
    def straightening():
        now_across = across(lambda b: face @ (source.matrix_world @ b.head))
        return Quaternion((0, 0, 1), heading(ours_across) - heading(now_across))

    fixed = None
    if spec["straighten"] is not True and spec["straighten"]:
        scene.frame_set(start + round(spec["straighten"] * fps))
        fixed = straightening()

    frames = []
    for frame in range(start, end + 1):
        scene.frame_set(frame)
        hip = face @ (source.matrix_world @ hips.head)
        straight = fixed if fixed is not None else straightening() if spec["straighten"] else Quaternion()
        change = {}
        for bone, target in pairs:
            world = straight @ face @ (source.matrix_world @ bone.matrix).to_quaternion()
            change[target.name] = world @ source_rest[bone.name].inverted() @ align[target.name]
        ankles = [(face @ (source.matrix_world @ foot.head)).z for foot in feet]
        feet_now = [(straight @ (face @ (source.matrix_world @ foot.head) - hip)) * scale for foot in feet]
        if spec["upright"]:
            # Level the hips, moving them so the hip joints stay where they
            # were: the legs keep their place, the spine nearly so.
            tilted, change["pelvis"] = change["pelvis"], level(change["pelvis"])
            hip_joint = sum((rig.arm.data.bones[f"thigh_{side}"].head_local for side in "lr"), Vector()) / 2
            hip_joint -= rig.arm.data.bones["pelvis"].head_local
            hip += (tilted @ hip_joint - change["pelvis"] @ hip_joint) / scale
        frames.append([change, (hip.z, min(ankles)), feet_now])

    for ob in imported:
        bpy.data.objects.remove(ob)
    if action.users == 0:
        bpy.data.actions.remove(action)

    # Hip height over the ground (the lowest the ankles get), in place: only
    # the bob up and down is kept.
    ground = min(f[1][1] for f in frames)
    for f in frames:
        f[1] = Vector((0.0, 0.0, (f[1][0] - ground) * scale + our_ankle - our_hip))
    frames = cut(frames, fps, spec)
    if spec["still"] is not False:
        lift = Vector((0.0, 0.0, 0.0 if spec["still"] is True else spec["still"]))
        frames = [(change, lift, feet) for change, _, feet in frames]
    if spec["plant"]:
        stance = frames[0]
        frames = [({**change, **{bone: stance[0][bone] for bone in LEGS if bone in stance[0]}}, stance[1], feet)
                  for change, _, feet in frames]

    # Key our bones: each turns in the world as its counterpart did.
    rest = rig.rest
    out = bpy.data.actions.new(name)
    out.use_fake_user = True
    rig.arm.animation_data_create()
    rig.arm.animation_data.action = out
    rig.set_constraints(False)
    previous = {}
    for index, (change, offset, _) in enumerate(frames):
        rig.reset()
        world = {}
        for bone in rig.bones:
            parent = world.get(bone.parent.name, Quaternion()) if bone.parent else Quaternion()
            # How far each bone has turned in the world since rest; unmapped
            # bones (fingers, leaf bones) turn with their parent.
            world[bone.name] = change.get(bone.name, parent)
            rotation = rest[bone.name].inverted() @ parent.inverted() @ world[bone.name] @ rest[bone.name]
            if bone.name in previous and previous[bone.name].dot(rotation) < 0:
                rotation = -rotation
            previous[bone.name] = rotation
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = rotation
            bone.keyframe_insert("rotation_quaternion", frame=index, group=bone.name)
        pelvis = rig.bones["pelvis"]
        pelvis.location = rest["pelvis"].inverted() @ offset
        pelvis.keyframe_insert("location", frame=index, group="pelvis")
    if spec["still"] is False:
        settle(rig, frames, rest)
    if NO_ACCENTS:
        print("  accents left off")
    for layer in () if NO_ACCENTS else spec["accents"]:
        apply_accent(rig, layer, len(frames), fps)
    rig.arm.animation_data.action = None
    rig.reset()
    seconds = (len(frames) - 1) / fps
    print(f"retargeted {name}: {len(frames)} frames, {seconds:.2f} s, {len(pairs)} bones, pace {pace([f[2] for f in frames], fps):.1f} m/s")
    return out, seconds


def lowest_point(rig):
    """How high the lowest part of the feet (ankles and balls) is in the pose."""
    world = rig.arm.matrix_world
    return min((world @ rig.bones[f"{part}_{side}"].head).z for part in ("foot", "ball") for side in "lr")


def settle(rig, frames, rest):
    """Lowers a clip whose feet never reach the floor: captures whose
    skeleton is built differently from ours (Mixamo's) come out hovering
    by a few centimeters, the hips scaled by leg length but the feet not
    meeting the ground. The lowest the feet get through the clip touches
    the floor, as our rest pose's feet do."""
    scene = bpy.context.scene
    floor = rig.arm.data.bones["ball_l"].head_local.z
    lowest = []
    for index in range(len(frames)):
        scene.frame_set(index)
        lowest.append(lowest_point(rig))
    sink = min(lowest) - floor
    if sink <= 0.005:
        return
    pelvis = rig.bones["pelvis"]
    for index, (_, offset, _) in enumerate(frames):
        pelvis.location = rest["pelvis"].inverted() @ (offset - Vector((0.0, 0.0, sink)))
        pelvis.keyframe_insert("location", frame=index, group="pelvis")
    print(f"  lowered {sink:.3f} m onto the floor")


def accent_weight(layer, t):
    """How strongly an accent applies at clip time `t`: 0 to 1, smoothly."""
    start, end = layer["time"], layer["time"] + layer["hold"]
    if t < start:
        x = 1.0 - (start - t) / layer["into"]
    elif t > end:
        x = 1.0 - (t - end) / layer["out"]
    else:
        return 1.0
    x = max(0.0, min(1.0, x))
    return x * x * (3.0 - 2.0 * x)


def apply_accent(rig, layer, count, fps):
    """Blends an accent pose over the action on `rig.arm`, frame by frame:
    poses the capture, solves the accent's arms with IK from where the body
    is, and keys each arm bone part way from the capture to that pose."""
    scene = bpy.context.scene
    world = rig.arm.matrix_world

    def spot(position):
        p = world @ position
        return Vector((-p.x, -p.y, p.z))

    def place(v):
        return Vector((-v.x, -v.y, v.z))

    sides = {a["side"] for a in layer["arms"]}
    chain = lambda side: [f"upperarm_{side}", f"lowerarm_{side}", f"hand_{side}"] + [
        f"{finger}_0{joint}_{side}" for finger in volley.FINGERS + ["thumb"] for joint in (1, 2, 3)]
    for frame in range(count):
        w = accent_weight(layer, frame / fps)
        if w <= 0.0:
            continue
        scene.frame_set(frame)
        captured = {name: rig.bones[name].rotation_quaternion.copy() for side in sides for name in chain(side)}
        between = (rig.bones["hand_l"].head + rig.bones["hand_r"].head) / 2
        for limb in layer["arms"]:
            side = limb["side"]
            base = {"shoulder": rig.bones[f"upperarm_{side}"].head, "head": rig.bones["Head"].head, "hands": between}[limb["base"]]
            origin = spot(base)
            rig.targets[f"hand_{side}"].location = place(origin + limb["hand"])
            rig.targets[f"elbow_{side}"].location = place(origin + limb["elbow"])
            rig.bones[f"lowerarm_{side}"].constraints["IK"].enabled = True
            hand = rig.bones[f"hand_{side}"]
            rest = rig.rest[hand.name]
            hand.rotation_quaternion = rest.inverted() @ volley.wrist_back(limb["wrist"], side) @ rest
            if limb["fingers"] is not None:
                rig.curl_fingers(side, limb["fingers"])
        bpy.context.view_layer.update()
        posed = rig.visual_basis()
        for side in sides:
            rig.bones[f"lowerarm_{side}"].constraints["IK"].enabled = False
            for name in chain(side):
                target = posed[name][1]
                if captured[name].dot(target) < 0:
                    target = -target
                bone = rig.bones[name]
                bone.rotation_quaternion = captured[name].slerp(target, w)
                bone.keyframe_insert("rotation_quaternion", frame=frame, group=name)
    print(f"  accent at {layer['time']:.2f} s on {', '.join(sorted(sides))}")


def cut(frames, fps, spec):
    """The frames to keep: a fixed stretch, or the best seamless loop."""
    if spec["span"]:
        a, b = (round(t * fps) for t in spec["span"])
        return frames[a : b + 1]
    if not spec["loop"]:
        return frames
    shortest, longest = (round(t * fps) for t in spec["loop"])
    names = sorted(frames[0][0])

    def pose(i):
        return [frames[i][0][n] for n in names]

    def distance(i, j):
        return sum(1.0 - abs(p.dot(q)) for p, q in zip(pose(i), pose(j))) + abs(frames[i][1].z - frames[j][1].z)

    # Skip the start and end of a take, where the actor starts and stops.
    margin = min(len(frames) // 4, 3 * fps)
    best = None
    for i in range(margin, len(frames) - margin - longest, 2):
        for length in range(shortest, longest + 1):
            d = distance(i, i + length)
            if best is None or d < best[0]:
                best = (d, i, length)
    _, i, length = best
    print(f"  loop at {i / fps:.2f} s, {length / fps:.2f} s long, mismatch {best[0]:.4f}")
    # The last frame repeats the first, so the loop closes exactly.
    return frames[i : i + length] + [frames[i]]


def pace(strides, fps):
    """How fast a clip carries the body along, in m/s, from how fast a planted
    foot slides back under the hips: the ground speed it looks right at."""
    speeds = []
    for foot in range(2):
        heights = [frame[foot].z for frame in strides]
        planted = min(heights) + 0.03
        for a, b in zip(strides, strides[1:]):
            if a[foot].z < planted and b[foot].z < planted:
                speeds.append((b[foot] - a[foot]).xy.length * fps)
    speeds.sort()
    return speeds[len(speeds) // 2] if speeds else 0.0


def measure(rig, action):
    """Prints where the hands are through a clip, in the character's terms
    (right, forward, up, in meters from the feet), to find its contact frame
    and where the ball is then: `Swing` in the game's characters.rs."""
    scene = bpy.context.scene
    rig.arm.animation_data_create()
    rig.arm.animation_data.action = action
    start, end = (round(f) for f in action.frame_range)
    world = rig.arm.matrix_world

    # The knuckles of the middle finger: the palm, where a hand meets the ball.
    # (The skeleton's bone tails don't point anywhere meaningful.)
    def spot(bone):
        p = world @ rig.bones[bone].head
        return Vector((-p.x, -p.y, p.z))

    previous = None
    for frame in range(start, end + 1):
        scene.frame_set(frame)
        right, left = spot("middle_01_r"), spot("middle_01_l")
        speed = (right - previous).length * scene.render.fps if previous is not None else 0.0
        previous = right
        print(f"  {frame / scene.render.fps:5.2f} s  right hand {right.x:5.2f} {right.y:5.2f} {right.z:5.2f}  "
              f"left hand {left.x:5.2f} {left.y:5.2f} {left.z:5.2f}  right speed {speed:4.1f} m/s")
    rig.arm.animation_data.action = None
    rig.reset()


def heights(rig, action):
    """Prints how high the ankles and balls of the feet get through a clip
    (lowest, median), to check the feet meet the floor."""
    scene = bpy.context.scene
    rig.arm.animation_data_create()
    rig.arm.animation_data.action = action
    start, end = (round(f) for f in action.frame_range)
    world = rig.arm.matrix_world
    rows = {name: [] for name in ("foot_l", "foot_r", "ball_l", "ball_r")}
    for frame in range(start, end + 1):
        scene.frame_set(frame)
        for name in rows:
            rows[name].append((world @ rig.bones[name].head).z)
    low = lambda side: [min(a, b) for a, b in zip(rows[f"ball_{side}"], rows[f"foot_{side}"])]
    lowest = sorted(min(a, b) for a, b in zip(low("l"), low("r")))
    ankles = sorted(min(a, b) for a, b in zip(rows["foot_l"], rows["foot_r"]))
    print(f"  heights {action.name}: lowest point of the feet min {lowest[0]:.3f} median {lowest[len(lowest) // 2]:.3f}; "
          f"lower ankle min {ankles[0]:.3f} median {ankles[len(ankles) // 2]:.3f}")
    rig.arm.animation_data.action = None
    rig.reset()


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    preview = args[args.index("--preview") + 1] if "--preview" in args else None
    trying = args[args.index("--try") + 1].split(",") if "--try" in args else None
    global NO_ACCENTS
    NO_ACCENTS = "--no-accents" in args
    measuring = "--measure" in args
    rig = volley.Rig()
    bpy.context.scene.render.fps = 30
    jobs = dict(CLIPS)
    jobs.update({f"{hero}_{name}": spec for hero, clips in HERO_CLIPS.items() for name, spec in clips.items()})
    if trying:
        pool = {**jobs, **CANDIDATES}
        jobs = CANDIDATES if trying == ["all"] else {name: pool[name] for name in trying}
    missing = [str(spec["path"]) for spec in jobs.values() if not spec["path"].exists()]
    if missing:
        sys.exit(f"missing downloads: {missing}")
    for name, spec in jobs.items():
        action, seconds = retarget(rig, name, spec)
        if measuring:
            measure(rig, action)
        if "--heights" in args:
            heights(rig, action)
        if preview:
            Path(preview).mkdir(parents=True, exist_ok=True)
            sheet = {"name": action.name, "keys": [(0.0, {}), (seconds, {})]}
            volley.render_contact_sheet(rig, sheet, action, preview)
    if not trying:
        volley.OUTPUT = OUTPUT
        volley.export(rig)
        print(f"wrote {OUTPUT}")


main()
