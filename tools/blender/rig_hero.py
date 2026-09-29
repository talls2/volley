"""Rigs a hero's 3D model (from Tripo, or any image-to-3D tool) onto Volley's
skeleton, so every animation in the game plays on it.

The model comes in as one textured mesh in a relaxed A-pose. This:

1. stands it on the ground, facing -y, at the hero's height, and simplifies
   it to a game-sized mesh. Simplifying scrambles the image-to-3D model's
   texture layout (thousands of tiny islands), so the simple mesh gets a clean
   layout of its own and its colors and surface detail are baked onto it from
   the full model;
2. scales the placeholder superhero's skeleton and body to the same height,
   stretches the arm bones to reach the model's hands, and poses the arms down
   to the model's A-pose;
3. copies each vertex's bone weights from the nearest point of the placeholder
   body, which is skinned to the same skeleton and now in the same pose;
4. makes that pose the skeleton's rest pose and exports the rigged model as
   `crates/client/assets/characters/<Hero>.glb`.

The game's animations set every bone's rotation (and only the hips'
position), so they play on this skeleton whatever its rest pose and
proportions.

Run from the repository root:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \\
        -P tools/blender/rig_hero.py -- --hero Cross --source MODEL.glb \\
        [--height 1.9] [--faces 50000] [--preview DIR]
"""

import sys
from pathlib import Path

import bpy
from mathutils import Quaternion, Vector

REPO = Path(__file__).resolve().parents[2]
CHARACTERS = REPO / "crates/client/assets/characters"
SKELETON = CHARACTERS / "Superhero_Male_FullBody.gltf"
# The placeholder body, whose skeleton and weights the model borrows.
BODY = "SuperHero_Male"
TEXTURE_SIZE = 2048

ARM_CHAIN = ("upperarm", "lowerarm", "hand")


def objects_added(action):
    before = set(bpy.data.objects)
    action()
    return [ob for ob in bpy.data.objects if ob not in before]


def select_only(*objects):
    bpy.ops.object.select_all(action="DESELECT")
    for ob in objects:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def bounds(ob):
    points = [ob.matrix_world @ v.co for v in ob.data.vertices]
    low = Vector([min(p[i] for p in points) for i in range(3)])
    high = Vector([max(p[i] for p in points) for i in range(3)])
    return low, high, points


def load_model(path, height, faces):
    """The model as one mesh, standing on the ground at `height`, simplified to
    about `faces` faces, with its colors and surface detail baked on."""
    meshes = [ob for ob in objects_added(lambda: bpy.ops.import_scene.gltf(filepath=str(path))) if ob.type == "MESH"]
    select_only(*meshes)
    if len(meshes) > 1:
        bpy.ops.object.join()
    model = bpy.context.view_layer.objects.active
    bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    low, high, _ = bounds(model)
    scale = height / (high.z - low.z)
    model.location = -Vector(((low.x + high.x) / 2, (low.y + high.y) / 2, low.z)) * scale
    model.scale = (scale, scale, scale)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    full = model
    model = full.copy()
    model.data = full.data.copy()
    bpy.context.scene.collection.objects.link(model)
    # Image-to-3D meshes come split apart along every texture seam; weld them
    # into one surface first, so simplifying keeps it whole and the new
    # texture layout gets big islands instead of one per triangle.
    select_only(model)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.remove_doubles(threshold=0.0005)
    bpy.ops.object.mode_set(mode="OBJECT")
    ratio = min(1.0, faces / len(model.data.polygons))
    decimate = model.modifiers.new("Decimate", "DECIMATE")
    decimate.ratio = ratio
    bpy.ops.object.modifier_apply(modifier=decimate.name)
    bpy.ops.object.shade_smooth()
    print(f"model: {len(model.data.polygons)} faces after simplifying by {ratio:.3f}")
    bake_textures(full, model)
    bpy.data.objects.remove(full)
    return model


