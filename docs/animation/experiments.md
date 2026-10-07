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
  It depends on the machine's state (another heavy program, heat after a
  long session), so compare frame times only between runs made back to back,
  alternating the builds.
- **Hit-stop:** each touch's launch speed and how long the game froze for it.
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

## 05 · Hit-stop that grows with the hit

**Idea.** The game froze for a fixed 30–80 ms on any attack (more for a clean
one), and not at all for serves. In Lethal League the pause grows with the
ball's speed, so harder hits feel heavier; in Smash the hitter shakes through
the freeze, which sells the impact without freezing longer; sets should stay
soft (research.md).

**How.** `feel.rs`: a hit freezes 6 ms for each m/s the ball leaves faster than
10 m/s, capped at 120 ms; attacks scale that by how clean they were
(0.5 + 0.5 × quality). The hitter shakes during the freeze, 5 cm at first and
fading, across the body on the ground and up and down in the air. The bench
now records each touch's launch speed and freeze (`hit_stop` in the JSON).

**Result** (launch speeds are the same in both; freezes per hit):

| Hit | Launch speed | Freeze before | Freeze after |
|---|---|---|---|
| Pass (bump, set) | 8.1 m/s (max 11.5) | none | 1 ms (max 9) |
| Serve | 19.0 m/s | none | 54 ms |
| Spike | 23.1 m/s (max 24.6) | 79 ms | 78 ms (max 85) |
| Banana kick | 25.4 m/s (max 28.3) | 80 ms | 93 ms (max 110) |

Contact accuracy is unchanged within noise. Frame times: the three bench runs
came out slow (28.5 ±8 ms mean), but that was the machine, not the change. Run
interleaved with the previous build, old and new matched (20.0 against
19.1–19.4 ms), both slower than earlier in the session. **Lesson:** compare
frame times only between runs made back to back, alternating builds.

**Decision: keep.** Spikes keep their weight; serves gain an impact; soft
touches stay soft; the hardest kicks hit hardest. How it feels needs a
playtest: does a spike feel heavier than a serve, and a set soft?

## 06 · Inertialization

**Idea.** Every switch into or out of a hit crossfades over 150 ms: both clips
play at partial weight, so a dig or spike starts in a smear of the two poses.
Inertialization (Gears of War 4, Bollo, GDC 2018) cuts to the new clip at
full weight and carries the old pose's difference as an offset that fades out:
the new motion shows from the first frame, and one clip plays at a time.

**How.** `crates/client/src/inertia.rs`: right after Bevy applies the
animation, each bone's offset from the last shown pose is taken at a cut and
faded over the blend time (`1 - smoothstep`), the hair skeleton included.
Gait changes still crossfade, so strides line up. The bench gained a pop
measure: every palm's speed each frame (a pop is an impossible speed), and
`tools/bench.py run A B` now alternates variants run by run, so both share the
machine's state; `NAME:VAR=VALUE` sets an environment variable per variant.

**Result** (06-crossfade against 06-inertia, alternating, same build):

| | Crossfade | Inertialization |
|---|---|---|
| Gap, passes (m) | 0.203 ±0.008 | 0.210 ±0.022 |
| Gap, spikes (m) | 0.088 ±0.021 | 0.125 ±0.034 |
| Gap, banana kicks (m) | 0.238 ±0.022 | 0.285 ±0.035 |
| Frames with a palm over 30 m/s | 68 ±3 | 52 ±11 |
| Frame time (ms) | 18.95 | 18.92 |

## 07 · Inertialization, fading fast

**Idea.** The smooth fade keeps 78% of the old pose 30% of the way through, so
a hit cut into just before contact may not reach its pose in time. A fade that
drops fast at first, `(1 - x)³`, like Holden's dead blending.

**Result** (07-crossfade against 07-inertia-fast): banana kicks still 5 cm
further from the ball (0.264 ±0.004 against 0.211 ±0.017), hands the same
within noise, and frames with a palm over 30 m/s doubled (134 against 72):
the fast start snaps the hands.

**Decision: keep crossfading.** Neither fade beat the crossfade on contact,
and Golazo's kicks got consistently worse with both; the smooth fade only
helped pops a little. The module stays, off, behind `VOLLEY_BLEND=inertia`, to
try again for specific transitions (for instance into digs, which aren't
timed to contact). Why the kicks suffer is open: kicks often switch clip at
the very touch (a low kick turning into a volley), which may be where the
offset matters.

**Found along the way:** in every run some palm moves over 350 m/s in a frame,
with or without inertialization: something teleports a hand (not a rally
reset, which the measure skips). Fixed in 08.

