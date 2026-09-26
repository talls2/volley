# Volley

3v3 online beach volleyball with superpowers, built with Rust and Bevy.

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

| | Keyboard & mouse | Xbox controller |
|---|---|---|
| Look and aim | Mouse (click the window to capture it, Esc to release) | Right stick |
| Move | W A S D | Left stick |
| Jump | Space | A |
| Pass / serve / block | Q | RB (or X) |
| Spike (in the air) | E | RT (or Y) |
| Dive | Left Shift | LT (or B) |
| Let a bot play for you | 1 | View |

On a controller, hits are on the bumpers and triggers so your right thumb can
stay on the stick and keep aiming. The on-screen help switches to controller
buttons when you use one.

You play Red with a bot teammate. The camera follows you from behind, and movement
is relative to where it faces. The court is a third bigger than a real one (24 x 12 m).

- **Aiming:** hits go where the center of the screen points. Turn to pick a direction; look higher to hit farther. The yellow ring on the floor shows where your next hit lands (red means out). Passes stay on your side; serves, spikes and third touches go over.
- **Serving:** press pass. Each rally starts with the camera facing the net and the aim mid-way into the other court.
- **Pass:** your team's first touch goes mid-court, the second is a set near the net, and the third goes over automatically.
- **No touching twice in a row:** pass to your teammate, then go spike their set.
- **Spike:** jump, then spike when the ball is in reach.
- **Shot speed follows distance:** short shots are quick and flat, long ones take longer. A short spike from far off the net will hit the net.
- **Block:** at the net, while the ball is on the other side, pass means block: you jump with your hands up (or raise them if already in the air). Your hands stay up for that jump, so timing is everything: go up as the attacker hits. Squarely blocked spikes are stuffed back down on the attackers; edge-of-the-hands blocks pop up softly on your side. A block isn't one of your three touches, and serves can't be blocked.
- **Ball trail:** the streak behind the ball shows the last hit: orange for spikes, gold for serves, blue for passes and digs, green for lobs, purple off a block.
- **Dive:** lunges toward where you're moving (or at the ball), reaching balls near the floor. You're on the ground for a moment afterwards.
- The white rings mark where the ball will land if nobody touches it.
- Presses count for a few frames early, so you don't need frame-perfect timing.

## Art and sound

Placeholder characters and animations are the free Quaternius packs; the library
has no volleyball moves, so passes, spikes and serves borrow the closest motions
it has (`Clip::source` in `crates/client/src/characters.rs`), and blocking arms
are posed in code. The sand is a photo-scanned Poly Haven texture; sounds are
Kenney impacts, recorded ocean waves, a crowd, and a whistle synthesized for the
game. Every source and license is in
[`crates/client/assets/CREDITS.md`](crates/client/assets/CREDITS.md). The crowd
recordings are CC BY 4.0 and need their credit kept when the game ships.

## Next milestones

1. Polish normal volleyball until it's fun (in progress).
2. Hero characters, each with their own powers, strengths and weaknesses.
3. 3v3, rotations.
4. Networking (renet): the server runs the sim; clients send inputs.
