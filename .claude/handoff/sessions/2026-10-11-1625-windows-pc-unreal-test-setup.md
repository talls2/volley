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
`docs/unreal-test.md` (commit 4428a5b). Milestone 0, setup, is done and logged
there (fd910b5).

On this PC now:
- Unreal Engine 5.8.3, Visual Studio 2022 (C++ game workloads), Rust 1.99.
  The sim's 58 tests pass.
- The Unreal project `volley-unreal`, next to `volley`: blank C++ template,
  with the MCP and motion-matching plugins enabled. It builds, and the
  editor's MCP server answers on 127.0.0.1:8000. `.mcp.json` and the Epic
  plugin (in `.claude/settings.json`) are set up in it.
- **`volley-unreal` is only on this PC**, in no repository yet.

## Decisions
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
- **Commits** keep the noreply identity, now set in this clone's repo config,
  with no Claude co-author trailer.
- **The Windows Firewall stays on.** The MCP server is localhost-only. The
  editor's prompt to listen on the network (port 1985) can be cancelled.

## Next steps
1. **Open a new Claude Code session in `volley-unreal`** with the editor
   running. Approve the `unreal-mcp` server and install Epic's plugin when
   asked. The session that wrote this note couldn't load them, because it was
   started in another folder.
2. **Milestone 1: the sim in Unreal.** Add `crates/sim_ffi` to `volley` (a
   cdylib wrapping `Sim::new`, `step` and a state snapshot). Load it from a
   C++ plugin in `volley-unreal`, and show the bot match as capsules and a
   sphere. Check that the same inputs give the same events as
   `cargo run -p volley_server`.
3. Then milestones 2–6 as in the plan: arena and look, Cross and motion
   matching, hits and ball, playing it, the side-by-side comparison. Log each
   one in the plan.

## Open questions
- **Realistic or stylized art**: to decide before milestone 3, since it
  changes how much model and retargeting work Cross needs.
- **Where `volley-unreal`'s C++ source lives**: a new repository, or a folder
  in `volley`. Binary assets would need Git LFS, and GitHub's quotas are
  small.
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
