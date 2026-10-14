# How Knockout City's character animation is built

A study of how Knockout City (Velan Studios, 2021) organizes and times its
character animation, to decide where Volley's animation goes next.

**Sources.**
- A static analysis package from friends of the project: the game's
  character animation tree with its 311 transitions, a list of every
  animation resource, and the tool notes.
- The clip headers (duration, frame rate) of the 489 gameplay clips, read
  from an installed copy of the game.

Nothing was copied into Volley: no clips, curves, models or data, only how
it's organized and how long things last. Earlier lessons from the same tree
are in `docs/unreal-test.md` ("animation lessons from Knockout City").

## Five things that make it look the way it does

### 1. Actions are very short

Every gameplay clip is keyed at **30 fps** (and sampled 8× finer). Whole
actions last a fraction of a second:

| Move | Frames | Seconds |
|---|---|---|
| Throw (straight, lob, curve; any charge) | 19–22 | 0.63–0.73 |
| Pass / fast pass | 22 / 10 | 0.73 / 0.33 |
| Catch attempt / caught / missed | 13 / 15 / 22 | 0.43 / 0.5 / 0.73 |
| Bump | 16 | 0.53 |
| Dash start, end | 20 each | 0.67 each |
| Curling into a ball / out of it | 10 / 10 | 0.33 / 0.33 |
| Spin, flip | 20, 18 | 0.67, 0.6 |
| Hit reaction / skid | 30 / 12 | 1.0 / 0.4 |
| Jump start / airborne loop | 34 / 20 | 1.13 / 0.67 |

Volley's mocap actions run 1–2 s: the spike reaches contact 0.82 s in, the
set 1.05 s in, and both play on long after.

### 2. Into an action in 0.05 s; out after it has finished

Of 311 transitions, 303 are linear blends. Their lengths:

| Transition | Blend |
|---|---|
| Into any throw, pass, catch or dodge | 0.05 s |
| Into a hit reaction | 0.03 s |
| A quick turn | 0.03 s (ease-in) |
| Sprint on or off | 0.08 s |
| Idle, walk and run, between each other | 0.15–0.25 s |
| Back to locomotion after an action | 0.1–0.2 s, but only after a delay (0.42–0.5 s into a pass or throw) or once the clip has finished |
| Ground into the airborne loop | 0.4 s |

Upper-body moves (receiving a pass, a pickup) come in over 0.05 s, hold
0.15 s, and ease out over 0.1 s.

### 3. Every action in each direction

Almost everything has variants by direction relative to facing: 0, 45 and
90 left and right, 135 left and right, 180, and standing still. Some also
come in heights.
- **Catches:** 7 directions, each with attempt, success and miss, on the
  ground and in the air.
- **Passes:** 8 directions, plus fast passes in 8.
- **Pickups:** 8 directions at 3 heights.
- **Bumps:** 5 directions.
- **Jumps:** a start and an airborne loop in 5 directions.
- **Locomotion:** walk and run in 8 directions; sprint only forward and at
  45°.

The right variant plays rather than one clip being turned or warped to fit.

### 4. Dedicated transition clips

Instead of blending between two loops, short authored clips bridge them:
- **Run reversals:** run forward into run backward, left into right, 45°
  into 135°. 6 frames (0.2 s) each.
- **Run to a stop:** 6 frames.
- **Landings:** a 10-frame landing straight into running in each of 9
  directions (including standing), at two impact levels. A separate set
  lands into a sprint.
- **Turns on the spot:** left and right, 15 frames.
- **Others:** a catch into a charged throw, a dash end into idle.

### 5. Hit reactions on a grid

72 hit reactions: 8 directions × 3 heights × 3 strengths (most of them
1 s), on top of the four impact levels the tree picks by speed.

## What a test showed

Experiment 24 (`experiments.md`) tried Knockout City's 0.05 s blend into
actions with Volley's clips, now that dead blending is in. Hand pops went
up 15-fold, mostly in the set, whose clip starts with the hands far above
the head. **Snap-in blends need clips made for them:** short, and starting
close to the pose they're entered from. The look comes mostly from the
clips, not from engine tricks.

## What it means for Volley

Procedural layers and timing tricks (experiments 20-23) took the mocap a
long way. Getting closer to Knockout City now means changing the clips.
The options:

- **Cut the mocap down.** Trim each action to about 0.25 s before contact
  and 0.35 s after, then exit with Knockout City's delayed blend. Add
  mirrored and procedurally turned variants for directions. Cheap, but the
  motion stays realistic rather than cartoony.
- **Hand-key a small stylized set, Knockout City style.**
  - For each touch (bump, set, spike, serve, dig, dive, block, the foot
    moves): about 0.5 s at 30 fps, in 5 directions (0, ±90, 180,
    standing), with 2-3 heights for digs.
  - Plus pivots, stops, landings into running in 5-9 directions, and turns
    on the spot.
  - Roughly 80-120 clips per body type. The most Knockout City-like, but it
    needs an animator or AI animation tools.
- **A mix.** Motion matching on a large locomotion set (the free Game
  Animation Sample) for running, pivots and stops. Short hand-keyed or
  trimmed actions on top.
