# Animation experiments

Each experiment changes one thing, measures it against the one before on the
same bot match, and records what we decided. Ideas and sources are in
[research.md](research.md); how to run the measurements is in the
[README](README.md).

## How we measure

`python3 tools/bench.py run NAME` plays the same 90-second bot match three
times in bench mode (`crates/client/src/bench.rs`) and writes
`bench/NAME-N.json`; `python3 tools/bench.py compare A B` puts two experiments
side by side. Bots and the simulation are deterministic, so every run has the
same touches; what differs between runs of one build is measurement noise
(when in each frame a touch lands), shown as ±.

- **Contact gap:** at each touch, the distance from the toucher's nearest
  palm (middle knuckle) or toes to the ball's surface, as drawn. 0 is a hand
  on the ball. Reported by kind of hit; `Pass` covers bumps and sets.
- **Swing timing:** for wound-up hits, how far the clip was from its contact
  frame when the ball was touched (clip seconds), and how far the body was
  slid to meet it.
- **Frame time:** mean, p95, p99. macOS keeps vsync on whatever the game asks,
  so the mean sits at 16.7 ms on a 60 Hz display; p95 and p99 show hitches.
  A run made while something heavy runs alongside (Blender) drops to 33 ms.
- **What the numbers miss:** how a pose reads. The gap measures where the
  hand is, not whether the arm looks right, and some correct technique scores
  worse (a set's wrists bend back, which lowers the knuckle it measures). So
  each experiment also gets a visual check: Blender contact sheets
  (`retarget_mocap.py --preview`), and in-game shots with
  `VOLLEY_BENCH_SHOTS=dir`. In-game shots need the screen awake and unlocked;
  a locked Mac draws nothing.

## 01 · Baseline (2026-10-06)

The game as of the source-assets move: the set, serve and spike tracked from
video with DeepMotion, timed swings, root warp, arm IK, pass reach up to
2.15 m, serves launched as the swing reaches the ball.

| Contact gap (m) | mean | median |
|---|---|---|
| All | 0.168 ±0.004 | 0.175 ±0.006 |
| Pass (bump, set) | 0.192 ±0.010 | 0.213 ±0.016 |
| Spike | 0.080 ±0.008 | 0.099 ±0.014 |
| Serve | 0.036 ±0.000 | 0.007 ±0.001 |
| Curve (banana kick) | 0.263 ±0.016 | 0.276 ±0.009 |

Every wound-up swing was timed to its touch, landing within 0.002 s of its
contact frame. Frames: 16.76 ms mean, 20.6 p95, 23.1 p99. Earlier fixes show
here: the serve's palm meets the ball (the swing used to arrive 0.2 s late),
and sets no longer reach over the fingertips.

## 02 · Hand-keyed contact accents on the spike and set

**Idea.** Captures soften the big moments: actors don't really hit, and video
tracking smooths the fastest motion. The tracked spike reached contact with
the arm straight up by the ear and the free arm hanging, the most common way
a spike looks wrong; the tracked set held the hands wide apart, one high and
one low. Studios push mocap with a hand-keyed layer at contact (Sifu,
Assassin's Creed III, Lethal League's hand-picked poses; see research.md).

**How.** `retarget_mocap.py` gained `accent`: a pose solved with the rig's arm
IK and blended over the capture, easing in before the contact frame and out
after, with targets measured from the body each frame so the pose rides along
with the capture. The poses follow volleyball biomechanics:

- **Spike** (contact at 0.82 s): the hitting wrist 0.5 m above, 0.22 m in front
  of and 0.1 m outside the shoulder, elbow out and back (about 130° from the
  side, arm in front of the shoulder line), wrist starting to snap; the free
  arm pulled down across the body. In over 0.12 s, no hold, out over 0.06 s,
  so the capture's whip (14 m/s) follows straight after contact.
- **Set** (contact at 1.05 s): see 03 and 04 for where the hands ended up.

**Result.** The first set pose put the hands at forehead height from the
paper's numbers (27 cm above and 18 cm in front of the forehead for the
ball): passes got clearly worse, 0.192 → 0.228 ±0.010 m. The game's sets meet
balls about 2 m up, and that pose held the palms at 1.78 m against the
capture's 1.86. Spikes didn't change measurably (0.080 → 0.089 ±0.013): the
accent changes how the arm looks, not where the hand meets the ball.

![Spike before (top) and after (bottom)](images/spike-accent.jpg)

At contact (fourth column) the arm now reaches forward and out in front of the
shoulder instead of straight up, and the free arm pulls down across the body.

## 03 · Set window at the captured height

**Idea.** Raising the set pose's hands didn't work: at a window narrower than
the shoulders, these arms are already fully stretched. So the accent shouldn't
choose the height, only the shape: bring the hands together where the capture
holds them (`base="hands"`), wrists bent back 55°, elbows out.

**Result.** Passes 0.221 ±0.004 m: better than 02, still above the baseline.

## 04 · Set ball spot on the new hands

**Idea.** The set's `Swing` ball spot (where the body is slid to so the ball
meets the hands) was still 18 cm in front; the reshaped hands are 8–10 cm in
front, so the ball landed ahead of them. Moved to `spot(-0.09, 0.1, 1.92)`.

**Result.** Passes 0.207 ±0.007 m against the baseline's 0.192 ±0.010: about
the noise. Spikes 0.094 ±0.023, within noise of the baseline. Frame times
unchanged.

![Set before (top) and after (bottom)](images/set-accent.jpg)

**Decision: keep 04.** Both hits now pose like the real technique at contact
at no measurable cost to contact accuracy. What's left of the set's gap comes
from the measurement (bent-back wrists lower the knuckle) and from balls taken
high.

**Still to do:**
- In-game before/after shots, with the screen unlocked:
  `VOLLEY_BENCH=/tmp/b.json VOLLEY_BENCH_SHOTS=/tmp/shots cargo run -p volley_client`.
- Playtest questions: can you tell the moment of contact? spike from set
  before contact?

## Next

Ranked in research.md: hit-stop that grows with the hit and freezes the hitter
and ball (Lethal League); inertialization for snappier cuts into hits;
orientation and stride warping; chaining touch quality into the spike.
