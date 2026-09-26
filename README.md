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
| Attack (in the air) | E | RT (or Y) |
| Dive | Left Shift | LT (or B) |
| Foot save | F | LB |
| Pause | P | Menu |
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
- **Attack:** jump and press attack any time in the air: it stays armed until you land, steers you toward the ball, and hits it at the best moment it's in reach, with whatever reaches it. A ball overhead in front gets a **spike**, a low one a **volley kick**, and one behind your head a **bicycle kick**. The farther the ball is from that technique's sweet spot (a jump timed early or late, a ball out to the side), the weaker and wilder the hit: it flies slower and can land well off your aim, shown by a wider outer ring around the aim marker. A callout rates each attack, from "perfect!" to "scrambled".
- **Shot speed follows distance:** short shots are quick and flat, long ones take longer. A short spike from far off the net will hit the net.
- **Block:** at the net, while the ball is on the other side, pass means block: you jump with your hands up (or raise them if already in the air). Your hands stay up for that jump, so timing is everything: go up as the attacker hits. Squarely blocked spikes are stuffed back down on the attackers; edge-of-the-hands blocks pop up softly on your side. A block isn't one of your three touches, and serves can't be blocked.
- **Ball trail:** the streak behind the ball shows the last hit: orange for spikes, red for volley kicks, violet for bicycle kicks, gold for serves, blue for passes and digs, green for lobs, purple off a block.
- **Low balls:** you can't bump a ball below your knees. A **foot save** keeps you on your feet: it shoots a leg out almost instantly, reaches farther than a pass (but only low balls), and kicks the ball up high so a teammate has time to get there; it's rougher and you stumble for a moment. A **dive** reaches farthest but leaves you on the ground. When a save is the right move, a prompt says so ("Foot save! [F]", "Dive! [Shift]").
- **Dive:** lunges toward where you're moving (or at the ball), reaching balls near the floor. You're on the ground for a moment afterwards.
- The white rings mark where the ball will land if nobody touches it.
- Presses count for a few frames early, so you don't need frame-perfect timing.

## Match rules

Beach volleyball: sets to 21 won by two, a deciding third set to 15, best of
three. Teams switch sides every 7 points (every 5 in the deciding set). The
team that loses a set serves first in the next. A title screen starts the
match; a match-over screen offers a rematch.

## Moves and kits

Everything a player does to the ball is a *move*, described as data in
`crates/sim/src/moves.rs`: its button, whether it's done on the ground or in
the air, its windup, active and recovery time, where it can reach the ball,
any lunge, and what a touch does (keep it for a teammate, or attack over the
net), how accurately, and how long it hangs. Attacks pick their technique and
contact quality in `crates/sim/src/attack.rs`. A *kit* is a set of moves plus
stats like run speed and jump height. Everyone uses the All-rounder kit for
now; heroes will each be a kit with their own special moves.

## Art and sound

Placeholder characters are the free Quaternius packs, and running, jumping and
landing come from its animation library. The volleyball moves (ready stance,
bump, set, spike, volley and bicycle kicks, serve, block, dive, foot save, cheer)
are our own, authored for the same skeleton by a Blender script:

    ~/Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup \
        -P tools/blender/volley_animations.py -- --preview /tmp/volley-anims

It writes `crates/client/assets/animations/Volley.glb`, and with `--preview`,
a contact sheet per clip. Each clip is a few key poses in character terms
(where the wrists and ankles go, how the hips and spine turn), solved with IK
and baked. In game, arms still bend a little toward the real ball, and a foot
save's leg reaches for it. The sand is a photo-scanned Poly Haven texture; sounds are
Kenney impacts, recorded ocean waves, a crowd, and a whistle synthesized for the
game. Every source and license is in
[`crates/client/assets/CREDITS.md`](crates/client/assets/CREDITS.md). The crowd
recordings are CC BY 4.0 and need their credit kept when the game ships.

## Next milestones

1. Polish normal volleyball until it's fun (in progress).
2. Hero characters, each with their own powers, strengths and weaknesses.
3. 3v3, rotations.
4. Networking (renet): the server runs the sim; clients send inputs.
