# Alysa, the ice skater

A planned hero; nothing is built yet. Her concept art is still to be
generated, from the prompt below.

## Who she is

A figure-skating world champion who left the rink for the arena. She skates
on sand: every stride freezes a strip of ice under her blades, and she
glides on it. She is precise, cool under pressure and a little smug, and
plays volleyball like a routine she has rehearsed a thousand times.
- **Hero color:** glacier cyan.
- **Her mark:** a six-pointed snowflake crest.

## Kit

Like every hero: stats, passives, an ability on a cooldown and an ultimate
charged by touches and points.

- **Stats** (compared with Cross at 9.2 / 7.6 / 16):
  - fastest on the ground: run 9.6 m/s, dash 18 m/s;
  - the lowest jumper: 6.8 m/s off the ground.
- **Glide** (passive): she keeps her momentum. She accelerates fastest and
  carves wide turns, but she can't stop dead. A hard stop or reversal skids
  about a meter, and so does landing from a jump.
- **Wall Skate** (passive): skating into a wall, she rides along it instead
  of stopping, for up to a second and a half. She can jump off it at full
  height (a wall jump in any direction) or hit the ball from it. This opens
  attacks along the side walls that only she can reach.
- **Ice Trail** (passive): her dash leaves a strip of ice for 3 s. Anyone on
  it, either team, turns and stops like she does. She can trap a setter's
  approach, or get trapped by her own.
- **Flash Freeze** (ability, 8 s cooldown): armed like an attack. The next
  ball she hits freezes in mid-air for half a second, frosted and glittering,
  then drops on its way. It breaks the receivers' timing; a reception's
  quality depends on timing (`reception.rs`), so a frozen ball is a tough
  read.
- **Triple Axel** (ultimate): a spinning leap, higher and floatier than her
  jump, into a spike that turns the ball to ice.
  - The ice ball is heavy: fast and steep, and a block it hits shatters,
    knocking the blockers back.
  - Where it lands it bursts into a ring of ice, 3 m across, for 2 s. Anyone
    inside is on ice, and a dig from there has a stretched-reception
    penalty.
- **Brittle** (weakness): no dive. Her skates can't throw her flat out, so a
  ball out of reach needs a foot save or a slide on the ice instead. Her
  digs off hard balls are a little less controlled too.

The kit works with the breaking walls idea (README, "Ideas"): Wall Skate
leans on walls, and a cracked panel she rides might give way under her.

## How she moves

Animation notes for her clips:
- She runs with long, leaning skating strides, arms swinging wide.
- She glides with one leg extended behind.
- She stops with a spray of ice and the blades turned sideways (a hockey
  stop).
- Her jumps take off from one blade with a spin.
- Her spike is a spinning arm swing.
- She celebrates with a spin and a pose, arms up.

## Concept art prompt

For the concept sheet, in the layout of `cross.webp` and `golazo.webp`. These
go through Tripo for the 3D model, so the turnaround matters most.

**Image settings:** 3:2 landscape, **1536 × 1024 px** (or the closest your
tool offers, e.g. 1792 × 1024), PNG.

> Character concept sheet for a stylized sports video game hero, 3:2 landscape, 1536x1024, clean light grey background, the style of a modern stylized hero game (bold readable shapes, saturated colors, soft painterly shading with crisp edges, slightly exaggerated proportions), professional concept art presentation. The character: Alysa, an original character (not resembling any existing film or game character or any real person), a figure-skating world champion turned volleyball star with ice powers, an athletic woman in her mid-twenties, lean and strong with a skater's long legs, fair skin with a faint cool flush on the cheeks, sharp pale-blue eyes, platinum-blonde hair in a high tight bun with an undercut shaved on one side and frosted ice-blue tips, a few loose strands, frost glittering on her lashes, a small snowflake stud earring. Outfit: a sleek sleeveless high-neck athletic skating bodysuit in white and glacier cyan with angular crystalline panels and thin glowing icy-cyan trims, a six-pointed snowflake crest on the chest and the number 01 on the hip, fitted white athletic shorts over it, a translucent frosted cyan wrap on one forearm, fingerless white gloves, white figure-skating boots with icy cyan laces whose blades are made of glowing translucent ice, frost mist curling off the blades. Layout: on the left, a large full-body hero pose standing on sand, weight on one blade, one hand lifting a volleyball that is frosting over, ice crystals forming on the sand under her feet; top right, a full turnaround of front, right side, back and left side views in a neutral A-pose with arms slightly out, labeled FRONT, RIGHT, BACK, LEFT in small grey text; at the far right, two head close-ups (three-quarter face, and profile showing the undercut); bottom right, an action scene in a futuristic neon beach volleyball arena at sunset with glass walls and a crowd, Alysa skating sideways along a glass wall on her ice blades leaving a glowing frozen trail, then leaping into a spinning spike with an ice-encased volleyball, blockers in white jerseys reaching up, shards of ice and sand spraying. No other text, no logos besides the crest and number, no watermark.
