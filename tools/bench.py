"""Runs the game's bench mode (see crates/client/src/bench.rs) several times
and compares runs, for the animation experiments in docs/animation.

    python3 tools/bench.py run NAME [NAME:VAR=VALUE ...] [--runs 3] [--seconds 90] [--arena neon|beach]
    python3 tools/bench.py compare NAME_A NAME_B

Several variants in one `run` alternate (A, B, A, B, ...), so they share the
machine's state; NAME:VAR=VALUE runs a variant with an environment variable
set (e.g. 06-crossfade:VOLLEY_BLEND=crossfade).

`run` writes docs/animation/bench/NAME-1.json, NAME-2.json, ... The match is the
same every run; differences between runs of one build are measurement noise,
so `compare` shows each run's value, and an effect counts only when it's
bigger than that spread.
"""

import json
import os
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
            subprocess.run(["cargo", "run", "-q", "-p", "volley_client"], cwd=ROOT, env=env, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print(f"wrote {out.relative_to(ROOT)}")


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
    else:
        sys.exit(__doc__)


main()