def bake_textures(full, model):
    """Gives `model` a clean texture layout and bakes `full`'s colors and
    surface detail onto it."""
    select_only(model)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.15, island_margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")

    material = bpy.data.materials.new("MI_Hero")
    material.use_nodes = True
    nodes, links = material.node_tree.nodes, material.node_tree.links
    shader = next(node for node in nodes if node.type == "BSDF_PRINCIPLED")
    shader.inputs["Roughness"].default_value = 0.6
    color = nodes.new("ShaderNodeTexImage")
    color.image = bpy.data.images.new("T_Hero_BaseColor", TEXTURE_SIZE, TEXTURE_SIZE)
    normal = nodes.new("ShaderNodeTexImage")
    normal.image = bpy.data.images.new("T_Hero_Normal", TEXTURE_SIZE, TEXTURE_SIZE)
    normal.image.colorspace_settings.name = "Non-Color"
    model.data.materials.clear()
    model.data.materials.append(material)

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 4
    bake = scene.render.bake
    bake.use_selected_to_active = True
    bake.cage_extrusion = 0.02
    bake.max_ray_distance = 0.08
    bake.margin = 8
    # Bake the raw base color: the full model's color image fed straight to
    # emission. (A "diffuse color" bake comes out black wherever the material
    # is marked metallic.)
    for source in full.data.materials:
        tree = source.node_tree
        full_shader = next(node for node in tree.nodes if node.type == "BSDF_PRINCIPLED")
        base = full_shader.inputs["Base Color"].links[0].from_socket
        tree.links.new(base, full_shader.inputs["Emission Color"])
        full_shader.inputs["Emission Strength"].default_value = 1.0
    select_only(model, full)
    full.select_set(True)
    bpy.context.view_layer.objects.active = model
    for node, kind, extra in ((color, "EMIT", {}), (normal, "NORMAL", {})):
        nodes.active = node
        bpy.ops.object.bake(type=kind, **extra)
        node.image.pack()
        print(f"baked {kind.lower()}")

    links.new(color.outputs["Color"], shader.inputs["Base Color"])
    normal_map = nodes.new("ShaderNodeNormalMap")
    links.new(normal.outputs["Color"], normal_map.inputs["Color"])
    links.new(normal_map.outputs["Normal"], shader.inputs["Normal"])


def load_skeleton(height):
    """The placeholder superhero, scaled to `height`: its armature and body."""
    added = objects_added(lambda: bpy.ops.import_scene.gltf(filepath=str(SKELETON)))
    armature = next(ob for ob in added if ob.type == "ARMATURE")
    body = next(ob for ob in added if ob.name == BODY)
    for ob in added:
        if ob not in (armature, body):
            bpy.data.objects.remove(ob)
    # The file's bone display shape (an icosphere) would export as a mesh.
    for bone in armature.pose.bones:
        bone.custom_shape = None
    for ob in list(bpy.data.objects):
        if ob.type == "MESH" and ob.name.startswith("Icosphere"):
            bpy.data.objects.remove(ob)
    low, high, _ = bounds(body)
    scale = height / (high.z - low.z)
    armature.scale = (scale, scale, scale)
    select_only(armature, body)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return armature, body


def hand_tips(model):
    """The model's outermost points on each side: its fingertips, in the A-pose."""
    _, _, points = bounds(model)
    left = max(points, key=lambda p: p.x)
    right = min(points, key=lambda p: p.x)
    return {"l": left, "r": right}


def fit_arms(armature, tips):
    """Stretches each arm (in the rest T-pose) so it's as long as the model's,
    then points it at the model's fingertips: the A-pose."""
    bones = armature.data
    select_only(armature)
    bpy.ops.object.mode_set(mode="EDIT")
    for side, tip in tips.items():
        shoulder = bones.edit_bones[f"upperarm_{side}"].head.copy()
        # How far the fingers reach from the shoulder, on the skeleton and on the model.
        reach = max((b.tail - shoulder).length for b in bones.edit_bones if b.name.endswith(f"_{side}") and "04_leaf" in b.name)
        stretch = (tip - shoulder).length / reach
        for bone in bones.edit_bones:
            if bone.name.endswith(f"_{side}") and bone.name.split("_")[0] in ("upperarm", "lowerarm", "hand", "index", "middle", "ring", "pinky", "thumb"):
                bone.head = shoulder + (bone.head - shoulder) * stretch
                bone.tail = shoulder + (bone.tail - shoulder) * stretch
        print(f"arm {side}: stretched by {stretch:.3f}")
    bpy.ops.object.mode_set(mode="OBJECT")

    bpy.ops.object.mode_set(mode="POSE")
    for side, tip in tips.items():
        upper = armature.pose.bones[f"upperarm_{side}"]
        rest = upper.bone.matrix_local.to_quaternion()
        along = (upper.bone.tail_local - upper.bone.head_local).normalized()
        turn = along.rotation_difference((tip - upper.bone.head_local).normalized())
        upper.rotation_mode = "QUATERNION"
        upper.rotation_quaternion = rest.inverted() @ turn @ rest
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.context.view_layer.update()


