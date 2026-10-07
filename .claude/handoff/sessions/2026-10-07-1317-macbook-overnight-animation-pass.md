---
date: 2026-10-07 13:17 UTC
device: MacBook
branch: overnight-animation
commit: 56b89f3
topic: Overnight animation pass
---

# Overnight animation pass: AAA feel

## Where things stand
The user asked for an autonomous overnight pass on animation and mechanics
toward a "Triple A" feel. All of it is on branch `overnight-animation`
(25 commits since main's f07da74), pushed, not merged, no PR yet. 58 sim
tests pass. Each change was A/B-benched and written up as experiments 12–19
in docs/animation/experiments.md, with an "Also tonight" list and "Next".

Done:
- Foot locking with two-bone leg IK, and a landing dip at the hips (12).
- Spike follow-through: a second hand-keyed accent in retarget_mocap.py (13).
- The set no longer folds the body over when layered on running legs: it's
  retargeted with level hips (`upright=True`). Serves aren't layered any more;
  running cuts any hit's follow-through 0.1 s past contact (14).
- Mixamo-sourced clips floated 3–15 cm above the floor; `settle` in the
  retarget lowers them. Foot planting is judged by the foot's lowest point
  against its own floor, and not while the animation carries the foot fast (15).
- Stride warping (16), turn steps for held feet (17).
- Late hands: bots' sets and bumps start 0.45 s before the ball arrives; a pass
  on the run ends 0.3 s past contact; idle loops are out of phase (18).
- Jump anticipation for bots, from stepping the sim 8 ticks ahead (19).
- Contact shadows under players, recoil on hard digs, a slump for the side
  that loses a point.
- Instant replays of decisive big moments (crates/client/src/replay.rs): kept
  history replayed at 0.45 speed with a director camera, letterbox and a
  REPLAY tag; any key skips; off in the bench unless VOLLEY_BENCH_REPLAY is set.
- Earlier in the night: camera, ball and crowd react to play (fov punch, ball
  spin and squash, jumping fans), and chain spikes (a clean pass plus a clean
  set make a harder spike).
- Bench additions: film log, VOLLEY_BENCH_CAM=action director camera, posture
  (folded torsos), knees-off share, the plant test above; `bench.py ab` now
  snapshots the assets for each side.

Only on the MacBook: the films in ~/Downloads/volley-films (00-before, 03/04
overnight, 05 before-left/after-right, 06 replay). The assets clone was reset
to empty.

## Decisions
- Handoff notes now live in this repository (`.claude/handoff/sessions/`, on
  the branch the work is on), not in a separate notes repository, which was
  never created. Keep the latest note updated as work progresses, especially
  its next steps.
- Commit as the noreply identity with no Claude co-author trailer; overnight
  work stays on its branch until the user decides to merge.
- Keep a change only if the alternating A/B shows no regression beyond noise;
  write each up honestly, costs included (18 adds ~3 fast-hand frames a match).
- The player's own hands still rise as they press (feedback over looks); only
  bots get late hands and jump anticipation.
- Inertialization stays off (behind VOLLEY_BLEND=inertia).

## Next steps
1. The user reviews the films and decides whether to merge
   `overnight-animation` into main (no PR opened yet).
2. Continuing in a cloud session: the user wants to carry on from here.
3. Remaining snaps: a few frames a match with a hand over 40 m/s, mostly Cross
   airborne as a hit-stop ends around his crossover.
4. Ideas not done: a bump accent; anticipation for the player's own jumps.

## Context
- Run game test and bench runs under `caffeinate -d -i` (macOS). A cloud
  session can't run or film the game; it can build, test and change code.
- Source assets live in private talls2/volley-assets, cloned empty; sparse
  checkout `mixamo mocap` to retarget, then delete and re-clone empty.
- Disk on the MacBook is tight: film frames take about 12 GB for a few films.
- The user likes measured progress, films and infographics to see where
  things are.
