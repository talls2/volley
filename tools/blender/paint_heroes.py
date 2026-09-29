"""Paints heroes' stand-in looks onto the Quaternius superhero body.

Until each hero has their own model, this dresses the placeholder body in
their kit. Cross (see `docs/concept/cross.webp`): a black jersey with
burnt-orange trims and "00", black shorts with glowing stripes, a compression
sleeve on the right arm, fingerless gloves, glowing sneakers, an orange
headband and a short fade, over dark skin. Golazo (see
`docs/concept/golazo.webp`): a forest-green soccer jersey with gold pinstripes,
V-neck, crest and "10", a captain's armband, white shorts with green and gold
stripes, green socks with gold chevrons, wristbands, boots with glowing soles
and a short fade, over olive skin.

Painting works by where things are on the body, not by hand on the texture:
for every pixel of the texture, the script finds the point on the body (in its
T-pose) that the pixel covers and colors it by region: chest, shorts, right
arm, and so on. The original skin shading is kept and retoned. Glowing parts
also go into an emission texture. Each hero is exported, skeleton and all, as
`crates/client/assets/characters/<Hero>.glb`.

Run from the repository root:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \\
        -P tools/blender/paint_heroes.py -- [--hero NAME] [--preview DIR]
"""

import sys
from pathlib import Path

import bpy
import numpy as np

REPO = Path(__file__).resolve().parents[2]
CHARACTERS = REPO / "crates/client/assets/characters"
SOURCE = CHARACTERS / "Superhero_Male_FullBody.gltf"

BLACK = np.array([0.025, 0.025, 0.03])
FOREST = np.array([0.02, 0.2, 0.09])
GOLD_DIM = np.array([0.55, 0.42, 0.12])
NEON = np.array([0.3, 1.0, 0.35])
GOLD = np.array([1.0, 0.75, 0.2])
ORANGE = np.array([1.0, 0.36, 0.05])
WHITE = np.array([0.85, 0.85, 0.85])
HAIR = np.array([0.03, 0.022, 0.018])
# The texture's skin, averaged.
SKIN_FROM = np.array([0.78, 0.56, 0.43])
GLOW = 1.5

# Body landmarks, in meters: the body faces -y, its right side toward -x.
JERSEY_BOTTOM, JERSEY_TOP = 0.93, 1.53
SHORTS_BOTTOM = 0.56
SOCKS_TOP, SHOES_TOP = 0.32, 0.12
ARM_START = 0.2
WRIST, KNUCKLES = 0.72, 0.84
HAIRLINE_FRONT, HAIRLINE_BACK = 1.765, 1.62
HEADBAND = (1.74, 1.77)


def band(value, center, half):
    return np.abs(value - center) < half


def digit_zero(u, v, center_u, center_v, width, height, stroke):
    """A "0" drawn as a rectangular ring, in a flat (u, v) layout."""
    du, dv = np.abs(u - center_u), np.abs(v - center_v)
    outer = (du < width / 2) & (dv < height / 2)
    inner = (du < width / 2 - stroke) & (dv < height / 2 - stroke)
    return outer & ~inner


def digit_one(u, v, center_u, center_v, height, stroke):
    """A "1": a single upright bar."""
    return (np.abs(u - center_u) < stroke / 2) & (np.abs(v - center_v) < height / 2)


class Canvas:
    """Colors for a batch of body points: base color, glow, and whether each
    is still skin (which keeps the texture's shading)."""

    def __init__(self, n):
        self.base = np.zeros((n, 3))
        self.glow = np.zeros((n, 3))
        self.skin = np.ones(n, dtype=bool)

    def put(self, mask, color, glowing=False):
        self.base[mask] = color
        self.skin[mask] = False
        if glowing:
            self.glow[mask] = color


