# Animation research and experiments

A running record of how Volley's animation gets better: what other games and
real volleyball teach us, what we tried, how it measured, and what we kept.

- [research.md](research.md): findings with sources. Knockout City, arena ball
  games, the craft of game animation, volleyball biomechanics, and a ranked
  list of ideas for Volley.
- [experiments.md](experiments.md): one entry per experiment, each with its
  idea, how it was done, the measurements against the one before, and the
  decision.
- `bench/`: the raw measurements, `NAME-N.json` per run.
- `images/`: before and after pictures.

## Measuring

The game has a bench mode (`crates/client/src/bench.rs`): it plays a bot match
on its own and writes how close hands and feet come to the ball at each touch,
how well swings are timed, and frame times. The match is the same every run.

    python3 tools/bench.py run NAME            # three runs, docs/animation/bench/NAME-N.json
    python3 tools/bench.py compare BEFORE AFTER
    python3 tools/bench.py ab BEFORE AFTER     # last commit vs the working tree, alternating

To compare two settings fairly, run them together: runs alternate (A, B, A,
B...), so both see the same machine state. `NAME:VAR=VALUE` sets an
environment variable for that variant:

    python3 tools/bench.py run 06-crossfade 06-inertia:VOLLEY_BLEND=inertia

Or one run by hand, with screenshots of the first passes, spikes and serves
(the screen has to be awake and unlocked):

    VOLLEY_BENCH=/tmp/run.json VOLLEY_BENCH_SHOTS=/tmp/shots cargo run -p volley_client

Other settings: `VOLLEY_BENCH_SECONDS` (default 90), `VOLLEY_BENCH_ARENA`
(`neon` or `beach`), `VOLLEY_BENCH_HERO` (the hero you play, 0 to 2),
`VOLLEY_BENCH_CAM=action` (a close camera beside whoever plays the ball
next, cutting between players, to judge animation), `VOLLEY_BENCH_REPLAY`
(instant replays play, as in a real match; off otherwise so every run is
the same match).

Besides contact and timing, the bench reports hand speeds (pops), how far
planted feet slide (a foot is planted when its lowest point, ankle or ball,
is within 3 cm of the lowest it gets), the torso's lean (`folded`: standing
bodies bent over 60°), and knees pointing away from the body.

A film: the game slows down while filming so every frame can be saved, and
the frames stitch into a smooth real-speed video (`log.txt` beside the
frames says which tick each frame shows, and every touch):

    VOLLEY_BENCH=/tmp/run.json VOLLEY_BENCH_SECONDS=14 VOLLEY_BENCH_FILM=/tmp/film \
        VOLLEY_BENCH_FILM_FROM=1 VOLLEY_BENCH_FILM_SECONDS=12 caffeinate -d -i cargo run -p volley_client
    ffmpeg -framerate 30 -i /tmp/film/%04d.png -vf scale=1280:-2 -pix_fmt yuv420p film.mp4

## Adding an experiment

1. Change one thing.
2. `python3 tools/bench.py run NN-short-name`, then compare with the last kept
   experiment. For frame times, or anything close, alternate both in one run.
3. Look at it: `retarget_mocap.py --preview` contact sheets for clips, the
   bench's screenshots in game.
4. Write the entry in experiments.md: idea, how, result (with the ± noise),
   decision. Keep it or revert it.