def transfer_weights(body, model):
    """Each model vertex takes the bone weights of the nearest point on the
    posed placeholder body."""
    for group in body.vertex_groups:
        model.vertex_groups.new(name=group.name)
    transfer = model.modifiers.new("Weights", "DATA_TRANSFER")
    transfer.object = body
    transfer.use_vert_data = True
    transfer.data_types_verts = {"VGROUP_WEIGHTS"}
    transfer.vert_mapping = "POLYINTERP_NEAREST"
    transfer.layers_vgroup_select_src = "ALL"
    transfer.layers_vgroup_select_dst = "NAME"
    select_only(model)
    bpy.ops.object.modifier_apply(modifier=transfer.name)


def rig(hero, source, height, faces):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    model = load_model(source, height, faces)
    armature, body = load_skeleton(height)
    fit_arms(armature, hand_tips(model))
    transfer_weights(body, model)
    bpy.data.objects.remove(body)
    # The A-pose becomes the skeleton's rest pose, and the model is bound to it.
    select_only(armature)
    bpy.ops.object.mode_set(mode="POSE")
    bpy.ops.pose.armature_apply(selected=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    model.parent = armature
    model.modifiers.new("Armature", "ARMATURE").object = armature
    model.name = hero
    return armature, model


def export(hero, armature, model):
    output = CHARACTERS / f"{hero}.glb"
    select_only(armature, model)
    bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=True,
        export_animations=False,
        export_image_format="JPEG",
        export_jpeg_quality=88,
    )
    print(f"wrote {output} ({output.stat().st_size / 1e6:.1f} MB)")


def render_preview(armature, directory, hero):
    """The rigged model in a few of the game's animations, side and front."""
    added = objects_added(lambda: bpy.ops.import_scene.gltf(filepath=str(REPO / "crates/client/assets/animations/Mocap.glb")))
    added += objects_added(lambda: bpy.ops.import_scene.gltf(filepath=str(REPO / "crates/client/assets/animations/Volley.glb")))
    for ob in added:
        bpy.data.objects.remove(ob)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "TEXTURE"
    scene.render.resolution_x, scene.render.resolution_y = 300, 420
    camera = bpy.data.objects.new("Camera", bpy.data.cameras.new("Camera"))
    scene.collection.objects.link(camera)
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 2.6
    scene.camera = camera
    armature.animation_data_create()
    Path(directory).mkdir(parents=True, exist_ok=True)
    import numpy as np

    shots = [("Ready", 0.5), ("Sprint", 0.2), ("Spike", 0.32), ("Bump", 0.2), ("Dive", 0.3), ("Airborne", 0.2)]
    rows = []
    for view in (Vector((0.0, -6.0, 1.0)), Vector((6.0, -1.0, 1.1))):
        tiles = []
        for name, seconds in shots:
            action = bpy.data.actions.get(name)
            if action is None:
                continue
            armature.animation_data.action = action
            if action.slots and armature.animation_data.action_slot is None:
                armature.animation_data.action_slot = action.slots[0]
            scene.frame_set(round(seconds * scene.render.fps))
            camera.location = view
            camera.rotation_mode = "QUATERNION"
            camera.rotation_quaternion = (Vector((0.0, 0.0, 0.95)) - view).to_track_quat("-Z", "Y")
            path = str(Path(directory) / "_tile.png")
            scene.render.filepath = path
            bpy.ops.render.render(write_still=True)
            image = bpy.data.images.load(path)
            tiles.append(np.array(image.pixels[:]).reshape(image.size[1], image.size[0], 4))
            bpy.data.images.remove(image)
        rows.append(np.concatenate(tiles, axis=1))
    sheet = np.concatenate(rows[::-1], axis=0)
    out = bpy.data.images.new("sheet", sheet.shape[1], sheet.shape[0], alpha=True)
    out.pixels = sheet.ravel()
    out.filepath_raw = str(Path(directory) / f"{hero}_rigged.png")
    out.file_format = "PNG"
    out.save()
    print(f"preview {out.filepath_raw}")


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    value = lambda flag, default=None: args[args.index(flag) + 1] if flag in args else default
    hero = value("--hero")
    armature, model = rig(hero, Path(value("--source")).expanduser(), float(value("--height", 1.88)), int(value("--faces", 50000)))
    export(hero, armature, model)
    if value("--preview"):
        render_preview(armature, value("--preview"), hero)


main()