def paint_cross(p):
    """Cross's kit, for body points `p` (N x 3)."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    ax = np.abs(x)
    canvas = Canvas(len(p))
    put = canvas.put
    base, glow, skin = canvas.base, canvas.glow, canvas.skin

    front = y < 0.01
    arm = ax > ARM_START
    torso = ~arm

    # Shorts: black, a glowing stripe down the outside of each leg, glowing hems.
    shorts = torso & (z > SHORTS_BOTTOM) & (z < JERSEY_BOTTOM + 0.02)
    put(shorts, BLACK)
    put(shorts & (ax > 0.178), ORANGE, glowing=True)
    put(shorts & (z < SHORTS_BOTTOM + 0.018), ORANGE, glowing=True)

    # Jersey: a black tank top with orange trims, side panels, the crossed-arrows
    # emblem and "00".
    jersey = torso & (z >= JERSEY_BOTTOM) & (z < JERSEY_TOP)
    armhole = (ax > 0.155) & (z > 1.3)
    neckline = (z > JERSEY_TOP - 0.06) & (ax < 0.09) & front
    jersey &= ~armhole & ~neckline
    put(jersey, BLACK)
    trim = jersey & (
        (z < JERSEY_BOTTOM + 0.018)
        | ((ax > 0.14) & (z > 1.28))
        | ((z > JERSEY_TOP - 0.08) & (ax < 0.1) & front)
        | band(ax, 0.13, 0.008) & (z < 1.28)
    )
    put(trim, ORANGE, glowing=True)
    arrow = jersey & front & (np.abs(z - 1.36) < 0.035) & (ax < 0.035) & (
        band(x, z - 1.36, 0.007) | band(-x, z - 1.36, 0.007)
    )
    put(arrow, ORANGE)
    for side, (height, width, tall, stroke) in ((True, (1.2, 0.055, 0.085, 0.016)), (False, (1.23, 0.07, 0.12, 0.02))):
        facing = front if side else ~front
        digits = digit_zero(x, z, -width * 0.62, height, width, tall, stroke) | digit_zero(x, z, width * 0.62, height, width, tall, stroke)
        put(jersey & facing & digits, ORANGE)

    # Right arm: a black compression sleeve ringed with glowing circuit bands.
    right_arm = (x < -ARM_START) & (x > -WRIST)
    put(right_arm & (x < -0.26), BLACK)
    put(right_arm & (x < -0.26) & (np.mod(ax, 0.12) < 0.01), ORANGE, glowing=True)
    # Fingerless gloves on both hands.
    gloves = (ax > WRIST) & (ax < KNUCKLES)
    put(gloves, BLACK)
    put(gloves & band(ax, WRIST + 0.01, 0.008), ORANGE, glowing=True)

    # Socks and sneakers, with glowing soles.
    socks = torso & (z > SHOES_TOP) & (z < SOCKS_TOP)
    put(socks, BLACK)
    put(socks & band(z, SOCKS_TOP - 0.035, 0.012), ORANGE)
    shoes = torso & (z <= SHOES_TOP)
    put(shoes, BLACK)
    put(shoes & (y < -0.1), ORANGE)
    put(shoes & (z < 0.03), ORANGE, glowing=True)
    put(shoes & band(z, 0.07, 0.01) & (y > -0.05), WHITE)

    # Head: a short fade with a line shaved into the side, and an orange headband.
    head = torso & (z > 1.6)
    paint_fade(canvas, x, y, z, head)
    headband = head & (z > HEADBAND[0]) & (z < HEADBAND[1])
    put(headband, ORANGE, glowing=True)
    return base, glow, skin


def paint_fade(canvas, x, y, z, head):
    """A short dark fade with a line shaved into the side."""
    ax = np.abs(x)
    hair = head & (((z > HAIRLINE_FRONT) & (y < -0.02)) | ((z > HAIRLINE_BACK) & (y >= -0.02)))
    hair &= ~((z < 1.72) & (ax > 0.07) & (y < 0.05))  # clear of the ears and temples
    canvas.put(hair, HAIR)
    shaved = hair & (ax > 0.06) & band(z - 0.4 * y, 1.705, 0.004)
    canvas.skin[shaved] = True


def paint_golazo(p):
    """Golazo's kit, for body points `p` (N x 3)."""
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    ax = np.abs(x)
    canvas = Canvas(len(p))
    put = canvas.put
    front = y < 0.01
    arm = ax > ARM_START
    torso = ~arm

    # White shorts with green-and-gold side stripes, and a green "10" on the
    # right leg.
    shorts = torso & (z > SHORTS_BOTTOM) & (z < JERSEY_BOTTOM + 0.02)
    put(shorts, WHITE)
    put(shorts & (ax > 0.172), GOLD)
    put(shorts & (ax > 0.18), FOREST)
    leg_number = digit_one(x, z, -0.115, 0.66, 0.06, 0.01) | digit_zero(x, z, -0.08, 0.66, 0.035, 0.06, 0.01)
    put(shorts & front & leg_number, FOREST)

    # Forest-green jersey: short sleeves, fine gold pinstripes, a gold V-neck
    # trim and cuffs, glowing gold side panels, the crest, the captain's
    # armband, and a gold "10" front and back.
    v_neck = front & (z > JERSEY_TOP - 0.13) & (ax < (z - (JERSEY_TOP - 0.13)) * 0.9)
    jersey = torso & (z >= JERSEY_BOTTOM) & (z < JERSEY_TOP) & ~v_neck
    sleeves = arm & (ax < 0.36) & (z > 1.3)
    put(jersey | sleeves, FOREST)
    put(jersey & (np.mod(x + 0.5, 0.05) < 0.004), GOLD_DIM)
    trim = front & (z > JERSEY_TOP - 0.15) & band(ax, (z - (JERSEY_TOP - 0.13)) * 0.9, 0.012)
    put(jersey & trim, GOLD)
    put(sleeves & (ax > 0.34), GOLD)
    put(jersey & band(ax, 0.135, 0.006) & (z < 1.3), GOLD, glowing=True)
    put(sleeves & (x > 0.29) & (x < 0.32), GOLD)
    # From the front the body's right (-x) is on the viewer's left, so the "1"
    # goes there; from behind, the other way round.
    put(jersey & front & (digit_one(x, z, -0.03, 1.2, 0.07, 0.013) | digit_zero(x, z, 0.02, 1.2, 0.04, 0.07, 0.013)), GOLD)
    put(jersey & ~front & (digit_one(x, z, 0.045, 1.24, 0.13, 0.022) | digit_zero(x, z, -0.04, 1.24, 0.075, 0.13, 0.022)), GOLD)
    put(jersey & front & (np.hypot(x - 0.075, z - 1.38) < 0.022), GOLD)

    # Black wristbands.
    put(arm & (ax > WRIST - 0.05) & (ax < WRIST), BLACK)

    # Long green socks with gold chevrons and gold tops, and black boots with
    # glowing neon soles.
    socks = torso & (z > SHOES_TOP) & (z < 0.45)
    put(socks, FOREST)
    put(socks & (z > 0.41), GOLD)
    shin = np.abs(ax - 0.1)
    put(socks & front & (band(z - 1.6 * shin, 0.3, 0.012) | band(z - 1.6 * shin, 0.22, 0.012)), GOLD)
    boots = torso & (z <= SHOES_TOP)
    put(boots, BLACK)
    put(boots & (z < 0.03), NEON, glowing=True)
    put(boots & band(z, 0.06, 0.008), NEON)

    # A short dark fade, like the concept art.
    paint_fade(canvas, x, y, z, torso & (z > 1.6))
    return canvas.base, canvas.glow, canvas.skin


