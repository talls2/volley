# Volley

3v3 online volleyball with superpowers, built with Rust and Bevy.

Right now it's a local 2v2 against bots: you and a bot teammate against two bots.
The goal of this stage is polished normal volleyball, Mario Tennis style, before
adding hero characters with powers and then online play.

## Layout

| Crate | What it is |
|---|---|
| `crates/sim` | The game rules. Plain Rust, no engine. Server, client and tests all run this. |
| `crates/client` | The Bevy app: input, 3D view, characters, HUD. Contains no rules. |
| `crates/server` | Headless match loop. Plays bots for now; networking goes here next. |

## Running

```bash
cargo run -p volley_client          # play
cargo run -p volley_server          # watch bots play 2v2 in the terminal (add 1 or 3 for other sizes)
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
| Dive | Left Shift | Y / Triangle |
| Let a bot play for you | 1 | |

You play Red with a bot teammate. The camera follows you from behind, and movement
is relative to where it faces. The court is a third bigger than a real one (24 x 12 m).

- **Serving:** press pass. Movement keys aim it.
- **Pass:** your team's first touch goes mid-court, the second is a set near the net, and the third goes over automatically.
- **No touching twice in a row:** pass to your teammate, then go spike their set.
- **Spike:** jump, then spike when the ball is in reach. Movement keys aim it.
- **Dive:** lunges toward where you're moving (or at the ball), reaching balls near the floor. You're on the ground for a moment afterwards.
- The white rings mark where the ball will land.
- Presses count for a few frames early, so you don't need frame-perfect timing.

## Art

Placeholder characters and animations by [Quaternius](https://quaternius.com), CC0:
the free versions of Universal Base Characters and Universal Animation Library
(licenses in `crates/client/assets`). The library has no volleyball moves, so
passes, spikes and serves borrow the closest motions it has; the mapping is
`Clip::source` in `crates/client/src/characters.rs`.

## Next milestones

1. Polish normal volleyball until it's fun (in progress).
2. Hero characters, each with their own powers, strengths and weaknesses.
3. 3v3, rotations.
4. Networking (renet): the server runs the sim; clients send inputs.
