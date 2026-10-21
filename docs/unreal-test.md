# Unreal test

A time-boxed test of Unreal Engine as Volley's client, to decide whether the
game moves to it or stays on Bevy. Written 2026-10-11, at commit 041f87f.

**Tick numbers below are for the bot match of their entry's date.** The
lineup changed on 2026-10-14 (Alysa joined) and on 2026-10-15 (heroes only),
and every rule change shifts the match too, so the same moments now fall at
other ticks, and older checksums no longer match. To find a moment in today's
match, run `cargo run -p volley_sim_ffi --release --example events [TICKS]`.
In the 2026-10-15 match, for example:

| Moment | Tick |
|---|---|
| Flash Freeze (Blue's Alysa) | 311, frozen 330-360 |
| Banana kick (Red's Golazo) | 582 |
| Crossover (Blue's Cross) | 821 |
| First cracked panel | 1860 |
| Wall skate, then a wall shot (Blue's Alysa) | 4804, 4847 |
| Chilena (Red's Golazo) | 7610 |
| Triple Axel (Red's Alysa), ice burst | 12058, 12177 |

## Decision (2026-10-11)

**Volley moves to Unreal.** After milestones 0 to 5, the user decided to
stick with Unreal and skip milestone 6 (the side-by-side comparison with the
Bevy build). The Unreal client (`talls2/volley-unreal`) is now the game's
client; the Bevy client (`crates/client`) stays as the reference the port
follows, but new client work goes into Unreal. The questions this plan left
for "Unreal wins" are below, under Next.

## Why

The overnight animation pass (docs/animation/experiments.md, 12–19) made real
but small gains. The verdict: "it surely improved but we still have a long way
to go". The bigger levers are better source animation, the look (characters,
lighting, post-processing) and presentation. Those are what Bevy is weakest at
and Unreal is strongest at:

- Motion matching, and Epic's free Game Animation Sample: about 500
  professional mocap clips (locomotion, starts, stops, turns, jumps), in place
  of our handful of generic Mixamo clips.
- Lumen lighting, post-processing, Niagara effects for the powers, Sequencer
  for replays, an IK Retargeter and Control Rig for our own moves.
- Built-in replication, for the online milestone.

Unreal used to cost a lot of manual editor work. Epic now publishes an official
Claude Code plugin (`unreal-engine-skills-for-claude-code`, in the official
plugin marketplace) that drives the running editor through MCP: actors,
materials, Niagara, Control Rig, Sequencer, the Gameplay Ability System,
automation tests. So Claude can do most of the building.

The test answers one question: **does Unreal get Volley visibly closer to the
feel we want, at a cost we can carry?** We answer it by playing the build
and watching it side by side with a film of the Bevy build, not by reasoning
about it.

## Approach: Unreal as a new client, the rules stay in Rust

`crates/sim` is plain Rust with no engine, deterministic at 60 Hz, and its
only dependency is `glam`. Its interface is small: `Sim::new(MatchConfig)`,
`step(&[PlayerInput]) -> Vec<Event>`, and queries for positions, the ball and
the score. The Bevy client contains no rules.

So the test keeps the sim as it is and only replaces the client:

- A new crate, `crates/sim_ffi`, builds the sim as a DLL with a C interface:
  create a match, step it with inputs, read back a snapshot (players, ball,
  phase, score) and the tick's events.
- A small C++ Unreal plugin loads that DLL, steps the sim at 60 Hz, and drives
  everything on screen from its state. Unreal's physics never moves the ball
  or the players.

That way the rules, the bots and the 58 tests carry over unchanged, and the
headless server stays Rust. If Unreal wins, porting the sim to C++ becomes a
separate decision, and we may never need to.

## Scope

In:

- **Beach arena**: sand (our Poly Haven texture), glass walls, the net, sea
  and sky. Lumen, a sun, post-processing.
- **Cross** as the one real hero, from `crates/client/assets/characters/Cross.glb`,
  retargeted to Unreal's mannequin skeleton so the Game Animation Sample's
  clips play on him. The other five players are Unreal's stock mannequins.
- **Locomotion by motion matching**, fed by the sim's positions and velocities.
- **Hits**: our existing clips (`Volley.glb`, `Mocap.glb`) retargeted, timed
  from the sim's hit events, with hands reaching for the ball as in Bevy.
- **A full 3v3 bot match**: the sim already plays the bots. Then the player
  controlling Cross with keyboard, mouse and controller (Enhanced Input mapped
  to `PlayerInput`), with the follow camera and ball cam.
- **One instant replay** of a spike, slowed down, to try Sequencer.
- **Sound**: our existing sounds.

Out: menus and hero select, the HUD beyond the score, Golazo, the Neon Stadium,
abilities and ultimates, networking, packaging.

## Milestones

Each one ends with a film or screenshots that the user looks at before the
next one starts.

0. **Setup.** Install Unreal Engine 5 through the Epic Games Launcher (the user
   signs in to their Epic account), with the Game Animation Sample. Enable the
   ModelContextProtocol and AllToolsets plugins, install Epic's Claude Code
   plugin, and create a C++ project, `volley-unreal`, next to `volley`.
1. **The sim in Unreal.** Build `sim_ffi`, load it, and show the bot match as
   capsules and a sphere. Check: the same inputs give the same events as
   `cargo run -p volley_server`.
2. **The arena and the look.** Sand, walls, net, sea, lighting and
   post-processing, still with capsules.
3. **Cross and locomotion.** Import and retarget Cross. Motion matching for
   every player.
4. **Hits and the ball.** The volleyball clips on the hit events, the ball
   with its trail and shadow, and sounds.
5. **Playing it.** Control of Cross, the cameras, and the replay.
6. **The comparison.** Film the same kind of rally from both builds
   (`VOLLEY_BENCH_CAM=action` in Bevy, a matching camera in Unreal), cut them
   side by side, and play both.

**Time-box: two weeks.** If milestone 3 isn't done by the end of the first
week, stop and reassess with what we have.

## How we work

Following what's worked here and what others report:

- **One feature per step**, each ending in something the user can see.
- **Whoever builds doesn't approve.** A separate reviewer agent only watches
  the films and screenshots against this plan and the Bevy film, and reports
  what's wrong before the user looks.
- **Every finding goes in a file**: a log at the end of this document, with
  what was tried, how long it took, and what was kept, the way
  docs/animation/experiments.md works.
- **Keep track of the cost**: the time each milestone took, Claude plan usage,
  and how much needed the user clicking in the editor rather than Claude
  driving it.

## How we decide

1. **Looks, the user's call**: in the side-by-side films and in play, is
   Unreal clearly better? This decides it. Everything below only has to be
   acceptable.
2. **Feel**: input to motion no slower than in Bevy. 60 fps or more on this PC
   in a full match.
3. **Cost**: could a new hero or a new move be added in Unreal in reasonable
   time, mostly by Claude?
4. **Workflow**: sessions can still hand off work. Cloud sessions can't
   open the Unreal editor, just as they can't run the Bevy client today. Unreal
   runs on the MacBook too, but less well; this Windows PC (RTX 5070 Ti) is
   the main machine for the test.

Then:

- **Unreal wins**: the Unreal client replaces `crates/client`. Decide whether
  the sim stays in Rust behind the DLL or moves to C++, and whether online
  play uses Unreal's replication or the planned renet server.
- **Bevy wins**: bring back what the test taught us (lighting, motion
  matching ideas, how much better source animation helps), and put the next
  effort into the look in Bevy.

## Risks

- **Motion matching without Unreal's character movement.** The sample drives
  it from Unreal's own movement component. Here positions come from the sim,
  so we feed it the sim's trajectory (current velocity plus where the player
  is heading). If that fights us, the fallback is a movement component that
  only follows the sim's position.
- **Retargeting Cross.** He's rigged to our shared skeleton (from the
  Quaternius characters), not Unreal's. The IK Retargeter should handle it;
  his proportions (1.93 m) and the hand poses for hits are the parts to watch.
- **Gaps in the MCP tools.** Some editor work may still need clicking. We
  note each case in the log, since it's part of the cost.
- **Disk.** C: has about 180 GB free after removing Forza. Unreal, the sample
  and a project take a large part of that, so check before installing.

## Open questions

- **Art direction**: decided 2026-10-11, **stylized, like Knockout City**:
  saturated colors, soft light, bold simple shapes rather than photo-real
  detail. Stylized bodies also hide thin animation better. Cross's model and
  materials in milestone 3 follow this.
- **Version control** of the Unreal project: its assets are binary and large,
  so it needs Git LFS, and GitHub's LFS quotas are small. During the test,
  its source, config and docs are in the private `talls2/volley-unreal`.
  Builds, `Saved/` (films included) and any imported assets stay on this PC.
  Decide properly if Unreal wins.
- **Licenses**: the Game Animation Sample's clips may only be used in Unreal
  projects, so they can't come back to Bevy. Mixamo clips can go in the game
  but not be republished raw, as now.
- **Other tools to try along the way**: Hyper3D next to Tripo for the next
  hero's model, and a comparison mode in the bench (our hit timings and ball
  arcs next to reference games and footage), like the attack-pattern
  visualizer others built for their combat.

## Log

### 2026-10-11: milestone 0, setup (Windows PC)

Done in one session, except the Game Animation Sample, which comes with
milestone 3.

- **Installed**: Epic Games Launcher, Unreal Engine 5.8.3 (by the user, signed
  in to Epic), Visual Studio 2022 17.14 with the C++ game, C++ desktop and .NET
  desktop workloads (MSVC 14.44, Windows SDK 10.0.26100), and Rust 1.99. The
  sim's 58 tests pass on this machine.
- **Disk**: C: had 12 GB free. Removing a 135 GB game made room; about 137 GB
  was left after the engine.
- **Project**: `volley-unreal`, next to `volley`, made from the engine's blank
  C++ template (`Templates/TP_Blank`) with the template's renames applied by
  hand. Plugins: ModelContextProtocol, AllToolsets, PoseSearch, Chooser,
  MotionWarping, AnimationWarping. The editor target builds in about 40 s.
  Its source is in the private `talls2/volley-unreal`, cloned next to
  `volley` (Open questions).
- **MCP**: set up as in Epic's plugin's `references/setup.md`.
  `bAutoStartServer=True` goes in the per-user
  `Saved/Config/WindowsEditor/EditorPerProjectUserSettings.ini`, and a
  hand-written `.mcp.json` next to the `.uproject` points at
  `http://127.0.0.1:8000/mcp`. The server answers about 25 s after the editor
  starts. Epic's Claude Code plugin is enabled in the project's
  `.claude/settings.json`, which offers to install it when a session opens
  there.

What to know:

- **Claude drives the editor only from a session opened in `volley-unreal`**,
  with the editor running. `.mcp.json` and the plugin load at session start.
- **The MCP server listens only on 127.0.0.1**, so it needs no firewall rule.
  On first launch Windows asks to let the editor listen on the network (port
  1985). That isn't needed for the test, so the prompt can be cancelled; the
  firewall stays on.
- The editor's log says what goes through MCP is "Licensed Technology" under
  the Unreal EULA (section 6(e)): the LLM provider must not train on it.
- Calling `UnrealBuildTool.exe` directly fails without a system-wide .NET 10.
  `Engine/Build/BatchFiles/Build.bat` uses the engine's own copy and works,
  for builds and for `-projectfiles`.
- The blank template logs a GameFeatureData error at startup (no asset
  manager rule for game feature plugins). It's harmless for now.

Cost: mostly waiting on downloads. The user did the Epic sign-in, the engine
install and the disk cleanup. Claude did the rest, with no clicking in the
editor.

### 2026-10-11: milestone 1, the sim in Unreal

The bot match plays inside Unreal and gives **the same match tick for tick**
as plain Rust. Every checksum over a minute of play is identical.

- **`crates/sim_ffi`** (new): the sim as `volley_sim_ffi.dll` with a C
  interface. It creates the game's lineup (Red three All-rounders; Blue Cross,
  Golazo and an All-rounder), steps it with bots and optionally one human
  input, and reads back the match, the players and the tick's events. The
  header is `include/volley_sim.h`. Tests check that the C interface plays the
  same match as the sim used directly, and that the struct sizes match the
  ones the Unreal module `static_assert`s. The `checksum` example plays the
  same match in Rust and prints a fingerprint every 10 s of play:
  `cargo run -p volley_sim_ffi --release --example checksum`.
- **`volley-unreal`**: the game module delay-loads the DLL at startup.
  `VolleyUnreal.Build.cs` links it from `../volley/target/release` and copies
  it next to the module, so build it there first. `AVolleyMatchActor` steps the
  sim at a fixed 60 Hz and draws positions blended between the last two ticks.
  It builds a greybox from the engine's basic shapes: sand, low edges where the
  walls are, the net, team-coloured capsules, the ball, the decoy and the
  landing marker. A broadcast camera looks across the court, and the score
  shows on screen. `AVolleyGameMode` (no pawn) spawns it on the engine's
  `Template_Default` map. Sim to Unreal: x stays x, sim z becomes y, sim y
  becomes z, and meters become centimeters (which also turns right-handed into
  left-handed).
- **Bench settings**, like the Bevy bench: `VOLLEY_SHOTS=DIR` saves a
  screenshot every `VOLLEY_SHOT_EVERY` ticks between `VOLLEY_SHOT_FROM` and
  `VOLLEY_SHOT_TO`, and `VOLLEY_QUIT_AT` quits at that tick. For a film, run
  with `-game -benchmark -fps=30` and `VOLLEY_SHOT_EVERY=2`: every frame is
  then two ticks, and none are skipped while saving.
- **Check**: 3600 ticks in Unreal against the `checksum` example. All six
  fingerprints (ticks 600 to 3600), scores and touch counts are identical.
- **Film**: `volley-unreal/Saved/Films/milestone1-greybox.mp4`, a 14 s rally
  (ticks 400 to 1250).

Notes:

- Rebuilding C++ needs the editor closed (Live Coding blocks
  `Build.bat`). An editor build takes about 16 s.
- In `-game` mode, the Epic toolset plugins log Python errors at startup
  (their editor-only Python modules). They're harmless.
- The engine's auto exposure makes the sand look washed out. That's for
  milestone 2.

Cost: one session. Everything was done by Claude from code and the
command line. The editor wasn't opened and nothing was clicked.

### 2026-10-11: milestone 2, the arena and the look

The Beach is in Unreal, laid out like the Bevy one (`arena.rs`) and styled
like Knockout City (the user's choice).

- **Generated assets**: `Tools/make_assets.py`, run in a headless editor
  (`UnrealEditor-Cmd -run=pythonscript`), imports our Poly Haven sand and
  builds the materials node by node. It also makes an empty `Arena` map. The
  output (`Content/Volley`) isn't committed: the script recreates it. The
  materials:
  - `M_Stylized`: a color with a soft rim of that color, plus optional glow.
    Used for players, ball, frames, pads, straps.
  - `M_Sand`: the photo texture pulled 45% toward a flat warm tone, with
    softened bumps.
  - `M_Glass`: nearly clear head-on, tinted and glowing at the edges.
  - `M_Water`: turquoise by the beach to deep blue farther out, with two
    layers of panning waves from an engine normal map.
  - `M_Net`: a masked 10 cm cord grid.
- **`AVolleyArena`** spawns the sky and light itself:
  - a warm sun, high as in Bevy
  - sky atmosphere, at 2× its physical brightness for the sunny look
  - volumetric clouds
  - live sky light and sea haze
  - post-processing: fixed exposure at -1 (the washed-out look is gone),
    saturation +18%, a little contrast, bloom and vignette

  It also builds the geometry: 160 m of sand, the sea, 8 m glass walls with
  frame posts every 4 m and a top rail, team-coloured pads, blue straps, and
  the net with its top band, posts and pads. Lumen (software) is on.
- **Tuning without rebuilding**: `VOLLEY_EXPOSURE`, `VOLLEY_SUN` and
  `VOLLEY_SKY`. Exposure was chosen from side-by-side shots at -1, -2 and -3.
  `VOLLEY_CAM=orbit` circles the arena, 24 s a turn.
- **Films** in `volley-unreal/Saved/Films`: `milestone2-orbit.mp4` (20 s),
  and `milestone2-broadcast.mp4`, the same rally as milestone 1.

To fix later: from some angles a dark band shows on the sea near the horizon.
The camera can see the edge of the square sand, and the sea has no shoreline
or foam. The players are still capsules (milestone 3).

Cost: one session, all from code, the command line and a Python script. The
editor wasn't opened.

### 2026-10-11: milestone 3, first pass: the Rust version's characters

The user asked to build on what the Rust version already has rather than
Epic's Game Animation Sample. Unreal now plays volley's own bodies and clips,
animated by a C++ port of `crates/client/src/characters.rs`.

- **Import** (`Tools/make_assets.py`): the bodies (Cross, Golazo, the
  Quaternius male and female All-rounders, their two hair meshes) and the
  clips in `Mocap.glb` and `Volley.glb`, all on one Unreal skeleton.
  - Volley's shared skeleton already uses the Unreal mannequin's bone names
    (it's Quaternius's Unreal-compatible library), so nothing needed renaming.
  - Unreal's glTF importer only finds a skeleton through a mesh, and the clip
    files have none. `Tools/gltf_anim_helper.py` writes a copy with a 1 mm
    triangle skinned to the root.
  - Interchange ignores a pipeline object passed as an import task's options.
    It takes one as a path in `ImportAssetParameters.override_pipelines`.
  - Interchange imports don't save on their own, so the script saves the
    folders after importing.
- **Proportions**: the clips carry translations for all 65 bones, at the
  proportions of the skeleton they were made on. The bodies differ: Cross's
  legs are 10–14% longer, and the female body's upper arms are shorter. Played
  as they are, the clips reshape every body to the clips' proportions (the
  Bevy client probably does this today). `UVolleyAnimInstance` keeps each
  body's own bone lengths, takes only rotations from the clips, and scales the
  hips' height by the body's.
- **`UVolleyAnimInstance`**: a native clip player, with no Blueprint graph.
  It holds any number of crossfading clips with their own time and rate,
  samples them in its proxy, and blends them with
  `FAnimationRuntime::BlendPosesTogether`.
- **`FVolleyCharacter`** ports characters.rs, keeping its names and constants
  (with line references):
  - looks per kit, and hair tint
  - gait with hysteresis and the minimum gait time, stride-matched rates,
    idle desync
  - takeoff, airborne and landing; dash, knockdown, celebration, block, dive,
    foot save
  - the serve held until it's swung
  - hit clip choice per move and per touch; wind-up timed to the predicted
    contact; per-frame swing timing; technique switching; late hands for bots
  - Golazo's feet swaps, follow-through cuts, finish rules, the crossover's
    side, and facing with its priorities and turn rate
- **`sim_ffi`** now exposes per player what that logic reads: kit; the
  current move, its phase, start and direction; the active move; blocking,
  grounded; dash and carry; must-not-touch; the Feet and NoLook passives;
  predicted contact (ticks and spot, a second ahead); and the attack
  technique. `VolleyPlayerState` is 108 bytes; the size checks on both sides
  moved with it. The checksums still match the Rust run.
- **Camera**: `VOLLEY_CAM=ball` follows the ball from inside the cage, close
  enough to judge animation. `VOLLEY_EYE`/`VOLLEY_LOOK` place the camera by
  hand. Motion blur is off (Knockout City is crisp, and it smeared films).
- **Film**: `volley-unreal/Saved/Films/milestone3-characters-v1.mp4` (20 s,
  ball camera).

Not done yet in milestone 3:

- **Motion matching** with the Mixamo locomotion in `volley-assets` (starts,
  stops, turns, strafes, backpedal), retargeted in Unreal: the plan's key
  experiment.

### 2026-10-11: milestone 3, the team-coloured stylized look

- **Why the bodies were grey**: Interchange's imported materials lack the
  "used with skeletal mesh" flag, so Unreal replaced them with its default
  grey. Nothing from the textures was showing.
- **`M_Character`**, generated by the asset script, now goes on every body,
  hair and eye slot:
  - The body's own texture, a little more saturated.
  - A rim of team color on everyone, so both teams read at a glance.
  - For the All-rounders, whose bodies come in swimwear, a kit painted by
    body region: a team jersey, darker team shorts and white shoes.
  - The regions come from the rest pose (`PreSkinnedPosition`, passed through
    a vertex interpolator because it's vertex-only), as fractions of each
    body's height. The kit stays put whatever the animation does.
  - Cross and Golazo keep their concept-art outfits.
- **Film**: `volley-unreal/Saved/Films/milestone3-team-look.mp4` (20 s, ball
  camera).

To improve: at a distance, Blue's heroes (Cross dark, Golazo green) read
less clearly as Blue than the All-rounders do. Options: a stronger rim, or a
team-coloured accent on their outfits (shorts, shoes, armbands).

### 2026-10-11: milestone 3, characters.rs's procedural layers

All of characters.rs's procedural layers are ported, with its constants (line
references in the code).

- **`FVolleyCharacter`** decides the goals and weights each frame, in world
  space:
  - lean into acceleration
  - hips turned toward the run
  - the body warp so the swing's contact spot meets the real ball
  - landing dip, jump gather, recoil on hard balls, slump after a lost point
  - stride warp
  - when feet may lock
  - head look, arm reach (both arms, or only the right for a spike), and
    the leg's reach for foot saves and kicks
  - bumps, sets and cheers moving onto the upper body once the player runs
    (`UpperBody`)
- **`UVolleyAnimInstance`** applies them after blending, in Bevy's order:
  - A legs layer, a looping gait taking the root, hips and legs under a
    layered hit.
  - Local edits: hips down and turned (spine turned back), back and neck
    bowed.
  - Component-space edits, through a small forward-kinematics helper:
    stride warping and foot locking (with each foot's memory kept in the
    proxy), both solved with Bevy's two-bone leg solver; then the head turn,
    and `point_limb` for arms and leg.
- **Hit-stop** (`AVolleyMatchActor::HitStop`): as feel.rs, the game clock
  slows to 5% for a beat on hard hits, stuff blocks, catches, posterizers and
  splits, and the hitter shakes through it. It's timed on real time, through
  Unreal's global time dilation. The camera shake isn't ported.
- **`sim_ffi`**:
  - `takeoff_ticks` per player: the match stepped 8 ticks ahead on a copy
    with the bots' inputs, as Bevy's `look_ahead`, for the jump gather.
  - The ball's velocity, for the hit-stop and recoil.
  - The checksums still match the Rust run.
- **Fixed**: a member `FaceSeconds` shadowed the constant of the same name,
  so the face-this-way overrides (where a hit goes, a dash, a dive) never
  took effect. It was in since the first pass.
- **Film**: `volley-unreal/Saved/Films/milestone3-procedural.mp4` (20 s, ball
  camera). Hit-stops don't show in films: frames are taken every two sim
  ticks, and a freeze slows the ticks.


### 2026-10-11: milestone 3, motion matching

Locomotion is motion matched. The matcher is ours: a database built by our
tools and searched in C++. It isn't Unreal's Pose Search, which needs an
animation Blueprint graph or a Pose History node that this code-only client
doesn't have. `VOLLEY_MM=0` brings back the picking by speed, as in Bevy.

- **Data**: the locomotion takes in `volley-assets/mixamo`: run, sprint,
  start, stop, 180° turn, change of direction, backpedal, two strafes, two
  sidesteps and the goalkeeper ready stance.
  - They're retargeted by volley's own `tools/blender/retarget_mocap.py`,
    with a new `--locomotion PATH` run (the `LOCOMOTION` set,
    `root_motion=True`). It keeps the capture's turning and the hips' travel
    and writes `volley-unreal/Assets/Locomotion.glb`. Blender 5.2 was
    already on this PC.
  - The game's own `Mocap.glb` is untouched.
  - Mixamo's "Offensive Idle" was tried and dropped: matching chose it over
    the goalkeeper crouch, which stood players upright where the game had
    them crouched.
- **Database** (`Tools/locomotion_db.py`, standard library only):
  - Each frame's root is the hips' travel and a smoothed heading from the hip
    joints. The stride's hip swing stays in the pose.
  - The clips are rewritten in place and facing forward for playback
    (`LocomotionInPlace.glb`), and the junk Mixamo actions are dropped.
  - 18 features per frame, in the body's terms: velocity, the spot 0.2 /
    0.4 / 0.6 s ahead, the facing at 0.6 s, the feet's positions and
    velocities.
  - The body's speed comes from the planted foot sliding back under it, so
    the takes downloaded "in place" (run, sprint, turn, change of direction)
    work like the ones that travel.
  - 12 clips, 417 frames.
- **Query**: velocity, plus each player's real future: `sim_ffi` now steps
  its look-ahead copy 36 ticks and gives where each player will be in 0.2,
  0.4 and 0.6 s. The bots are deterministic, so this is exact. A typical game
  has to predict it from input.
- **Search** (`FVolleyMotionMatcher`):
  - Brute force over all frames, every 0.1 s, each feature normalized by its
    spread and weighted: trajectory 1.5, velocity and facing 1, feet 0.6 and
    0.4.
  - It keeps what's playing unless another frame is 25% better, and holds
    each pick at least 0.25 s.
  - One-shots aren't entered in their last 0.2 s.
  - Changes crossfade over 0.2 s, and the clip's rate follows the sim's speed
    (0.8–1.3×).
- **Measured** over a minute of the bot match: 510 picks across 6 players.
  - By clip: Sprint 109, Turn 99, Stop 60, Cut 54, Ready 43, Strafe_Left 37,
    Start 33, Sidestep_Right 32, Jog 21, Backpedal 17, Strafe_Right 4,
    Sidestep_Left 1.
  - The left/right imbalance comes from the clips' speeds: Mixamo's
    Strafe_Right is slower than Sidestep_Right, and the reverse on the left.
    It isn't a sign error; the database's signs check out.
  - Before the hysteresis was raised from 15% to 25% and the 0.25 s hold
    added, there were 666 picks.
  - The checksums still match the Rust run.
- **Films**: `milestone3-motion-matching-ab.mp4` (the same 20 s rally, by
  speed on the left, motion matched on the right) and
  `milestone3-motion-matching.mp4`.

Limits of this experiment:

- The data is thin: Mixamo takes are short single cycles, with one turn and
  one stop. Motion matching shines with long takes of dancing-around
  movement (Epic's sample has about 500 clips). So this mostly shows the
  pipeline works, not how good it can get.
- The bots change direction all the time, so Turn and Cut are picked a lot.
  The body's facing still comes from the game's turn rule, not from the
  clips.

### 2026-10-11: milestone 4, hits, the ball and sound

The hits already played on the sim's events (milestone 3). This milestone
adds the ball and what helps you read it, and sound, both ported from the
Bevy client with its constants.

- **The ball** (`FVolleyBallView`):
  - Made by `make_assets.py`: a UV sphere of the sim's radius, with the Bevy
    client's six-panel texture generated as a PNG. It wears `M_Character`.
  - It spins with speed (15% of rolling, at most 28 rad/s), stretches with
    speed, and squashes on every touch.
  - After a rally it bounces and rolls to a stop on its own
    (`start_bounce`).
  - The decoy looks the same.
- **Helpers on the floor**:
  - The ball's shadow shrinks and fades as it rises.
  - Blob shadows under the players.
  - Three landing rings in the color of the half the ball falls on, lighter.
    Two are fixed (0.5 and 0.25 m); the third closes in from 2.5 m over the
    last 1.5 s. Time to landing comes from the ball's height and vertical
    speed.
  - New materials `M_Shadow` and `M_Ring` (unlit, radial).
- **Trail**: a camera-facing ribbon, a procedural mesh with the additive
  `M_Trail`, over the ball's last 0.35 s.
  - Colored by the last hit as in trail.rs: chain spikes wider and
    near-white, blocks violet.
  - Cleared on new hits, the net, new rallies, and while a decoy flies.
- **Sand puff**: 40 instanced grains where the ball lands, as effects.rs.
- **Sound** (`FVolleyAudio`):
  - All 27 of volley's sound files, imported as they are (OGG and FLAC).
  - audio.rs's table, event by event, with its volumes and round-robin
    variants: spatial at the ball or player, crowd and whistle in 2D.
  - Footsteps every 1.3 m run and on landing.
  - The crowd's murmur on a loop, and a wave every 3.5–7 s.
- **Films with sound**:
  - Films are made of screenshots, so they're silent.
  - With `VOLLEY_SOUND_LOG=FILE`, the game writes each sound: sim time,
    sound, volume as heard from the camera, and pan.
  - `Tools/film_audio.py` mixes them onto the film with ffmpeg at their exact
    frames.
- **Film**: `volley-unreal/Saved/Films/milestone4-ball-and-sound.mp4` (20 s,
  ball camera, 323 sounds).

Not ported, as it's HUD and camera (milestone 5): the off-screen ball marker
and the camera shake.

### 2026-10-11: the Bevy camera

At the user's request, the camera is now the Bevy client's (camera.rs),
ported with its constants. Before, it was a broadcast view beside the court.

- 7 m behind the player, looking at a point 2.6 m above the feet. It
  follows 35% of a jump.
- Each rally starts facing the net.
- **Ball cam**: it swings round toward the ball at 6/s and tips down for
  high balls, ignoring a ball within 1.5 m. With a decoy flying, it looks
  between the two balls.
- It stays inside the side walls, coming in closer and rising for what it
  loses, with 4 m of room behind the end walls.
- The lens is Bevy's 0.82 rad vertical field of view (converted to
  Unreal's horizontal), wider at a sprint and punched by hard hits.
- feel.rs's camera shake is now in too: trauma from hits, stuff blocks,
  catches, posterizers, splits, chains and hard landings, squared, on real
  time.
- It follows Red's first player, the one the user will control in
  milestone 5; mouse and stick turning come then.
- `VOLLEY_CAM=broadcast` brings back the old view.
- **Film**: `volley-unreal/Saved/Films/milestone4-shoulder-camera.mp4`
  (20 s, with sound).

Seen in the film: when ball cam turns round for a ball behind a player at
the net, the camera sits just across it and the net fills the view for a
moment. Bevy's rig does the same, but its net is thinner and see-through.

### 2026-10-11: the Neon Stadium, and the camera on the player's side

The user wants the futuristic, Rocket League-like arena. The README pitches
Volley that way, and the Bevy build has one: the Neon Stadium. It's now
ported (`VolleyArenaNeon.cpp`) and the default. `VOLLEY_ARENA=beach` keeps
the Beach.

- From arena.rs's `neon`, with its constants:
  - **Floor:** a dark glossy court with a glowing cyan grid. The grid square
    is generated as a PNG by the asset script, with the `M_Grid` material.
    Bright white court lines, and a kickoff ring in each half.
  - **Walls:** faint blue energy fields (`M_Glass`, tinted) with a white top
    rail and cyan ribs. A frame of light on each end wall, like a goal, and
    white corner pillars.
  - **Net:** a net of light, with a bright band along the top.
  - **Stands:** seven rows on all four sides, with about 1,500 fans as
    instanced meshes in six shirt colors, a few holding up phone lights. They
    jump for points, posterizers, stuff blocks, splits and chains, each fan
    on their own rhythm, as `cheer`.
  - **Around:** light towers, 220 stars, and two rings of light turning over
    the net (a torus mesh generated by the asset script).
- **Lighting**, scaled like the Beach's (Bevy's value × 10/11000): a cool key
  light, two washes from the ends, dark haze, more bloom.
  - The key light is raised from 3.2 to 6, and the fans' shirts brightened,
    so the crowd reads as in Rocket League's stands.
  - The floor's roughness went from Bevy's 0.15 to 0.3: Lumen's reflections
    of the lights speckled at 0.15.
- **Camera**: kept on the player's side of the net (0.6 m from it), pulled in
  and raised like at the walls, at the user's request. Bevy's camera can
  swing across.
- **Film**: `volley-unreal/Saved/Films/neon-stadium.mp4` (20 s, with sound).

Changed afterwards, at the user's request: **the floor mustn't draw the eye
from the ball**.
- The glowing grid and kickoff rings are gone: the court is plain, dark and
  glossy.
- The court lines are dim.
- The goal frames on the end walls are gone.
- Both halves stay lit the same, with no team colors on the floor.

Film: `volley-unreal/Saved/Films/neon-stadium-calm.mp4`.

### 2026-10-11: milestone 5, playing it

A person can play Red's first player. `Tools/Play.bat` in `volley-unreal`
starts the game, and `Play.bat cross` or `Play.bat golazo` plays a hero, as
the Bevy hero select does.

- **Controls** (`FVolleyControls`), ported from input.rs:
  - The same keyboard and controller bindings.
  - Presses are kept until the next sim tick, so none is lost or counted
    twice.
  - Pass and attack charge while held (full in 0.5 s), and a serve goes on
    release.
  - Movement and aim are relative to the camera: hits go the way you move,
    or the way you look standing still.
  - Keys are read straight from the player controller (no Enhanced Input
    assets), so it stays in code.
  - The help line follows the device used last.
- **Camera**: turned by the mouse (click to capture, Esc frees it) or the
  right stick, with camera.rs's speeds. Turning by hand takes it off ball
  cam; Tab or right stick click toggles ball cam.
- **Also**: `P` pauses; `1` hands your player to a bot and back.
  `sim_ffi`'s `volley_match_new_as(kit)` sets your player's hero. Your
  passes don't get the bots' "late hands" wait, as in characters.rs.
- **Instant replays** (`FVolleyReplay`, `AVolleyHUD`), ported from
  replay.rs:
  - Every tick's state and events are kept for 4 s.
  - A point won by a big moment is replayed 0.9 s later, from 70 ticks before
    the moment to 30 after, at 0.45 speed. Big moments are chain spikes,
    posterizers, stuff blocks, volley and bicycle kicks, and spikes over
    20 m/s every third point.
  - The replay is fed back through the same code as live play, so
    characters, ball, trail, sounds and hit-stops all play again.
  - The camera films from beside whoever made the moment, then follows the
    ball. Letterbox bars and a blinking REPLAY tag show; any key skips.
  - The live match waits and resumes exactly: later points fall on the same
    ticks.
  - Replays are on when a person plays (`VOLLEY_REPLAY=1` while filming).
- **Films and benches** still have a bot play your player, so they stay the
  same every run (checksums unchanged).
- **Film**: `volley-unreal/Saved/Films/milestone5-replay.mp4`, a replay of
  Cross's winning spike. It's silent: film_audio.py places sounds by sim
  time, which a replay rewinds.

Not checked by Claude: how it feels to play. Only the user can judge that,
by playing.

### 2026-10-11: the decision, and what's next

The user skipped milestone 6 and chose Unreal. Open, for the user to decide:

1. **Where the rules live.** They now run in Rust behind the DLL
   (`crates/sim_ffi`), with the 58 tests, the bots and the headless server.
   This has worked through every milestone: deterministic, matching
   checksums, and Unreal reads anything it needs from it. Keeping it is the
   recommendation; porting to C++ only pays if the build or online play
   needs it.
2. **Online play.** Either Unreal's replication, or the planned renet server
   running the same sim, with clients sending inputs. A deterministic sim
   already in both suits the second, and lockstep or rollback suit an arena
   game with this few players.
3. **What carries over next** from the Bevy client: the HUD (score, aim
   marker, off-screen ball pointer, callouts), the title, hero select and
   match-over screens, and hero abilities' looks.
4. **Repositories and docs.** The README and CLAUDE.md in `volley` still
   describe the Bevy client as the game. Decide whether `volley-unreal`
   stays separate or moves into `volley`, then update them. Binary assets
   (once there are any not made by script) need Git LFS.

### 2026-10-11: the HUD

The Bevy client's HUD (hud.rs, aim.rs) is ported, drawn on Unreal's canvas
by `AVolleyHUD` (no UMG assets) with hud.rs's sizes and text shadow:

- Score and set ("Set 1 (to 21) | sets 0 : 0"), with PAUSED when paused.
- Callouts:
  - points with their reason, held until the next rally, then sides
    switching and sets won
  - your attacks' contact, from "perfect!" to "scrambled"
  - dribbles, chain spikes, crossovers, posterizers, chilenas, decoys
    fooling someone, blocks
- The "Foot save!" and "Dive!" prompts, with the right key for your device.
- The hero's ability and ultimate: ready, cooldown, charge, gold when the
  ultimate is ready.
- Hero names over everyone, in team colors.
- A pointer on the screen's edge to the ball when it's off screen, with its
  distance; hidden while a decoy flies.
- The "Ball cam" label, and hud.rs's controls help, worded for keyboard or
  controller.
- The replay's letterbox moved into the same HUD.
- **The aim marker** (`FVolleyAimMarker`), drawn on the floor:
  - rings of 1 m and 0.9 m, a cross, the spread ring when a hit may stray,
    and a faint line from your feet
  - warming from yellow to orange as the hit charges
  - shown while the ball is yours to play
- **`sim_ffi`'s `volley_match_hud(player)`** gives what the sim knows: the
  hit preview and whether the ball is yours (aim.rs's rules), the save
  prompt (hud.rs's `save_prompt`), the moves' names, cooldown and charge,
  and the points to win the set.
- Screenshots now include the HUD. `VOLLEY_HUD=player` shows the player's
  part (aim, prompts, abilities) while a bot plays them, to film it.
- **Film**: `volley-unreal/Saved/Films/hud.mp4` (20 s, playing Cross, with
  sound).

### 2026-10-11: the title and hero select screens

The Bevy client's screens around a match (flow.rs, select.rs, heroes.rs)
are ported, drawn by `AVolleyHUD` like the rest of the HUD:

- **Title**: "VOLLEY" over the arena, with the paused preview match behind
  it. Enter, Space or A goes on.
- **Hero select**: heroes.rs's three cards, each with the hero's name in
  their colour, title, blurb and lines. The buttons are filled in for your
  device ([R]/[G] or [X]/[Y]). The chosen card has a border in the hero's
  colour, and the newest hero (Golazo) is chosen by default.
  - A / D, the arrows, the d-pad or a flick of the stick pick the hero.
  - W / S or up / down switch the arena. It rebuilds behind the cards:
    `AVolleyArena::Build` takes down the old one's actors and pieces first.
  - Enter starts the match.
- **Playing**: each match is new (`AVolleyMatchActor::NewMatch`): the old
  players go, a fresh sim starts with your hero, and the controls forget the
  press that started it, as Bevy's do. The sim only steps while playing.
- **Pause**: an overlay replaces the "PAUSED" in the score line.
- **Match over**: "Red wins!" in the team colour, with the sets. Enter goes
  back to the hero select for a rematch.
- The menus are sized from Bevy's 1280x720 and scale with the window. The
  HUD's text is now the engine's runtime Roboto with a copy at each size,
  so big text stays sharp.
- `Play.bat cross` (or `golazo`, `all-rounder`) and filming skip the
  menus, so films and checksums are unchanged: tick 1200 is still
  `21fa2aa0367dbf04`. `VOLLEY_SCREEN=title` or `select` opens on a menu
  anyway, and with `VOLLEY_SHOTS` it's shot in both arenas.

### 2026-10-11: the heroes' ability looks, and the rest of the Bevy client

**The abilities' looks.** Bevy has few: trail colors, the decoy, puffs and a
gold ring. So they follow the concept art (`docs/concept`), in the hero's
color (Cross orange, Golazo green):

- **The kit glows** (`M_Character`'s new `HeroColor`, `Trim`, `Aura`):
  - The trims light from the texture itself, from texels close to the hero's
    hue, saturated and bright. That picks out Cross's orange stripes and
    "00", and Golazo's green boots and gold trims.
  - They glow a little at rest, brighter as the ultimate charges, and breathe
    once it's ready. Through an ability move a soft aura joins them.
- **Streaks** follow the limbs doing the move: Cross's hands through the
  Crossover and Posterizer, and Golazo's feet through the Banana Kick and
  Chilena.
- **The ball glows** in the hero's color after a dunk, a banana kick or a
  crossover. After the Chilena both balls glow alike.
- **Flashes and sparks** in the hero's color: the split, the crossover's
  catch, the dunk and the banana kick striking the ball, a decoy popping, a
  dribble.
- **The Posterizer**: its leap blasts sand and orange sparks off the floor.
  The dunk landing through a block sends a shockwave across the floor.
  Between bots, Cross starts the Posterizer (5 times in 40,000 ticks) but
  never lands the dunk, so a posterize hasn't been seen in a film yet.

**What was still missing from the Bevy client:**

- **effects.rs**, all of it:
  - sand where a dive hits the floor, a foot save sweeps, a dash pushes
    off, a jump lands, a foot kicks off a wall, a chain spike bursts
  - sparks instead in the Neon Stadium
  - `FVolleyEffects` now owns the puffs, the ball's landing puff included
- **draw_team_markers**: a ring in team color under everyone, doubled
  under yours, and a gold ring that breathes once an ultimate is ready.
- **The decoy's landing rings.** Only the real ball had them, which gave the
  decoy away. `sim_ffi`'s match state now carries where the decoy lands and
  how long until each ball lands (116 bytes). Player state carries the
  ultimate's charge (140 bytes).
- **The ball's glow in the Neon Stadium** (arena.rs).
- **Ring lines** widen with distance from the camera, as Bevy's 2 px gizmo
  lines do. On the Beach the markers glow brighter against the sand.
- **Bench mode** (bench.rs): `VOLLEY_BENCH=out.json`, with Bevy's settings
  and report fields, including the director's camera and screenshots at
  touches.
  - The first run (30 s, Neon) gives touch gaps of 0.25 m mean and swings
    all timed. Hand pops reach 459 m/s, and 20% of planted-foot frames
    slide. Those last two are the numbers to work on.
- Not ported: inertia.rs. It's off by default in Bevy and measured no better
  than crossfades (experiments 06 and 07).

**Fixed on the way:** `M_Stylized` lacked the instanced-mesh flag. The Neon
crowd, the stars, and the sand and sparks were drawing with the engine's
grey default material. The crowd now shows its six shirt colors, so the
stands are much busier than in earlier films.

**Tools:**
- `VOLLEY_FOLLOW=N` films one player close up.
- `cargo run -p volley_sim_ffi --release --example events` lists the
  heroes' moments, to find them.
- `MAKE_ASSETS_ONLY=materials` remakes just the materials.

### 2026-10-11: animation lessons from Knockout City

These come from a structural study of Knockout City's character animation
tree. No data or assets were taken, only how it's organized and tuned. Each
change was measured with bench mode: 90 s, Neon, the same bot match.
Hand-pop counts vary about ±50 between identical runs.

- **Blends.** Ground and air (and dashes) now blend over 0.1 s, and gaits
  faster: a quicker one over 0.2 s, a slower one over 0.15 s (Bevy's were
  0.15 and 0.25 s).
  - Contact gaps fell from 0.25 m to 0.19 m; banana kicks, which start from
    the air pose, from 0.78 to 0.39 m.
  - Planted feet slid in 27% of frames instead of 29%.
  - Knockout City's faster action blends (0.05–0.1 s) were tried and
    dropped: hand pops (frames over 30 m/s) went from about 210 to 360–790.
    Actions keep 0.15 s.
- **Gameplay sets the swing's playhead.** It comes from the time left to
  contact, like Knockout City's throws, instead of nudging clip speed. It
  never goes backward and catches up at most 4× (1.5× broke the serve).
  Contact-frame error fell from 5 ms to 2 ms.
- **A charge pose.** Holding pass or attack leans the upper body into the
  coming swing's wind-up, further as it charges, then hands over to the
  swing. It's mostly the arms; the spine and head take 40%, so a spike's
  arched wind-up doesn't fold a running body. It's for the person only:
  bots don't charge.
- **Graded impacts.** A hard ball (passes, digs and now blocks) lands at one
  of four impact levels by its speed (12/18/24 m/s): the hips push back
  0–14 cm, and from level 2 the upper body flinches back.
- **Perfect contact.** A clean attack ("perfect!") follows through 30%
  faster and flashes gold with gold sparks. Without a dedicated clip, that's
  how it reads apart from an ordinary one.

**Fixed:** the game crashed at its first garbage collection, about 60 s in.
The audio's sounds and attenuation, and the HUD's font, were held only by raw
pointers; they're properties now. A stress run with collection every 5 s
passes.

### 2026-10-11: hand pops

Bench mode now writes each hand pop (a palm over 30 m/s) to
`<report>.pops.csv`, with how fast the body and the pose moved and what was
playing. That found four things:

1. **The bench overcounted.** It divided the last frame's movement by this
   frame's time, so a short frame after a long one inflated speeds. Bevy
   measures in the same frame and didn't have this. Fixed, the 90 s run has
   124 pops, not about 300.
2. **Crossovers flipped the left arm, at 150–180 m/s.** A spike reaches for
   the ball with the right arm only. When the crossover took over, the left
   arm jumped straight to the right's reach weight. Each arm now blends on
   its own weight, and a limb aimed at a goal almost behind it eases off
   rather than spinning about an unstable axis.
3. **Blends flipped arms half a turn in one frame, at 150–230 m/s.** This hit
   a crossover handing to the spike's follow-through, and a serve starting
   out of a celebration. The engine's blend takes the shorter way round each
   frame, and for rotations nearly half a turn apart that choice flips
   between frames. Clips, the legs layer and the charge pose now blend each
   bone toward what it showed last frame, so the side can't flip.
4. **What's left are spikes and sets starting late in their wind-up,**
   35–55 m/s. The arm blends from running into the cocked swing over 0.15 s
   while the body slides toward the contact point. These are fast but
   continuous, not flips.
   - Limiting the swing's catch-up to 2× changed nothing.
   - A slower slide cut pops to 93 but moved passes from 0.19 m to 0.25 m off
     the ball.
   - An earlier slide changed nothing.
   - None was kept.

Result over 90 s: the fastest hand went from 228 m/s to 54, frames over
40 m/s from 37 to 26, and the 99.9th percentile is 26 m/s. Frames over
30 m/s stayed about the same (124 → 122). Contact gaps (0.19 m) and timing
(2 ms) held.

### 2026-10-11: the swing starts on the approach

Most remaining hand pops were spikes and sets starting late: the arm went
from running to cocked in one 0.15 s blend.

- **`sim_ffi`'s look-ahead now reports each player's next hit move** (pass,
  spike or an ability) and the ticks until it starts, within 0.6 s. Player
  state is 148 bytes.
- **The upper body leans into that swing's wind-up over its last 0.5 s**
  (up to 70%), as a spiker draws the arms back on the approach. When the
  swing starts, the lean fades out as the swing fades in. This uses the
  charge pose's layer: a person holding E gets the same lean from the
  charge, since the look-ahead can't know a person's presses.
- **Fixed on the way:** when the coming swing changed (a bump becoming a
  set), the charge pose vanished in one frame. It now fades out, then
  switches.

Over 90 s:
- Frames over 30 m/s fell from 122 to 45, and over 40 m/s from 26 to 0.
- The fastest hand fell from 54 m/s to 39.
- Contact gaps (0.20 m) and timing (2 ms) held, and the checksum is
  unchanged.

What's left is mostly the body itself moving fast (a run plus the slide
toward the contact), and Golazo's takeoff.

### 2026-10-11: clearing the net, and reception physics

Two rules changes in the sim, from the user's list.

**Golazo hit the net.** His kicks meet the ball at about net height and fly
flat like spikes. Between bots, 29 of 36 banana kicks (81%) went into the
net.
- Now any ball hit from below the net's top arcs over it. The flight
  lengthens to the shortest time whose arc clears the net
  (`clearing_seconds`): by 15 cm for a clean contact, down to 30 cm short of
  clearing for the worst, so a sloppy touch at the net can still find it.
  The arc is capped at 2 s.
- Golazo's banana kicks now net 0 of 35, leaving at 23.7 m/s instead of
  25.6.

**Receptions** (`crates/sim/src/reception.rs`) go where the ball and the
timing let them:
- **Control** is timing (ticks from press to touch: within 4 is on time,
  each earlier tick costs 1/16, down to 0.2, so holding pass is early) times
  position (full stretch halves it). Digs top out at 0.7 control, foot saves
  at 0.75 (0.95 with great feet).
- **Difficulty** is the ball's speed (10 to 28 m/s), how steeply it drops,
  and how sharply it has to be turned toward the aim. A hero's ability or
  ultimate adds 0.35.
- **Error** is difficulty × (1 − control). It widens the landing spread (up
  to 3 m per unit). From 0.3 the ball starts to shank, fully by 0.7: it
  carries on roughly along its path, deflected up to 80° at random, 2–10 m,
  lower and faster.
- **The bots now time their presses,** 1–4 ticks before the ball arrives,
  varying from ball to ball, instead of arming 0.35 s early. Their passes
  are mostly clean, and digs and foot saves are often wild.
- **The HUD** says "Perfect pass" or "Shanked!" on your receptions, and the
  help explains timing.

**For the client:** bots now press so late that a pass swing started on the
press had no time to wind up. Contact gaps went from 0.19 m to 0.53 m, and
there were 27 hand frames over 40 m/s.
- The look-ahead now gives each bot's next touch (ticks and spot), used as
  `contact_ticks` until a move is armed.
- The client starts a bot's pass or banana-kick swing from it. Passes are
  back to 0.18 m, and frames over 40 m/s are down to 3.
- Digs and foot saves, now common in longer rallies, meet the ball 0.9–1.4 m
  off: their clips have no reach or timing yet.

Over 10 minutes of bots: points per minute went from 5.6 to 3.9, and touches
per point from 6.1 to 9.7. The new checksum at tick 1200 is
`5823bc9c22db5ece`. `cargo run -p volley_sim_ffi --release --example stats`
prints these numbers for any future rules change.

### 2026-10-11: saves that reach, and the skill bar

**Digs and foot saves reach for the ball.** The sim takes a foot save up to
2.2 m from the body (2.6 m with great feet) and a dive up to 1.5 m. The
client only bent a leg or arms toward the ball where it was then.
- Now, over a save's last 0.3 s, the body slides (up to 1.6 m) so the
  reaching hands or foot, measured in the drawn pose, meet the predicted
  contact. The arms or leg aim there, and the slide holds after the touch.
- Golazo meets a high save (above 0.8 m) with a high kick or a lunging
  volley: a sweep or a slide along the floor can't reach a ball at the chest.
- Foot saves are timed like swings, and a save with the ball nearly there
  blends in by the touch.
- **In the sim,** a foot save's leg now shoots out over 6 ticks (from half
  its reach), so a ball at full stretch is met a moment after the press, not
  on it. No animation could cover 2.4 m in one tick. Bots and contact
  prediction use the same rule.
- Over 5 minutes: foot saves 1.37 m → 0.49 m off the ball, digs
  0.93 → 0.28 m, all touches 0.20 m.
- Bench mode now also writes every touch (gap, nearest hand or toe, state)
  to `<report>.touches.csv`.

**The skill bar,** along the bottom like a MOBA's:
- **The slots:** the hero's moves (pass, attack, dive, foot save if they
  have it, dash), then the ability and ultimate.
- **Icons:** drawn by `Tools/icons.py` from simple shapes, imported by the
  asset script.
- **Each slot shows:**
  - its button underneath;
  - a dark wedge that uncovers clockwise and counts down while it cools;
  - the ultimate's charge filling, with a breathing gold border when ready;
  - a warm fill while a hit is held to charge it;
  - a bright border while the move is underway;
  - dimmed when it can't be used now.
- **`sim_ffi`'s HUD struct** adds the hero's moves, the ability's full
  cooldown and the dash's cooldown (100 bytes).
- **Replaces the ability text.** The full controls help now shows for a
  match's first 30 s, or while H (d-pad down) is held, clear of the bar.

### 2026-10-11: one button for every touch

The user found too many buttons to be fun. Pass now also foot-saves and
dives, and the sim (`crates/sim/src/assist.rs`) picks which from the ball,
for a person's input (bots press their saves themselves):

- **Foot save:** the ball comes in below the pelvis (1 m; the chest, 1.5 m,
  for great feet) and a foot save reaches it. This wins over a low pass.
- **Pass:** a pass will meet the ball.
- **Dive:** the ball lands before you could run there, you're steering
  toward it (within 60°), and a dive gets there. Judging "out of reach" by
  running time keeps an early press while sprinting from becoming a dive.
- **Otherwise** a pass, armed and waiting. While it waits, it turns into a
  foot save or dive the moment the ball needs one.
- **Unchanged:** serves and blocks stay on pass, and dash stays on its own
  button (it's movement, not a touch).
- **Tests:** three new ones (kick a low ball in reach, dive for one out of
  reach only when steering to it, pass a ball at chest height). All 63 pass.
- **In the client:** the dive and foot-save buttons are gone. The skill
  bar's pass slot shows what the button would do now (`VolleyHud.pass_action`,
  104 bytes), and the save prompts and help say Q / RB.

### 2026-10-11: smoother animation, round one (dead blending, overlap)

The goal now is Knockout City's fluid feel. Each technique gets measured
by bench mode and filmed; it stays only if it wins (experiments.md 20-21).

- **New bench measure, `smoothness`:** how sharply the hands, head and hips
  accelerate relative to the body, as the 99th and 99.9th percentiles
  (m/s²). A jolt in the animation shows as a spike.
- **Dead blending, now the default:** Unreal's method for cutting between
  animations. On a cut, the old pose keeps moving the way it was going,
  slowing down, while it fades into the new clip. Crossfading blended two
  clips' poses instead.
  - The engine's `FAnimNode_DeadBlending` needs an Animation Blueprint, so it
    is ported into our proxy using the engine's own maths and defaults.
  - Over two runs each, hand pops over 30 m/s fell from 45 to 29, and
    hands, head and hips all move with 5-6% less jerk. Contact gap and foot
    sliding stayed the same or got a little better.
  - `VOLLEY_BLEND=crossfade` brings the old blending back.
- **Overlap, on by default:** the chest and arms are on springs that the
  hips' acceleration and turning push. The upper body trails the hips into
  a run, pitches past them when they stop, and swings back; the arms trail
  too. The arms swing less while they reach for the ball.
  - The bench calls it neutral: pops and contact unchanged, hand jerk 1.5%
    higher (the motion is deliberate).
  - It's judged by eye. `VOLLEY_OVERLAP=0` turns it off.
- **Films:** `Saved/Films/dead-blending-*.mp4` show the same rally before
  and after, side by side, from the broadcast camera and close up at half
  speed.

### 2026-10-12: animation state at resets

Continued from the newer dead-blending and overlap work already in the
checkout. Foot-lock history now resets when the rally changes, including
planted positions, release targets and prior samples. Feet no longer reuse
world positions from the previous rally. Dead blending also requires a
previous pose before capturing a transition, avoiding an empty-history read
on startup or after the bone set changes.

Validation: Unreal editor target compiled successfully. No new animation
bench was run; this change does not establish an improvement in ordinary
running foot sliding or hand-speed measurements.

### 2026-10-12: snappier hits (experiments.md 22)

- **Strike timing:** timed swings now settle into their loaded pose, hold
  it a beat, and strike 30% faster, speeding up into the ball.
- **Spike arm:** a spike's arm stops steering toward the ball on the way in
  and only snaps onto the contact point in the strike, so it can draw back.
- **The coil:** spikes and serves get a hand-keyed coil on top. The chest
  turns away and arches, the arm cocks behind the head, it all releases
  into contact, and it swings past after.
- **Bench:** a fifth fewer hand pops; contact and timing unchanged.
  `VOLLEY_SNAP=0` turns it all off.
- **Film:** `Saved/Films/snappy-spikes-quarter-speed.mp4`.

### 2026-10-12: stronger coil, and kicks coil too (experiments.md 23)

- **Stronger overhand coil:** about 1.6×. The chest turns 0.75 rad away and
  the arm cocks 0.85 rad.
- **Kicks wind up:** the kicking hip turns back, the leg draws back with the
  knee folded, and the chest leans away; then it all whips through. The
  kick's leg reach now snaps onto the contact point only in the strike.
- **Contact smoothed:** the coil leaves contact as fast as it came in, so
  there's no jolt in the hand there.
- **Film camera:** following a player can now reach the near sideline.
- **Film:** `Saved/Films/stronger-coil-and-kicks-quarter-speed.mp4`.

### 2026-10-13: curve volleys wind up (experiments.md 24)

- **Golazo's banana kicks are timed swings now.** The bot jumps right after
  pressing the kick, so it's met in the air, but the wind-up had picked the
  ground kick and the touch threw it away for the high volley's
  follow-through. The wind-up now picks by whether he'll still be standing
  at contact (the look ahead's takeoff), and the touch releases whichever
  curve kick is winding up. That brings the snappy strike and the kick
  coil.
- **Bench** (alternating, two warm runs each): curve-volley contact gap
  0.58 m to 0.21 m, worst touch 1.06 m to 0.37 m. Spike gap, hand pops and
  swing timing are unchanged.

### 2026-10-13: banana kicks predicted on the right tick (experiments.md 25)

- **The last two late curve volleys** (ticks 338 and 3992) came from the sim
  predicting the touch a tick late. The touch meets a banana kick wherever
  the ball is in reach, but `predicted_contact` waited for the attack check
  as well. It doesn't now, and a test pins every armed banana kick's
  prediction to its touch. The match itself is unchanged (same checksums).
- **Bench** (sim DLL alternating, two warm runs each): curve-volley gap
  0.23 m to 0.15 m, p90 0.45 m to 0.16 m; the two late touches 0.46 m to
  0.16 m. High-volley timing off 0.020 s to 0.007 s. Spikes, hand pops and
  other swings are unchanged.
- Unreal picks it up from `volley_sim_ffi.dll`; rebuild it with
  `cargo build -p volley_sim_ffi --release` before building the editor.

### 2026-10-13: a deeper study of Knockout City's animation

`docs/animation/knockout-city-study.md` covers it. The analysis package now
includes every animation resource, and the 489 gameplay clips' lengths were
read from an installed copy of the game.
- **Short actions:** keyed at 30 fps, whole actions are 10-22 frames.
- **Blends:** into actions in 0.05 s; out only after a delay.
- **Directions:** a variant of almost every action for each direction
  relative to facing.
- **Transitions:** dedicated clips for pivots, stops and landings into
  running.
- **Hits:** reactions on a grid of 8 directions × 3 heights × 3 strengths.

Their 0.05 s blends, tried with our clips even with dead blending, multiply
hand pops 15-fold (experiments.md 24): the look comes from clips made for
it. The study ends with the options for Volley's clips.

### 2026-10-14: walls that break, getting hit, power receptions

Three rules in the simulation (`crates/sim`), drawn by both clients. All 68
sim tests pass, four of them new.

- **Walls that break:** the walls are 24 glass panels, 6 m each.
  - A ball hitting a whole panel cracks it (`WallCracked`). The next one
    breaks through and is out (`WallBroken`, `Out`), as is any ball through
    the gap after. The team that didn't touch it last scores
    (`PointReason::Out`).
  - Panels reset each set. `MatchConfig::breakable_walls` turns it off.
  - The FFI's match state carries every panel's state (140 bytes now).
  - **Unreal:** builds the walls as panels; a cracked one goes frosted and
    bright, a broken one disappears, with a flash and a burst of sparks.
- **Getting hit:** a ball that hits a player not playing it (inside the
  body, 0.1-1.85 m up, coming at them) bounces off them.
  - **Where it goes:** back the way it came at 45% of its speed, popping up
    2-6.5 m/s higher the higher it hit, scattered up to 35° (seeded, so
    deterministic).
  - **It's a touch:** it counts as their team's touch.
  - **Fast balls (8 m/s and up):** stun the player for up to 1.3 s and push
    them back (`BodyHit`, with strength).
  - **Unreal:** they go down (with `VOLLEY_KC=1`, Knockout City's strong
    front-on hit) or flinch, with a flash and sand where they fall.
- **Power receptions:** a reception of difficulty 0.55 or more, met with an
  error of 0.3 or more (a hard or powered ball pressed badly), shoves the
  receiver back 2.5-6 m/s with a 0.25-0.7 s stun (`Shoved`), on top of the
  shank. In Unreal the hips recoil harder.
- **In bot play (10 minutes):** 17 shoves, 2 body hits (both knockdowns),
  2 cracked panels. Bots aim 1 m clear of the walls, so the walls are mostly
  in play for people's bank shots.

## 2026-10-14: bank shots and wall shots (`game-progress`)

The walls become a weapon, not only a hazard. 70 sim tests pass, two of them new.

- **Bank shots:** a shot over the net aimed past a side wall banks off it.
  - The flight aims at the target mirrored beyond the wall
    (`mirrored_beyond_wall`), so it folds back onto the spot.
  - It takes longer by the square root of how much farther the path is
    (`bank_stretch`).
  - Curves and decoys don't bank.
  - Bots weigh two bank landings per wall (at half and three-quarter depth,
    0.6 of the way to the wall) with a surprise bonus, but only when the
    panel it would hit is whole.
  - About 12% of bot attacks bank (12 of 103 in 10 minutes), and the bot
    match cracks 8 panels and breaks 1.
- **Wall shots:** an attack within 0.8 s of a wall jump (`wall_jump_tick`)
  gets +0.25 quality and is 15% faster, and it counts as a power ball for
  the receiver (`Event::WallShot`, FFI event 23).
  - Bots don't wall-jump into attacks yet, so in bot play it's a player's
    move.
- **Unreal:**
  - "Wall shot!" callout with a flash and sparks off the hand.
  - Callouts: "The glass shatters!", "Knocked down!", "Overpowered!".
  - A cracked panel now shows glowing crack lines: a star of rays with a
    few spurs, the same per panel. They replace the frosted tint, which the
    fresnel glass hid from the front.

## 2026-10-14: directional knockdowns (`game-progress`, Unreal)

Getting hit now reads from where the ball came. This uses the Knockout City
clips (`VOLLEY_KC=1`, local only).

- **Their hit clips are poses:** each `hit_d*_h*_i*` clip is a single held
  frame of a body thrown through the air (8 directions, 3 heights, 3
  strengths). The knockdown used to freeze on one of them.
- **Directions:** measured from how the head is thrown, d0 is a ball from
  the front, then clockwise seen from above, 45° apiece.
- **Knockdown** (body hit with quality over 0.3, or posterized), in four
  stages:
  1. The impact pose for where the ball came from and how high it hit.
  2. The body thrown up on a 55 cm arc for 0.22 s plus 0.18 s × quality.
  3. Their knockback fall (`dash_knockback_transto_dazed_loop`), then dazed
     on the ground (`dash_knockback_dazed_loop`) while the sim's stun lasts.
  4. Getting up (`dash_knockback_end`) at 1.4× speed.
  - Running cuts it short once the player is free.
  - A floor guard keeps every bone (as last drawn) at least 6 cm above the
    floor, since the flying poses dip below the feet.
- **Flinches:** a light body hit or a shove flashes the impact pose on the
  upper body (up in 0.05 s, gone by 0.4 s), over the charge pose.
- **Without `VOLLEY_KC`:** nothing changes (the mocap knockdown plays).
- **`VOLLEY_TEST_HITS=TICK`:** fakes hits on every player at that tick, for
  filming. The first four are knocked down from four sides, the fifth
  flinches, the sixth is shoved. Tick 30 has everyone standing for the
  serve.
- **Wall cracks:** a cracked panel now shows glowing crack lines (a star of
  rays with spurs).

## 2026-10-14: Alysa is playable (`game-progress`)

The ice skater is the fourth hero. 73 sim tests pass, three of them new.

- **Sim** (`moves::ALYSA`):
  - **Stats:** run 9.6 m/s, jump 6.8 m/s, dash 18 m/s; moves Pass, Spike,
    Foot save, Flash Freeze and Triple Axel. She has no dive.
  - **Glide** (`Passive::Glide`, also anyone with `ice_until` on them):
    - Speeding up, and turns within about 45° of the way she's moving, take
      1.2× the usual acceleration.
    - Braking and hard turns take 30 m/s².
    - A stop from a full run slides 1.3-1.9 m; the All-rounder stops in
      under 0.7 m.
  - **Flash Freeze** (`Touch::Freeze`, 8 s cooldown):
    - The ball freezes at its first net crossing for `FREEZE_TICKS` (0.5 s),
      by starting its flight a tick later each tick.
    - Its path afterwards is exactly the same, half a second late. Bots'
      reaction timing slips with it.
    - Not while it's blocked.
    - Events `BallFrozen` and `BallThawed`.
  - **Triple Axel** (`Touch::Ice`, ultimate):
    - A 1.15× leap at half gravity, steering 6 m.
    - The spike is clean, 15% faster, and goes through blocks like a dunk
      (`Posterized`).
    - Its first touch, body hit or landing bursts it (`IceBurst`) into a ring
      of ice, 1.5 m in radius, for 2 s. Anyone inside glides
      (`Sim::ice`, `Player::ice_until`).
- **Lineup:** both clients' bot match is now Red: three All-rounders, Blue:
  Cross, Alysa, Golazo. Alysa takes the attacking slot.
  - In 8 minutes of bot play she attacks 22 times (every one a freeze while
    it's ready) and takes 2 Triple Axels.
  - With her in the back slot instead, she barely touched the ball.
  - The cost: Golazo's banana kicks mostly leave bot play, since he's no
    longer the attacker.
  - Tick references in older entries are for the old lineup.
- **FFI:**
  - `VOLLEY_KIT_ALYSA` = 3.
  - Moves `VOLLEY_FLASH_FREEZE` = 8 and `VOLLEY_TRIPLE_AXEL` = 9.
  - Events 24 BALL_FROZEN, 25 BALL_THAWED, 26 ICE_BURST.
- **Unreal:**
  - **Look:** with no model yet, she's the athlete with the high bun,
    platinum blonde, in the team kit, with glacier-cyan trims that glow
    through her moves.
  - **Effects:** a frost burst where the ball freezes; the ball glows cyan
    until the next touch; a spray of ice when it thaws. The Triple Axel's
    takeoff throws ice and a shockwave. The ice ball's burst leaves a
    shimmering ring of ice on the floor that fades out.
  - **HUD:** a hero card on the four-card select screen
    (`VOLLEY_HERO=alysa`); skill icons (a snowflake, and a spin into a
    downward arrow); callouts "Flash Freeze!" and "The ice ball shatters!";
    the name "Alysa" over her.
- **Bevy client:** her hero card, lineup, clips (Flash Freeze and Triple
  Axel play the spike) and event arms.

## 2026-10-15: heroes only, Alysa's full kit, bots' wall plays (`game-progress`)

80 sim tests pass, seven of them new.

- **Lore:** `docs/lore.md` (ARBITER, the AGI that reconstructs a forgotten
  volleyball), and a paragraph in the README.
- **The All-rounder is gone.**
  - Its moves are `moves::BASE`: the kit every hero builds on, and a bare
    sim's default. It isn't a hero.
  - Teams are heroes only:
    - Red: the chosen hero plus the other two (`moves::team_with`, Golazo
      in the attacking slot when he can be).
    - Blue: `moves::RIVALS` (Cross, Alysa, Golazo).
  - The bot match is Red Alysa, Golazo, Cross against Blue Cross, Alysa,
    Golazo. In 8 minutes every hero uses their kit: 35 banana kicks, 31
    Flash Freezes, 6 crossovers, and all three ultimates.
  - FFI `KIT_BASE` = 0. The hero select has three cards (Alysa by default),
    and `VOLLEY_HERO` defaults to Alysa.
- **Wall Skate** (`Passive::WallSkate`, `Player::wall_skate`):
  - Into an arena wall (not the net) at 5 m/s or more, her run turns along
    the wall at full speed, 0.6 m up.
  - It lasts up to 1.5 s. Steering along the wall pushes at 12 m/s². It
    ends on steering away from the wall, below 2.5 m/s, on any jump (a wall
    jump from it is a full jump), or when stunned.
  - Riding a cracked panel breaks it (`WallBroken`) and drops her; she can't
    ride over a broken one.
  - Event `WallSkated` (FFI 27); `VolleyPlayerState.wall_skating`.
- **Ice Trail** (`Passive::IceTrail`):
  - Each tick of her dash leaves ice 0.6 m in radius for 3 s. Anyone on it
    glides.
  - The sim's ice is now a list of patches (`Sim::ice`, `Ice`), the ice
    ball's ring among them.
  - `VolleyPlayerState.on_ice`.
- **Bots' wall plays:**
  - A setter sends one set in three up a wall lane, 1.2 m from the wall.
  - An attacker meets a set coming down 1-4.5 m from a side wall: it runs
    (dashing if it must) to the wall under it, jumps, kicks off the wall at
    0.5 m about 0.2 s before the ball drops to 3.3 m, and spikes on the way
    out.
  - About 9-10 wall shots per 10 minutes of bot play (none before), and
    Alysa rides walls about 11 times in 8 minutes.
- **Unreal:**
  - Riding a wall plays as running, leaning off the glass about 30°, feet
    on it, throwing ice off the blades.
  - Her dash lays a trail of shimmering ice discs that fade over 3 s.
  - Anyone gliding at speed glitters with ice.

## 2026-10-15: Alysa's model

- **Tripo:** multiview from the four views of her turnaround (cropped from
  `docs/concept/alysa.webp`), with a 4K texture, lighting removed and no PBR.
- **`tools/blender/rig_hero.py`:**
  - Simplified to about 57k vertices and rebaked to a 2K texture.
  - Rigged with the new `--body female` (the female placeholder's weights).
  - **Leg fitting (new):** her model stands with its feet apart, which left
    her standing in a wide V in every pose until the legs were fitted. Each
    thigh now turns about 9° so the skeleton's ankle is over the model's foot,
    and that's the rest pose. The Blender preview poses relative to the rest
    pose, so it still shows the V; the game sets bone rotations outright, and
    she stands with her feet together.
- **Unreal:** `MAKE_ASSETS_ONLY=bodies:Alysa` imports just that body
  (`Characters/Alysa`), and her look is her own model, with her trims
  glowing in her color. The Bevy client uses it too.

## 2026-10-21: sounds from Openverse, and the heroes' models checked

- **Sounds** (Openverse, CC0 only, from Freesound; credited in
  `crates/client/assets/CREDITS.md`), trimmed, mono and normalized:
  - **The glass:** a crack when a panel cracks; a shatter, and the crowd's
    "ooh", when one breaks.
  - **Alysa's ice:** Flash Freeze freezes and thaws; the ice ball bursts.
  - **Skates:** a scrape up onto the wall (Wall Skate) and on a real hockey
    stop (from over 6 m/s, braking harder than 24 m/s², at most every 1.5 s).
    At first every glide's slowdown counted: 206 scrapes in 80 s.
  - **The crowd's "ooh"** on knockdowns, wall shots and the hardest hits (hit
    strength 0.9 or more), at most every 2 s.
  - Checked with `VOLLEY_SOUND_LOG` over 4900 ticks of bot play: every one
    plays.
- **Model check** (the `blender-asset-qa` skill on Alysa):
  - **Passed:** no floating parts or inside-out pieces, all vertices
    weighted, at most 4 influences, 50k triangles.
  - **Found:** the texture layout used **7.4%** of the texture. Smart UV
    Project makes the islands but doesn't pack them.
    - `rig_hero.py` now evens out the islands' texel density and packs them
      tight, turned to fit, with larger islands (angle limit 80°) and thinner
      margins.
    - Coverage is now about **40%**, about 5x the texture detail. The rest is
      lost to some 7,000 islands, each with a margin.
  - **Accepted:** 17 overlapping triangle pairs and 144 hidden back-to-back
    pairs left by Tripo; the duplicate vertices are glTF splitting at texture
    seams.
- **All three heroes re-rigged** from their Tripo sources (`volley-assets`)
  with the new packing. Cross's and Golazo's models stood with their feet
  apart too (the legs now turn 8° and 14° to fit, as Alysa's 9°).
- **An import bug:** re-importing a hero over its old folder replaced the
  mesh but kept the old textures. With a new texture layout, Cross came out
  in patches (fine in Blender). `import_bodies` now clears the folder first.


## 2026-10-21: a ragdoll for hard body hits

- A ball hitting a body at **14 m/s or more** now throws it as a ragdoll,
  led by the part it hit (experiment 33). Softer hits keep the knockdown
  clips.
- **The throw:**
  - Along the ball and spread by distance from the impact, so a head shot
    whips the head back first and the body turns over.
  - About 2 m on a hard hit.
- **Recovery:** after the stun, the pose it landed in blends into the get-up
  and the body eases back to its spot.
- **Off when hit effects are off.**
- **Test:** `VOLLEY_TEST_HITS=30` (four players hit from four sides at four
  heights).

## 2026-10-21: a main menu like a hero shooter's, and music

- **The heroes on a stage** (`VolleyMenu.h`): Cross, Golazo and Alysa stand
  in a row at one end of the arena, each under a spotlight with a touch of
  their color, a rim light behind them.
  - They're the game's own characters on made-up player states, so they
    idle, turn, walk and glow as in a match (charged ultimates breathe).
  - **On the main menu** they take turns showing off every 4.5 s: Cross's
    jump-shot cheer, Golazo's volley kick, Alysa's spin. The other two turn
    to watch.
  - **On the hero select** the chosen hero walks forward into the light,
    the other two step back and dim, and the chosen one cheers on arriving.
    They step round each other on the way.
  - **A ball between them:** on the main menu a ball drops in and they pass
    it round: Golazo's kick pass, Alysa's bump, Cross's set. Each swing starts
    so its contact frame meets the ball, and heads follow it. After six passes
    Golazo volleys it into the stands, and they all turn to watch. Rallies and
    showing off take turns.
  - **Keeping it up alone:** after the cheer, the hero in front keeps the ball
    up by themselves: Golazo with kick-ups, Alysa bumping, Cross setting
    himself high.
  - **On the beach** the spotlights stay low (in daylight they blew out the
    sand) and the rim light is off.
  - The camera drifts slowly, off to the side, so the heroes stand right of
    the menu.
- **The screens:**
  - **Main menu:** a dark shade down the left, the logo, and PLAY, HEROES,
    ARENA (changes the arena behind), SETTINGS and QUIT, with the
    highlighted item in orange and a line on what it does.
  - **Hero select:** the hero's name in their color, title, blurb and moves
    on the left, the roster along the bottom, the arena top right.
    HEROES opens it just to look.
  - **Settings:** hit effects, screen shake, hit-stop and music volume (kept
    with the other settings).
  - **Keys:** Esc / B goes back everywhere. Backspace / View on the pause
    screen leaves the match for the menu.
- **The match is hidden behind the menus:** its players, ball and rings.
- **The ragdoll** now also throws a blocker flattened by a dunk or an ice
  ball (POSTERIZED), from the hands and head. Bot matches give few hard body
  hits (3 body hits in 10 minutes, one hard enough), so this adds moments.
  A posterize didn't come up in 20 minutes of the example match, so this
  one isn't filmed yet.
- **Music** (ACE-Step 1.5, made locally for the game): the menu theme on the
  menus, the match loop in play, the fanfare when a match is won. They
  crossfade over 1.2 s; the volume follows the setting. Resampled to
  48 kHz: ACE-Step wrote 192 kHz, which made 50 MB assets.
- **Films:** `VOLLEY_SCREEN=title|select|gallery|settings`, with
  `VOLLEY_MENU_EVERY=N` (a shot every N frames), `VOLLEY_MENU_FRAMES` and
  `VOLLEY_MENU_HERO=1-3`.
