# Animation research

What we learned about making Volley's animation feel right, from other games,
the craft of game animation, and real volleyball. Each claim links its source;
**[unsourced]** and **[derived]** mark our own reasoning. Experiments that try
these ideas in Volley, with measurements, are in [experiments.md](experiments.md).

## Knockout City (Velan Studios, 2021)

Velan never published how its animation was made: no talk, no animator
interview. What's known:

- **Gameplay animation was hand-keyed, by one person.** The lead gameplay
  animator, Derek Bonikowski (ex-Retro Studios), keyframed every gameplay
  animation ([reel](https://vimeo.com/557330318)). Nothing mentions motion
  capture. Two more animators made intros, victories, defeats and seasonal
  emotes ([ArtStation Art Blast](https://magazine.artstation.com/2021/09/velan-studios-knockout-city-art-blast/)).
  The reel is short modular clips chained together ("run backward, catch,
  throw, run forward").
- **Tools:** Maya with Animbot (retiming), Studio Library (a shared pose
  library), bhGhost (onion skins), mGear rigs
  ([Velan blog](https://medium.com/velan-studios/tip-of-the-brush-animations-tools-of-the-trade-90cc5fc1c783)).
  Animation styles were "discussed, dissected, and made repeatable" (art
  director Ben Greene, Art Blast).
- **Throw and catch feel took about 18 months of prototyping**, with no art
  direction at first ([interview](https://godisageek.com/2021/07/velan-studios-we-knew-that-if-they-got-their-hands-on-it-theyd-love-it-knockout-city-interview/),
  [Velan blog](https://medium.com/velan-studios/innovation-and-knockout-city-41a7e76ea763)).
  The cartoony style followed from the mechanics: a realistic look couldn't
  survive players curling into balls ([Xbox Wire](https://news.xbox.com/fr-fr/2021/06/09/knockout-city-game-pass-velan-studios-interview/)).
- **Engine:** a deterministic, rewindable simulation at a fixed rate, drawn by
  interpolating between steps ([Introducing Viper](https://medium.com/velan-studios/introducing-viper-273d41b8c507),
  [GDC 2022](https://gdcvault.com/play/1027634/-Knockout-City-s-Parallel)).
  Volley is built the same way (fixed `Sim` ticks, blended drawing).
- **What to predict online:** your own catch is predicted locally so it feels
  instant; other players' catches and knockouts wait for the server, because
  undoing a knockout "feels terrible". Designers, animators, VFX, SFX and
  engineers decided together, mechanic by mechanic
  ([Catch This](https://medium.com/velan-studios/catch-this-the-magic-behind-knockout-citys-satisfying-gameplay-9b3965717f7d)).
- **Patch notes** show animation-tied cancel windows tuned after launch (a fake
  throw cancelling too fast, catch cooldown after a dodge) and directional
  locomotion blending ([fan copy](https://knockoutcity.fandom.com/wiki/Patch_Notes)).

### Level design (Willem Kranendonk, Rooftop Rumble)

From [his write-up](https://kranendonk-willem.medium.com/knockout-city-conception-to-execution-of-an-original-multiplayer-level-9fbb816dfc40):
a playable blockout in about a day; daily playtests asking pointed questions
("did you feel powerful here?", "where were you lost?") instead of "did you
like it?", fed into a living traversal-metrics document; every map gets one
mechanic, and it should solve a problem the map has (Rooftops' updrafts made
the risky glide between buildings learnable); the bounce boundary has to read
clearly against background art.

## Other arena ball games

- **Rocket League:** fixed 120 Hz physics ("larger penetrations = inconsistent
  hits"); collision shapes are a few shared presets, deliberately not matching
  the visuals; the client predicts everything, the ball included, and replays
  corrections ([GDC 2018 slides](https://media.gdcvault.com/gdc2018/presentations/Cone_Jared_It_Is_Rocket.pdf)).
- **Lethal League Blaze:** hit-pause makes what you hit feel resistant, and it
  grows with ball speed; animation moved from full 60 fps to hand-picked poses
  chosen for silhouette ([Game Developer](https://www.gamedeveloper.com/design/developing-the-stylish-indie-hit-fighting-game-i-lethal-league-blaze-i-)).
- **Rematch / Sifu (Sloclap):** animation "comes from action games more than
  sports sims", credibility over realism ([interview](https://www.pockettactics.com/rematch/interview-sifu-2));
  the ball is "the basic brick": tune it early and rarely, players learn
  distances from it ([PC Gamer](https://www.pcgamer.com/games/sports/rematch-devs-talk-balls-specifically-how-to-balance-the-balls-feel-with-function-its-the-basic-brick-of-the-game-its-like-an-atomic-component/)).
  Sifu was mocapped, then impacts were keyed by hand because actors couldn't
  really hit each other ([source](https://eip.gg/sifu/news/sloclap-brought-in-a-kung-fu-master-to-choreograph-combat/)).
- **Nintendo Switch Sports volleyball:** bump and set as the ball reaches you,
  spike just after its peak; chaining good touches unlocks an unblockable fast
  spike ([Nintendo](https://www.nintendo.com/jp/ichikara/as8sa/03_en.html)).
  Characters with attached arms needed 650+ motions against Wii Sports' 30
  ([Nintendo](https://www.nintendo.com/ph/interview/as8s/03.html)): IK and
  warping are what keep Volley's clip count small.
- **Spike Volleyball (Black Sheep / Bigben, 2019):** spikes and serves from
  160-camera mocap ([ActuGaming](https://www.actugaming.net/deux-videos-presentent-le-motion-capture-de-spike-volleyball-disponible-a-la-fin-du-mois-188630/)),
  yet reviews saw balls pass 20 cm from hands. Contact sync matters more than
  capture quality.
- **Omega Strikers:** reach is a hero stat, not an animation contact; unlit
  arenas keep characters and ball readable ([wiki](https://omegastrikers.wiki.gg/wiki/Omega_Strikers_Playbook),
  [artist](https://jasonlavoie.net/projects/2q236a?album_id=25211)).
- **Super Buckyball Tournament:** effects must never hide the ball or a
  character ([devlog](https://patheagames.itch.io/superbuckyballtournament/devlog/160873/sbt-dev-log-special-effects)).

## The craft

- **Anticipation, apex, recoil** (Jonathan Cooper, *Game Anim*): keep the
  player's anticipation short, other characters' longer so they read; agree
  timing rules early; feel beats looks ([gameanim.com](https://www.gameanim.com/?p=115),
  [five fundamentals](https://www.gameanim.com/?p=52801)).
- **Pushing mocap:** on Assassin's Creed III every take was sped up 15% to
  start, then animators exaggerated poses and timing for impact
  ([Cooper](https://www.gameanim.com/2014/02/11/animating-3rd-assassin/7/)).
  Corrections go on a layer keyed to zero a few frames either side of the
  fix, so it blends into the capture.
- **Retiming limits:** For Honor kept clips within 10% faster and 20% slower,
  sliding the body and fixing feet with IK instead; mark the exact event frame
  ([Clavet notes](https://www.gameanim.com/?p=13538)). Volley's timed swings
  allow 0.3x to 4x, far beyond this.
- **Hit-stop:** Smash freezes ⌊damage·0.65+6⌋ frames, capped at 30, and shakes
  the characters during it ([wiki](https://www.ssbwiki.com/Hitlag),
  [Sakurai](https://sourcegaming.info/2015/11/11/thoughts-on-hitstop-sakurais-famitsu-column-vol-490-1/)).
- **Transitions:** inertialization ([Bollo, GDC 2018](https://gdcvault.com/play/1025165/Inertialization))
  and dead blending ([Holden](https://theorangeduck.com/page/dead-blending))
  switch to the new clip at once and let the old pose's offset fade.
- **Warping:** distance matching, stride and orientation warping
  ([Paragon](https://www.gameanim.com/?p=14556), [UE docs](https://dev.epicgames.com/documentation/en-us/unreal-engine/pose-warping-in-unreal-engine));
  code-driven capsule with the visual root following within limits
  ([Holden](https://theorangeduck.com/page/code-vs-data-driven-displacement)).
- **Few keys plus procedure:** Overgrowth animated with about 13 keyframes,
  springs and IK ([Rosen, GDC 2014](https://www.gamedeveloper.com/design/video-an-indie-approach-to-procedural-animation)).
  The Cartoon Animation Filter adds anticipation and follow-through to mocap
  curves automatically ([Wang 2006](https://grail.cs.washington.edu/wp-content/uploads/2015/08/wang-2006-tca.pdf)).
- **Bevy additive blending** (bevy_animation 0.19 `animatable.rs`): an `Add`
  input applies `rot = slerp(I, q_add, w) * rot`, `t += w·t_add`, `s += w·s_add`;
  additive clips must hold deltas (zero scale, not one).

## Real volleyball

From sports science, for posing (14 NCAA players unless noted):

- **Spike at contact:** shoulder abducted about 130°, arm 30° in front of the
  shoulder line, elbow bent about 35° ([Reeser 2010](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC3445065/fullTextXML)).
  The hand is above and slightly in front of the hitting shoulder; contact
  comes at the top of the jump ([Kuhlmann 2007](https://ojs.ub.uni-konstanz.de/cpa/article/view/393/333)).
  Cocking to contact takes about 100 ms; the hand touches the ball about 20 ms,
  one frame ([Howard 2023](https://jssm.org/jssm-22-488.xml-Fulltext)). The
  shoulders are turned back about 34° while cocking and square at contact; the
  free arm pulls down across the body; the follow-through goes to the opposite
  hip. Standing reach is 1.29× height ([Fuchs 2019](https://iris.unicas.it/bitstream/11580/91401/1/02640414.2019.pdf)).
- **Set at contact:** hands in a triangle just over the forehead, thumbs toward
  the eyes, wrists bent back (inside angle 118–130°); elbows about 140° at
  contact and 160° at release; knees about 141° at their lowest, 164° at
  contact; trunk vertical, head tipped back. The ball's centre sits 33–34 cm
  from the forehead at 55–62° above horizontal: **[derived]** about 27 cm above
  and 18 cm in front. The ball stays on the hands about 70 ms, 4–5 frames
  ([Ridgway & Hay](https://ojs.ub.uni-konstanz.de/cpa/article/view/1463/1335),
  [Ridgway & Wilkerson](https://ojs.ub.uni-konstanz.de/cpa/article/view/1517/1422)).
- **What looks wrong:** arm straight up by the ear; hitting behind the head;
  a slow forward swing; shoulders square while cocking; a dangling free arm;
  a set at the chin with flat palms, or with straight legs.
- **Bicycle kick:** trunk horizontal, the free leg goes up first, the kicking
  knee snaps straight just before contact ([Shan 2008](https://ojs.ub.uni-konstanz.de/cpa/article/view/1930/1798)).

## Ideas, ranked for Volley

1. Hand-keyed contact accents over the mocap (Sifu, Lethal League, AC3).
   Done: experiments 02–04.
2. Hit-stop that grows with the hit (spikes most, sets barely), with the
   hitter shaking through it (Lethal League, Smash). Done: experiment 05.
3. Inertialization for snappy cuts into hits. Tried: experiments 06–07, no
   better than our crossfades; kept off behind `VOLLEY_BLEND=inertia`.
4. Orientation and stride warping on the existing body layering and IK.
   Orientation done: experiment 11 (and 09, legs that run under hits);
   instead of stride warping, foot locking with leg IK: experiment 12.
5. Chain touch quality into the spike (Switch Sports). Done: a clean
   reception and a clean set make a "chain spike", cleaner and faster
   (`CHAIN_QUALITY`, `CHAIN_SPEED` in the simulation), with its own trail,
   burst, sound and callout.
6. Keep gameplay reach and timing as data, apart from the clips (Rocket
   League, Windjammers 2, Omega Strikers).
7. Animation state as a pure function of the simulation, for online play.

## Metrics we can measure in-game

Contact error (palm to ball), contact timing (where the swing was at the
touch), retime factor, foot sliding, pose extension at contact, frame time;
and playtest questions: could you tell the moment of contact? did it feel
late after pressing? could you tell a spike from a set before contact?
