# Volley

3v3 online arena volleyball with superpowers, built with Rust and Bevy: a mix
of Knockout City and Rematch on the sand. Like Rematch isn't quite soccer and
Rocket League isn't quite anything, Volley keeps volleyball's net and three
touches but plays in a big walled arena built for dashes, leaps and powers.

Right now it's a local 3v3 against bots: you and two bot teammates against three
bots, with heroes picked before each match. Online play comes later.

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
| Look | Mouse (click the window to capture it, Esc to release) | Right stick |
| Move | W A S D | Left stick |
| Jump (hold for full height) | Space | A |
| Dash | C | Left stick click |
| Pass / block (hold for power) / serve (hold, release) | Q | RB |
| Attack (in the air; hold for power) | E | RT |
| Dive | Left Shift | LT (or B) |
| Foot save | F | LB |
| Hero ability | R | X |
| Hero ultimate | G | Y |
| Pause | P | Menu |
| Let a bot play for you | 1 | View |

On a controller, hits are on the bumpers and triggers so your right thumb can
stay on the stick and keep aiming. The on-screen help switches to controller
buttons when you use one.

Before each match you pick a hero. You play Red with an All-rounder bot
teammate, against Cross and an All-rounder. The camera follows you from behind, and movement
is relative to where it faces. The arena is 48 x 24 m, more than twice a real court, with glass walls all
around: the ball bounces off them and stays in play, so nothing is ever out.
It only ends when it hits the sand, and that side loses the point.

- **Movement:** a running jump goes about a third higher than a standing one, so run in to attack. Tap jump for a short hop, hold it for the full jump. **Dash** bursts about three meters along the sand toward where you're moving (you can hit or jump out of it; jumping out keeps its speed for a flying approach) and then needs a moment to recharge. Landing fast skids a little in the sand.
- **Aiming, Rematch style:** hits go the way you're moving: hold back and a pass goes behind you, hold left and it goes left. Standing still, they go the way the camera looks. Press pass and you hit the ball the moment it's in reach, so pressing a little early is fine: holding keeps the pass waiting for the ball, and a quick tap is a soft toss. The longer the button has been held when the ball arrives, the harder and farther the hit (full power in under a second, shown by a bar); letting go keeps the power reached. Attacks work the same way, armed for the rest of the jump. The yellow ring on the floor shows where it will land, sliding out as you charge. Hits over the net (serves, attacks, third touches) always go toward the other side, keeping your angle across. Bank shots off the walls are fair game.
- **Serving:** hold pass and let go: a tap drops it just past the net, a full charge sends it to the back wall. Each rally starts with the camera facing the net.
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

Volleyball's scoring, beach style: sets to 21 won by two, a deciding third set to 15, best of
three. Teams switch sides every 7 points (every 5 in the deciding set). The
team that loses a set serves first in the next. A title screen starts the
match; a match-over screen offers a rematch.

## Heroes

Each hero is a sports star from another field who happens to play volleyball,
with their own stats, passives, ability (on a cooldown) and ultimate (charged by
touching the ball and winning points). A gold ring under a player means their
ultimate is ready.

- **All-rounder**, beach volleyball pro: solid everywhere, with the foot save; no ability or ultimate.
- **Cross**, pro basketball superstar: quicker and a higher jumper, but no foot save.
  - *Dribble* (passive): once per possession, two touches in a row without a double-touch fault. Set yourself for a self alley-oop.
  - *No-look* (passive): defenders read his hits late, and he doesn't turn toward where the ball goes.
  - *Crossover* (ability, 7 s): armed in the air like an attack; when the ball arrives he palms it (a carry only he gets away with), swings it across his body while hanging and shifting about a meter sideways (the way you're moving), then spikes. Blockers lined up on him jump at the wrong spot and the wrong time.
  - *Posterizer* (ultimate): a leap about twice as high with hang time, steering to the ball from far away; the dunk goes through any block and knocks the blockers down for a second.

## Moves and kits

Everything a player does to the ball is a *move*, described as data in
`crates/sim/src/moves.rs`: its button, whether it's done on the ground or in
the air, its windup, active and recovery time, where it can reach the ball,
any lunge, and what a touch does (keep it for a teammate, or attack over the
net, carry it, or dunk it), how accurately, how long it hangs, and for hero
moves their cooldown, ultimate charge, leap, hang time and air steering.
Attacks pick their technique and contact quality in `crates/sim/src/attack.rs`.
A *kit* is a hero: moves, passives, and stats like run speed, jump height and
dash speed. Adding a hero is mostly a new kit (plus any new kind of touch or
passive it needs), a card in `crates/client/src/heroes.rs`, and its animations.

## Art and sound

Placeholder characters are the free Quaternius packs, and running, jumping and
landing come from its animation library. The volleyball moves (ready stance,
takeoff, dash, bump, set, spike, volley and bicycle kicks, serve, block, dive,
foot save, cheer) and hero moves (Cross's crossover and dunk, getting knocked down)
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
