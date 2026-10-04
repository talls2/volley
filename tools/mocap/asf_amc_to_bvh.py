"""Converts CMU motion capture (an ASF skeleton and an AMC motion) to BVH,
which Blender imports.

In ASF/AMC each bone has its own axes (`axis`) and turns about them by the
motion's degrees of freedom; in BVH every joint's rest frame lines up with the
world. So each bone's rotation becomes C * R * C^-1 (C its axes, R its turn),
its offset is its parent's direction times the parent's length, and the result
is the same motion in world-aligned terms.

    python3 tools/mocap/asf_amc_to_bvh.py SKELETON.asf MOTION.amc OUT.bvh [--fps 120]
"""

import math
import sys


def rot(axis, degrees):
    a = math.radians(degrees)
    c, s = math.cos(a), math.sin(a)
    if axis == "x":
        return [[1, 0, 0], [0, c, -s], [0, s, c]]
    if axis == "y":
        return [[c, 0, s], [0, 1, 0], [-s, 0, c]]
    return [[c, -s, 0], [s, c, 0], [0, 0, 1]]


def mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def transpose(a):
    return [[a[j][i] for j in range(3)] for i in range(3)]


def euler_xyz(x, y, z):
    """Rz * Ry * Rx: turn about x first, then y, then z."""
    return mul(rot("z", z), mul(rot("y", y), rot("x", x)))


def to_zyx(m):
    """Angles (z, y, x) in degrees with m = Rz * Ry * Rx."""
    y = math.asin(max(-1.0, min(1.0, -m[2][0])))
    if abs(m[2][0]) < 0.9999:
        x = math.atan2(m[2][1], m[2][2])
        z = math.atan2(m[1][0], m[0][0])
    else:
        x = math.atan2(-m[1][2], m[1][1])
        z = 0.0
    return math.degrees(z), math.degrees(y), math.degrees(x)


def parse_asf(path):
    bones = {"root": {"direction": [0, 0, 0], "length": 0.0, "axis": [0, 0, 0], "dof": ["rx", "ry", "rz"]}}
    children = {}
    section = None
    current = None
    for raw in open(path):
        line = raw.split("#")[0].strip()
        if not line:
            continue
        if line.startswith(":"):
            section = line.split()[0]
            continue
        words = line.split()
        if section == ":root" and words[0] == "axis":
            pass
        if section == ":bonedata":
            if words[0] == "begin":
                current = {"dof": []}
            elif words[0] == "end":
                bones[current["name"]] = current
            elif words[0] == "name":
                current["name"] = words[1]
            elif words[0] == "direction":
                current["direction"] = [float(v) for v in words[1:4]]
            elif words[0] == "length":
                current["length"] = float(words[1])
            elif words[0] == "axis":
                current["axis"] = [float(v) for v in words[1:4]]
            elif words[0] == "dof":
                current["dof"] = words[1:]
        elif section == ":hierarchy" and words[0] not in ("begin", "end"):
            children.setdefault(words[0], []).extend(words[1:])
    for name, bone in bones.items():
        c = euler_xyz(*bone["axis"])
        bone["C"], bone["Cinv"] = c, transpose(c)
    return bones, children


def parse_amc(path):
    frames, current = [], None
    for raw in open(path):
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith(":"):
            continue
        words = line.split()
        if words[0].isdigit() and len(words) == 1:
            current = {}
            frames.append(current)
        elif current is not None:
            current[words[0]] = [float(v) for v in words[1:]]
    return frames


def write_bvh(bones, children, frames, out, fps):
    order = []
    lines = []

    def joint(name, parent, depth):
        pad = "  " * depth
        offset = [0.0, 0.0, 0.0]
        if parent and parent != "root":
            p = bones[parent]
            offset = [d * p["length"] for d in p["direction"]]
        kind = "ROOT" if name == "root" else "JOINT"
        lines.append(f"{pad}{kind} {name}")
        lines.append(f"{pad}{{")
        lines.append(f"{pad}  OFFSET {offset[0]:.6f} {offset[1]:.6f} {offset[2]:.6f}")
        if name == "root":
            lines.append(f"{pad}  CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation")
        else:
            lines.append(f"{pad}  CHANNELS 3 Zrotation Yrotation Xrotation")
        order.append(name)
        kids = children.get(name, [])
        for kid in kids:
            joint(kid, name, depth + 1)
        if not kids:
            b = bones[name]
            end = [d * b["length"] for d in b["direction"]]
            lines.append(f"{pad}  End Site")
            lines.append(f"{pad}  {{")
            lines.append(f"{pad}    OFFSET {end[0]:.6f} {end[1]:.6f} {end[2]:.6f}")
            lines.append(f"{pad}  }}")
        lines.append(f"{pad}}}")

    lines.append("HIERARCHY")
    joint("root", None, 0)
    lines.append("MOTION")
    lines.append(f"Frames: {len(frames)}")
    lines.append(f"Frame Time: {1.0 / fps:.6f}")
    for frame in frames:
        values = []
        for name in order:
            bone = bones[name]
            data = frame.get(name, [])
            if name == "root":
                values += data[:3]
                angles = dict(zip(["rx", "ry", "rz"], data[3:6]))
            else:
                angles = dict(zip(bone["dof"], data))
            r = euler_xyz(angles.get("rx", 0.0), angles.get("ry", 0.0), angles.get("rz", 0.0))
            local = mul(bone["C"], mul(r, bone["Cinv"]))
            values += list(to_zyx(local))
        lines.append(" ".join(f"{v:.5f}" for v in values))
    open(out, "w").write("\n".join(lines) + "\n")


def main():
    args = sys.argv[1:]
    fps = float(args[args.index("--fps") + 1]) if "--fps" in args else 120.0
    asf, amc, out = args[:3]
    bones, children = parse_asf(asf)
    frames = parse_amc(amc)
    write_bvh(bones, children, frames, out, fps)
    print(f"wrote {out}: {len(frames)} frames")


main()