# Each hero: how to paint them, and their skin tone.
HEROES = {
    "Cross": (paint_cross, np.array([0.43, 0.27, 0.17])),
    "Golazo": (paint_golazo, np.array([0.76, 0.56, 0.41])),
}


def rasterize(mesh, world, original, size, paint, skin_to):
    """For every texture pixel a triangle covers, the point on the body it
    shows; returns painted base and glow images and a mask of painted pixels."""
    height, width = size
    base = original.copy()
    glow = np.zeros((height, width, 3))
    covered = np.zeros((height, width), dtype=bool)
    mesh.calc_loop_triangles()
    uv = mesh.uv_layers.active.data
    positions = np.array([world @ v.co for v in mesh.vertices])
    for tri in mesh.loop_triangles:
        uvs = np.array([uv[i].uv for i in tri.loops]) * [width, height]
        pts = positions[list(tri.vertices)]
        lo = np.floor(uvs.min(axis=0)).astype(int)
        hi = np.ceil(uvs.max(axis=0)).astype(int)
        lo = np.clip(lo, 0, [width - 1, height - 1])
        hi = np.clip(hi, 0, [width - 1, height - 1])
        xs, ys = np.meshgrid(np.arange(lo[0], hi[0] + 1) + 0.5, np.arange(lo[1], hi[1] + 1) + 0.5)
        px = np.stack([xs.ravel(), ys.ravel()], axis=1)
        a, b, c = uvs
        m = np.array([b - a, c - a]).T
        if abs(np.linalg.det(m)) < 1e-9:
            continue
        w12 = np.linalg.solve(m, (px - a).T).T
        w = np.column_stack([1 - w12.sum(axis=1), w12])
        inside = (w >= -0.02).all(axis=1)
        if not inside.any():
            continue
        w, px = w[inside], px[inside]
        points = w @ pts
        colors, glows, skin = paint(points)
        cols = px[:, 0].astype(int)
        rows = px[:, 1].astype(int)
        shade = original[rows, cols, :3]
        colors[skin] = shade[skin] * (skin_to / SKIN_FROM)
        base[rows, cols, :3] = colors
        glow[rows, cols] = glows
        covered[rows, cols] = True
    # Bleed painted colors past the islands' edges, so filtering never samples
    # the old texture along a seam.
    for _ in range(6):
        grow = ~covered
        for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            src = np.roll(covered, (dr, dc), axis=(0, 1)) & grow
            base[src] = np.roll(base, (dr, dc), axis=(0, 1))[src]
            glow[src] = np.roll(glow, (dr, dc), axis=(0, 1))[src]
            covered |= src
            grow = ~covered
    return base, glow


