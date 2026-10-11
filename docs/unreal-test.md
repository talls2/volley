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

- **Art direction**: realistic heroes, or stylized? Stylized bodies hide thin
  animation much better (see the ik01 demo), and the choice changes how much
  retargeting and model work milestone 3 needs. Best decided before it.
- **Version control** of the Unreal project: its assets are binary and large,
  so it needs Git LFS, and GitHub's LFS quotas are small. During the test, the
  project stays on this PC with only its C++ source in a repository. Decide
  properly if Unreal wins.
- **Licenses**: the Game Animation Sample's clips may only be used in Unreal
  projects, so they can't come back to Bevy. Mixamo clips can go in the game
  but not be republished raw, as now.
- **Other tools to try along the way**: Hyper3D next to Tripo for the next
  hero's model, and a comparison mode in the bench (our hit timings and ball
  arcs next to reference games and footage), like the attack-pattern
  visualizer others built for their combat.

## Log

Nothing yet.
