"""Runs the game's bench mode (see crates/client/src/bench.rs) several times
and compares runs, for the animation experiments in docs/animation.

    python3 tools/bench.py run NAME [NAME:VAR=VALUE ...] [--runs 3] [--seconds 90] [--arena neon|beach]
    python3 tools/bench.py compare NAME_A NAME_B
    python3 tools/bench.py ab BEFORE AFTER [--runs 3] [--seconds 90] [--arena neon|beach]

Several variants in one `run` alternate (A, B, A, B, ...), so they share the
machine's state; NAME:VAR=VALUE runs a variant with an environment variable
set (e.g. 06-crossfade:VOLLEY_BLEND=crossfade).

`ab` compares the last commit (BEFORE) with the working tree (AFTER): it
builds each once, sets the two programs aside, and alternates runs of them,
so the code can keep changing while it runs (assets are shared, though).

`run` writes docs/animation/bench/NAME-1.json, NAME-2.json, ... The match is the
same every run; differences between runs of one build are measurement noise,
so `compare` shows each run's value, and an effect counts only when it's
bigger than that spread.
"""

import json
import os
import shutil
import tempfile
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "docs/animation/bench"


def run(variants, runs, seconds, arena):
    """Runs each variant `runs` times, alternating between them so they share
    the machine's state (heat, other programs). A variant is NAME, or
    NAME:VAR=VALUE to set an environment variable for it."""
    BENCH.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "-q", "-p", "volley_client"], cwd=ROOT, check=True)
    for i in range(1, runs + 1):
        for variant in variants:
            name, _, setting = variant.partition(":")
            out = BENCH / f"{name}-{i}.json"
            env = dict(os.environ, VOLLEY_BENCH=str(out), VOLLEY_BENCH_SECONDS=str(seconds), VOLLEY_BENCH_ARENA=arena)
            if setting:
                key, _, value = setting.partition("=")
                env[key] = value
            # Keeps the display awake: a sleeping, locked Mac draws nothing.
            subprocess.run(["caffeinate", "-d", "-i", "cargo", "run", "-q", "-p", "volley_client"], cwd=ROOT, env=env, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print(f"wrote {out.relative_to(ROOT)}")


def build_copy(dest):
    subprocess.run(["cargo", "build", "-q", "-p", "volley_client"], cwd=ROOT, check=True)
    shutil.copy(ROOT / "target/debug/volley_client", dest)


def ab(before, after, runs, seconds, arena):
    """The last commit against the working tree, alternating runs."""
    BENCH.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="volley-ab-"))
    changed = subprocess.run(["git", "diff", "--quiet"], cwd=ROOT).returncode != 0
    if changed:
        subprocess.run(["git", "stash", "-q"], cwd=ROOT, check=True)
    # Each side keeps its own copy of the assets too, for changes to clips.
    def snapshot(side):
        build_copy(work / side)
        shutil.copytree(ROOT / "crates/client/assets", work / f"{side}-root/assets")

    try:
        snapshot("before")
    finally:
        if changed:
            subprocess.run(["git", "stash", "pop", "-q"], cwd=ROOT, check=True)
    snapshot("after")
    for i in range(1, runs + 1):
        for name, side in ((before, "before"), (after, "after")):
            program = work / side
            out = BENCH / f"{name}-{i}.json"
            env = dict(os.environ, VOLLEY_BENCH=str(out), VOLLEY_BENCH_SECONDS=str(seconds), VOLLEY_BENCH_ARENA=arena,
                       CARGO_MANIFEST_DIR=str(work / f"{side}-root"))
            for _ in range(2):
                if out.exists():
                    break
                subprocess.run(["caffeinate", "-d", "-i", str(program)], cwd=ROOT, env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print(f"wrote {out.relative_to(ROOT)}")
    shutil.rmtree(work)
    compare(before, after)


def load(name):
    files = sorted(BENCH.glob(f"{name}-*.json"))
    if not files:
        sys.exit(f"no runs named {name}")
    return [json.loads(f.read_text()) for f in files]


def series(runs, path):
    values = []
    for r in runs:
        node = r
        for key in path:
            node = node.get(key) if isinstance(node, dict) else None
        if node is not None:
            values.append(node)
    return values


def describe(values):
    if not values:
        return "-"
    spread = f" ±{(max(values) - min(values)) / 2:.3f}" if len(values) > 1 else ""
    return f"{statistics.mean(values):.3f}{spread}"


def compare(a, b):
    ra, rb = load(a), load(b)
    print(f"{'':28}{a:>22}{b:>22}")
    kinds = sorted(set().union(*(r["gap_m"].keys() for r in ra + rb)))
    for kind in kinds:
        for stat in ("mean", "p50"):
            print(f"{'gap ' + kind + ' ' + stat + ' (m)':28}{describe(series(ra, ['gap_m', kind, stat])):>22}{describe(series(rb, ['gap_m', kind, stat])):>22}")
    clips = sorted(set().union(*(r["swings"].keys() for r in ra + rb)))
    for clip in clips:
        print(f"{'off ' + clip + ' (s)':28}{describe(series(ra, ['swings', clip, 'off_s_mean'])):>22}{describe(series(rb, ['swings', clip, 'off_s_mean'])):>22}")
    for stat in ("p99", "p999", "max", "over_30", "over_40", "teleports"):
        print(f"{'hand speed ' + stat:28}{describe(series(ra, ['hand_speed', stat])):>22}{describe(series(rb, ['hand_speed', stat])):>22}")
    for stat in ("slide_mean", "slide_p90", "sliding_share", "strafing_share", "hips_strafing_share", "body_off_deg", "hips_off_deg"):
        print(f"{'feet ' + stat:28}{describe(series(ra, ['feet', stat])):>22}{describe(series(rb, ['feet', stat])):>22}")
    for stat in ("tilt_mean", "tilt_p99", "folded"):
        print(f"{'posture ' + stat:28}{describe(series(ra, ['posture', stat])):>22}{describe(series(rb, ['posture', stat])):>22}")
    for stat in ("mean", "p95", "p99"):
        print(f"{'frame ' + stat + ' (ms)':28}{describe(series(ra, ['frame_ms', stat])):>22}{describe(series(rb, ['frame_ms', stat])):>22}")


def main():
    args = sys.argv[1:]
    if len(args) >= 2 and args[0] == "run":
        value = lambda flag, default: args[args.index(flag) + 1] if flag in args else default
        flags = {"--runs", "--seconds", "--arena"}
        variants = [a for i, a in enumerate(args[1:], 1) if a not in flags and args[i - 1] not in flags]
        run(variants, int(value("--runs", 3)), int(value("--seconds", 90)), value("--arena", "neon"))
    elif len(args) == 3 and args[0] == "compare":
        compare(args[1], args[2])
    elif len(args) >= 3 and args[0] == "ab":
        value = lambda flag, default: args[args.index(flag) + 1] if flag in args else default
        ab(args[1], args[2], int(value("--runs", 3)), int(value("--seconds", 90)), value("--arena", "neon"))
    else:
        sys.exit(__doc__)


main()
