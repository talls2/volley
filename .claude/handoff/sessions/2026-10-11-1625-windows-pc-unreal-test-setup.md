---
date: 2026-10-11 16:25 UTC
device: Windows PC
branch: main
commit: fd910b5
topic: Unreal test, plan and setup
---

# Unreal test: plan and setup

## Where things stand
A new Windows PC is set up for Volley. Both repositories are cloned next to
each other in the Downloads folder: `volley`, and `volley-assets` as a **full
checkout** (the user chose that here; there's enough disk on this PC).

Working from the previous note's verdict (the gaps are source animation, the
look and presentation), the user shared posts on AI-driven game making. The
one that mattered is Epic's official Claude Code plugin, which drives the
Unreal Editor through MCP. That led to a decision to **test Unreal as
Volley's client** before porting anything. The plan is
`docs/unreal-test.md` (commit 4428a5b). Milestones 0 (setup), 1 (the sim
in Unreal) and 2 (the stylized Beach arena, sky and light) are done and logged
there, with the first pass of milestone 3 (the Rust version's bodies and
clips, animated by a C++ port of characters.rs). Films are in
`volley-unreal/Saved/Films`, on this PC only.

Milestone 1: the new crate `crates/sim_ffi` builds the sim as a DLL with a C
interface. `volley-unreal` loads it and draws the bot match as a greybox
(capsules, ball, net, broadcast camera). Over a minute of play it gives the
same match as plain Rust: all six checksums match the `checksum` example. A
14 s film is in `volley-unreal/Saved/Films/milestone1-greybox.mp4`.

On this PC now:
- Unreal Engine 5.8.3, Visual Studio 2022 (C++ game workloads), Rust 1.99.
  The sim's 58 tests pass.
- The Unreal project `volley-unreal`, next to `volley`: blank C++ template,
  with the MCP and motion-matching plugins enabled. It builds, and the
  editor's MCP server answers on 127.0.0.1:8000. `.mcp.json` and the Epic
  plugin (in `.claude/settings.json`) are set up in it.
- **`volley-unreal`'s source is in the private `talls2/volley-unreal`**
  (source, config, README; no builds, `Saved/` or assets). Clone it next to
  `volley`; its README says how to build.

## Decisions
- **Unreal it is** (2026-10-11): the Unreal client replaces the Bevy one;
  Bevy stays as the reference the port follows.
- **Test before porting.** Unreal replaces only the client. The Rust sim
  becomes a DLL (`crates/sim_ffi`, C interface) that a C++ Unreal plugin steps
  at 60 Hz. The rules, bots, tests and server stay Rust. Porting the sim to
  C++ is a later, separate decision.
- **Two-week time-box**, six milestones, each ending in a film the user
  watches. If Cross isn't moving with motion matching after the first week,
  stop and reassess.
- **The decision comes from side-by-side films and play**, the user's call on
  looks first. Feel (60 fps or more, no slower input), cost and workflow only
  have to be acceptable.
- **The test runs on this Windows PC** (RTX 5070 Ti), not the MacBook.
- **Art is stylized, like Knockout City**: saturated colors, soft light, bold
  simple shapes. That applies to the arena now and to Cross in milestone 3.
- **The arena is futuristic, like Rocket League**: the Neon Stadium (from the
  Bevy build) is the default; the Beach stays with `VOLLEY_ARENA=beach`.
  Nothing on the floor should draw the eye from the ball: no glowing grid,
  kickoff rings or goal frames; dim lines; both halves lit the same.
- **The camera is Bevy's**, behind the shoulders, but kept on the player's
  side of the net.
- **Unreal assets come from code**: `Tools/make_assets.py` generates the
  materials and the map in a headless editor, and C++ builds the arena at
  runtime. Generated content isn't committed.
- **Commits** keep the noreply identity, now set in this clone's repo config,
  with no Claude co-author trailer.
- **The Windows Firewall stays on.** The MCP server is localhost-only. The
  editor's prompt to listen on the network (port 1985) can be cancelled.

## Next steps
1. **The user watches the milestone 2 films** (orbit and broadcast) and gives
   feedback on the look.
2. **Decided: Volley moves to Unreal** (milestone 6 skipped). The user plays
   `volley-unreal/Tools/Play.bat` and says how it feels.
3. The open questions from the plan's last log entry: where the rules live
   (recommendation: keep them in Rust behind the DLL), how online play works,
   and updating `volley`'s README and CLAUDE.md, which still describe Bevy
   as the game. The HUD and the title, hero select, pause and match-over
   screens have come over from Bevy (logged in the plan); next from it are
   the looks of the heroes' abilities.

## Open questions
- **Binary assets** for `volley-unreal` (maps, imported models) would need Git
  LFS, and GitHub's quotas are small. Undecided until there are some.
- **The Game Animation Sample**: the user adds it from Fab before milestone 3.

## Context
- Reviewed and set aside for now: three.js tools (ik01, an asset studio),
  OhMyGame. Worth trying later: Hyper3D next to Tripo for a hero model, a
  reviewer agent that only watches films and doesn't build, and a bench mode
  comparing our hit timings and ball arcs with reference games.
- The `claude` command-line tool isn't installed on this PC. The desktop app
  is used instead.
- `UnrealBuildTool.exe` run directly needs a system .NET 10. Use
  `Engine/Build/BatchFiles/Build.bat` instead (it works for `-projectfiles`
  too).
- The user likes measured progress, films and infographics (unchanged).
