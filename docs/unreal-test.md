# Unreal test

A time-boxed test of Unreal Engine as Volley's client, to decide whether the
game moves to it or stays on Bevy. Written 2026-10-11, at commit 041f87f.

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