def to_image(name, pixels):
    height, width = pixels.shape[:2]
    image = bpy.data.images.new(name, width, height, alpha=True)
    rgba = np.ones((height, width, 4))
    rgba[..., :3] = pixels[..., :3]
    image.pixels = rgba.ravel()
    image.pack()
    return image


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    preview = args[args.index("--preview") + 1] if "--preview" in args else None
    names = [args[args.index("--hero") + 1]] if "--hero" in args else list(HEROES)
    for name in names:
        paint_hero(name, preview)


def paint_hero(name, preview):
    paint, skin_to = HEROES[name]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(SOURCE))
    for ob in list(bpy.data.objects):
        if ob.type == "MESH" and ob.parent is None:
            bpy.data.objects.remove(ob)
    body = bpy.data.objects["SuperHero_Male"]
    material = body.data.materials[0]
    nodes = material.node_tree.nodes
    shader = next(node for node in nodes if node.type == "BSDF_PRINCIPLED")
    # The base color texture: the image node reading the body's color map.
    texture = next(node for node in nodes if node.type == "TEX_IMAGE" and "Dark" in node.image.name)
    source = texture.image
    width, height = source.size
    original = np.array(source.pixels[:]).reshape(height, width, 4)

    base, glow = rasterize(body.data, body.matrix_world, original, (height, width), paint, skin_to)
    texture.image = to_image(f"T_{name}_BaseColor", base)
    emission = nodes.new("ShaderNodeTexImage")
    emission.image = to_image(f"T_{name}_Emission", glow)
    material.node_tree.links.new(emission.outputs["Color"], shader.inputs["Emission Color"])
    shader.inputs["Emission Strength"].default_value = GLOW
    material.name = f"MI_{name}"
    # Eyebrows use the placeholder hair's gray texture (the game tints hair);
    # darken their copy of it.
    brows = bpy.data.objects["Eyebrows"].data.materials[0]
    brows_texture = next(node for node in brows.node_tree.nodes if node.type == "TEX_IMAGE" and "BaseColor" in node.image.name)
    gray = brows_texture.image
    pixels = np.array(gray.pixels[:]).reshape(gray.size[1], gray.size[0], 4)
    pixels[..., :3] *= HAIR / 0.5
    brows_texture.image = to_image(f"T_{name}_Brows", np.clip(pixels, 0.0, 1.0))

    if preview:
        render_preview(preview, name)

    bpy.ops.object.select_all(action="DESELECT")
    for ob in bpy.data.objects:
        ob.select_set(True)
    output = CHARACTERS / f"{name}.glb"
    bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=True,
        export_animations=False,
        # JPEG keeps the file a few megabytes instead of fifteen.
        export_image_format="JPEG",
        export_jpeg_quality=88,
    )
    print(f"wrote {output}")


def render_preview(directory, name):
    """Front, back and head views of the painted body."""
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "TEXTURE"
    scene.render.resolution_x, scene.render.resolution_y = 500, 700
    camera = bpy.data.objects.new("Camera", bpy.data.cameras.new("Camera"))
    scene.collection.objects.link(camera)
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 2.1
    scene.camera = camera
    Path(directory).mkdir(parents=True, exist_ok=True)
    from mathutils import Vector
    hero = name.lower()
    for view, location in (("front", Vector((0.0, -4.0, 0.95))), ("back", Vector((0.0, 4.0, 0.95))), ("head", Vector((1.2, -2.5, 1.7)))):
        camera.location = location
        target = Vector((0.0, 0.0, 1.62 if view == "head" else 0.95))
        camera.data.ortho_scale = 0.6 if view == "head" else 2.1
        camera.rotation_mode = "QUATERNION"
        camera.rotation_quaternion = (target - location).to_track_quat("-Z", "Y")
        scene.render.filepath = str(Path(directory) / f"{hero}_{view}.png")
        bpy.ops.render.render(write_still=True)


main()
