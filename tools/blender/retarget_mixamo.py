"""Retargets Mixamo motion capture onto Volley's skeleton.

Mixamo animations come on Mixamo's own skeleton. This imports each FBX, and
for every frame turns each of our bones the way its Mixamo counterpart turned
from its rest pose (both rest in a T-pose, so world-space changes carry over
directly). Hip motion is scaled to our character's size and kept in place.
The clips the game uses (see `CLIPS`) are exported to
`crates/client/assets/animations/Mocap.glb`, under the game's names for them.

Run from the repository root:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \\
        -P tools/blender/retarget_mixamo.py -- --source ~/Downloads/volley-mixamo [--preview DIR] [--list]

`--list` prints each file's name, length and bones, to see what's there, and
`--all` converts every file (with `--preview`, to judge new downloads).
The downloaded FBX files stay out of the repository: Mixamo's terms allow
using the animations in a game but not republishing the raw files.
"""

import importlib.util
import re
import sys
from pathlib import Path

import bpy
from mathutils import Quaternion, Vector

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("volley_animations", HERE / "volley_animations.py")
volley = importlib.util.module_from_spec(spec)
spec.loader.exec_module(volley)

OUTPUT = volley.ASSETS / "animations/Mocap.glb"

# Downloaded file (Mixamo's name for it) -> the game's name for the clip.
# Tried and left out: "Run To Dive" (a dive into water, not along the sand)
# and "Flying Bicycle Kick" (barely moves); our own clips do those better.
CLIPS = {
    "Goalkeeper Idle": "Ready",
    "Running": "Jog",
    "Fast Run": "Sprint",
    "Falling Idle": "Airborne",
    "Landing": "Land",
    "Idle To Sprint": "Dash",
    "Knocked Down": "Knocked_Down",
    "Cheering": "Celebrate",
}

# Mixamo bone (without its "mixamorig:" prefix) -> ours.
BONES = {
    "Hips": "pelvis",
    "Spine": "spine_01",
    "Spine1": "spine_02",
    "Spine2": "spine_03",
    "Neck": "neck_01",
    "Head": "Head",
}
for side, ours in (("Left", "l"), ("Right", "r")):
    BONES.update({
        f"{side}Shoulder": f"clavicle_{ours}",
        f"{side}Arm": f"upperarm_{ours}",
        f"{side}ForeArm": f"lowerarm_{ours}",
        f"{side}Hand": f"hand_{ours}",
        f"{side}UpLeg": f"thigh_{ours}",
        f"{side}Leg": f"calf_{ours}",
        f"{side}Foot": f"foot_{ours}",
        f"{side}ToeBase": f"ball_{ours}",
    })
    for finger, theirs in (("thumb", "Thumb"), ("index", "Index"), ("middle", "Middle"), ("ring", "Ring"), ("pinky", "Pinky")):
        for joint in (1, 2, 3):
            BONES[f"{side}Hand{theirs}{joint}"] = f"{finger}_0{joint}_{ours}"


def plain(name):
    """A Mixamo bone name without its rig prefix, like "mixamorig:" or "mixamorig1:"."""
    return name.split(":")[-1]


def clip_name(path):
    """An animation name from a file name: "Volleyball Spike (2).fbx" -> "Volleyball_Spike_2"."""
    return re.sub(r"[^A-Za-z0-9]+", "_", path.stem).strip("_")


def world_rotation(ob, bone):
    return (ob.matrix_world @ bone.matrix).to_quaternion()


def world_rest(ob, bone):
    return (ob.matrix_world @ bone.bone.matrix_local).to_quaternion()


