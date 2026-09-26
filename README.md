# Volley

3v3 online volleyball with superpowers, built with Rust and Bevy.

Right now it's at milestone 1: one player per team, played locally, no powers or
networking yet. The question this milestone answers: **does hitting the ball feel good?**

## Layout

| Crate | What it is |
|---|---|
| `crates/sim` | The game rules. Plain Rust, no engine. Server, client and tests all run this. |
| `crates/client` | The Bevy app: input, 3D view, characters, HUD. Contains no rules. |
| `crates/server` | Headless match loop. Plays bots for now; networking goes here next. |

## Running

```bash
cargo run -p volley_client          # play
cargo run -p volley_server          # watch bots play in the terminal (add 3 for 3v3)
cargo test -p volley_sim            # rules tests
```

The first build compiles Bevy and takes several minutes; after that it's quick.
Run the client with `cargo run`: Bevy finds `crates/client/assets` through the
variables cargo sets, so launching the binary directly won't load the characters.

## Controls

| | Keyboard & mouse | Gamepad |
|---|---|---|
| Look | Mouse (click the window to capture it, Esc to release) | Right stick |
| Move | W A S D | Left stick |
| Jump | Space | A / Cross |
| Pass / serve | Q | X / Square |
| Spike (in the air) | E | B / Circle |
| Let a bot play for you | 1 | |

You play Red against a bot. The camera follows you from behind, and movement is
relative to where it faces.

- **Serving:** press pass. Movement keys aim it.
- **Pass:** sends the ball high to your side of the net, setting you up. Your third touch goes over automatically.
- **Spike:** jump, then spike when the ball is in reach. Movement keys aim it.
- The white rings mark where the ball will land.
- Presses count for a few frames early, so you don't need frame-perfect timing.

## Art

Placeholder characters and animations by [Quaternius](https://quaternius.com), CC0:
the free versions of Universal Base Characters and Universal Animation Library
(licenses in `crates/client/assets`). The library has no volleyball moves, so
passes, spikes and serves borrow the closest motions it has; the mapping is
`Clip::source` in `crates/client/src/characters.rs`.

## Next milestones

1. Tune the feel until hitting is fun.
2. Networking (renet): the server runs the sim; clients send inputs.
3. Superpowers.
4. 3v3, rotations, better bots.