## 08 · Hands popping between strides

**Found.** Logging every frame where a palm moved over 40 m/s, with the clips
that had just started, showed two things:

- The 350–440 m/s jumps were whole bodies moving from the title screen's
  spots to the new match's at the start. Not visible in play; the bench now
  counts them apart, as `teleports`.
- The real pops, 0.6–0.8 m jumps of a hand in one frame, all followed the same
  pattern: a player running near the sprint speed dropped from Sprint to Jog
  and back to Sprint about 0.1 s later. Bevy's `AnimationTransitions::play`
  restarts the clip it switches to, so the sprint still fading out mid-stride
  jumped back to its first frame.

**Fix.** Switching back into a looping gait that's still playing carries on
from where it is; and a gait plays at least 0.25 s before another takes over
(`MIN_GAIT_SECONDS`), so a burst of braking and speeding up doesn't flick
between strides at all.

**Result** (old and new builds alternating, two runs each):

| | Before | After |
|---|---|---|
| Frames with a palm over 30 m/s | 70, 86 | 23, 27 |
| Frames with a palm over 40 m/s | many | 0, 2 |
| Fastest palm, bodies' teleports apart | up to 440 m/s | 39, 50 m/s |
| Contact gap, all touches (m) | 0.167, 0.171 | 0.167, 0.180 |
| Frame time (ms) | 19.3, 19.2 | 19.0, 18.9 |

The few palms still over 40 m/s are most likely swings the game speeds up (up
to 4×) to meet the ball on time, not pops. **Decision: keep.**

## 09 · Legs that run

**Found.** Starting on orientation warping, the bench gained two measures:
how fast planted feet slide (a foot on the ground should stay put) and how
often a running body faces away from where it runs. Planted feet slid at
4.3 m/s on average, at the body's whole speed while sprinting: the legs
weren't running at all. Logging what played while players moved fast on the
ground: sets (650 frames a match), the serve's follow-through as the server
runs in (342), celebrations while walking back (260), all played on the whole
body; and the standing pose for 205 frames, the 0.25 s minimum stride time
from 08 delaying the switch from standing to jogging. Only bumps and sets
could play on the upper body over running legs, and only if the player was
already running when they started.

**Fix.** In `characters.rs`:

- The serve and the cheer can play on the upper body too, and a hit already
  playing moves onto the upper body (keeping its time) as soon as the player
  runs. A serve stays whole through its hit (its hips are part of where the
  hand meets the ball) and moves only for the follow-through.
- The legs blend between strides and in and out, instead of switching in one
  frame: the hips carry the upper body, so a sudden change jumped the hands.
- A stride taking over picks up the outgoing stride's place in its cycle,
  whole body to legs only and back.
- Each one-shot clip has a twin node in the animation graph: starting a hit
  again while it's still fading out plays the twin, instead of restarting the
  fading copy at its first frame (the same jump as 08, for hits).
- The minimum stride time applies to slowing down and to jog/sprint flicks,
  counted from when a stride starts playing; speeding up is immediate.

**Result** (three runs each, alternating with the build before):

| | Before | After |
|---|---|---|
| Planted foot slide (m/s) | 4.27 | 1.67 |
| Planted foot slide, p90 (m/s) | 8.64 | 4.18 |
| Frames with a palm over 40 m/s | 1.3 | 2.0 ±2 |
| Contact gap, all touches (m) | 0.184 ±0.010 | 0.177 ±0.012 |
| Contact gap, spikes (m) | 0.120 ±0.007 | 0.095 ±0.010 |
| Frame time (ms) | 16.76 | 16.74 |

Getting here took four rounds: layering hits mid-play first brought pops
(117 frames over 30 m/s) and worse serves; each was traced with the same
logging to the cause above it. **Decision: keep.**

## 10 · Stride paces from the clips

**Idea.** The code assumes the jog and sprint look right at 4.0 and 7.0 m/s;
the retarget script measured them at 2.9 and 4.9 (about 3.1 and 5.2 on the
heroes). With the measured paces, strides play faster to match the ground.

**Result.** Planted foot slide 1.67 → 1.57 m/s, but the faster cadence pumps
the arms harder (frames with a palm over 30 m/s, 20 → 36) and looks hurried.
**Decision: revert.** What's left of the sliding needs the legs to reach
further rather than step faster: stride warping.

## Next

Ranked in research.md: orientation warping (16% of running frames face more
than 30° away from where the body runs) and stride warping; chaining touch
quality into the spike.