def import_fbx(path):
    """Imports an FBX and returns its armature and action."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=str(path), automatic_bone_orientation=False, ignore_leaf_bones=True)
    new = [ob for ob in bpy.data.objects if ob not in before]
    armature = next(ob for ob in new if ob.type == "ARMATURE")
    action = armature.animation_data.action if armature.animation_data else None
    return armature, action, new


def retarget(rig, path, name):
    source, action, imported = import_fbx(path)
    if action is None:
        print(f"skipping {path.name}: no animation")
        for ob in imported:
            bpy.data.objects.remove(ob)
        return None
    start, end = (round(f) for f in action.frame_range)
    scene = bpy.context.scene
    fps = scene.render.fps
    pairs = [(bone, rig.bones[BONES[plain(bone.name)]]) for bone in source.pose.bones if plain(bone.name) in BONES]
    source_rest = {bone.name: world_rest(source, bone) for bone, _ in pairs}
    target_rest = rig.rest

    # Hip heights, to scale hip motion to our character.
    source_hips = next(bone for bone, target in pairs if target.name == "pelvis")
    source_hip_rest = (source.matrix_world @ source_hips.bone.head_local)
    target_hip_height = rig.arm.data.bones["pelvis"].head_local.z
    scale = target_hip_height / max(source_hip_rest.z, 1e-3)

    frames = []
    for frame in range(start, end + 1):
        scene.frame_set(frame)
        # How much each mapped bone has turned in the world since its rest pose.
        change = {target.name: world_rotation(source, bone) @ source_rest[bone.name].inverted() for bone, target in pairs}
        hips = source.matrix_world @ source_hips.head
        offset = (hips - source_hip_rest) * scale
        # In place: the game moves the body; keep only the bob up and down.
        offset.x = offset.y = 0.0
        frames.append((change, offset))

    for ob in imported:
        bpy.data.objects.remove(ob)
    if action.users == 0:
        bpy.data.actions.remove(action)

    # Key our bones: each turns in the world as its counterpart did.
    out = bpy.data.actions.new(name)
    out.use_fake_user = True
    rig.arm.animation_data_create()
    rig.arm.animation_data.action = out
    rig.set_constraints(False)
    previous = {}
    for index, (change, offset) in enumerate(frames):
        rig.reset()
        world = {}
        for bone in rig.bones:
            parent = world.get(bone.parent.name, Quaternion()) if bone.parent else Quaternion()
            world[bone.name] = change.get(bone.name, parent)
            rest = target_rest[bone.name]
            rotation = rest.inverted() @ parent.inverted() @ world[bone.name] @ rest
            if bone.name in previous and previous[bone.name].dot(rotation) < 0:
                rotation = -rotation
            previous[bone.name] = rotation
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = rotation
            bone.keyframe_insert("rotation_quaternion", frame=index, group=bone.name)
        pelvis = rig.bones["pelvis"]
        pelvis.location = target_rest["pelvis"].inverted() @ offset
        pelvis.keyframe_insert("location", frame=index, group="pelvis")
    rig.arm.animation_data.action = None
    rig.reset()
    seconds = (len(frames) - 1) / fps
    print(f"retargeted {name}: {len(frames)} frames, {seconds:.2f} s, {len(pairs)} bones")
    return out, seconds


def list_files(files):
    for path in files:
        source, action, imported = import_fbx(path)
        names = sorted(plain(bone.name) for bone in source.pose.bones)
        unmapped = [n for n in names if n not in BONES]
        span = tuple(round(f) for f in action.frame_range) if action else None
        print(f"{path.name}: frames {span}, {len(names)} bones, unmapped {unmapped}")
        for ob in imported:
            bpy.data.objects.remove(ob)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    source = Path(args[args.index("--source") + 1]).expanduser()
    preview = args[args.index("--preview") + 1] if "--preview" in args else None
    files = sorted(source.glob("*.fbx"))
    if not files:
        sys.exit(f"no .fbx files in {source}")
    rig = volley.Rig()
    bpy.context.scene.render.fps = 30
    if "--list" in args:
        list_files(files)
        return
    if "--all" not in args:
        missing = [stem for stem in CLIPS if not (source / f"{stem}.fbx").exists()]
        if missing:
            sys.exit(f"missing downloads: {missing}")
        files = [source / f"{stem}.fbx" for stem in CLIPS]
    for path in files:
        result = retarget(rig, path, CLIPS.get(path.stem, clip_name(path)))
        if result and preview:
            action, seconds = result
            Path(preview).mkdir(parents=True, exist_ok=True)
            clip = {"name": action.name, "keys": [(0.0, {}), (seconds, {})]}
            volley.render_contact_sheet(rig, clip, action, preview)
    volley.OUTPUT = OUTPUT
    volley.export(rig)
    print(f"wrote {OUTPUT}")


main()
