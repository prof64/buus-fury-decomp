# rhai-story-event — a story challenge that teaches a new scripted move

Chiaotzu now waits in the Southwest Forest (8-3), a few steps left of where you arrive. Talk to him:

1. He asks you to take on one of Babidi's soldiers hiding in the trees (a choice: *Yes* / *Not now*).
2. *Yes* makes the soldier jump out (an enemy the area load never places, created by the event).
3. Beat him: his on-defeat event sets a story flag, gives a Senzu Bean and teaches the leader the **Dodon Ray**
   (skill 27), a new move written in Rhai.
4. Press **L** until the Dodon Ray's icon is selected, then **B**: a fast, piercing ray of ki (hold B for a second
   ray). It works in the field and in battles.

Talking to him again shows his tip (won) or a warning (the soldier is still out).

| file | what it does |
|---|---|
| `pack.json` | the mod's id and name |
| `world/areas/8-3+chiaotzu.json` | **appends** two objects to the base area 8-3 (`add_objects`, the `+name` file name keeps it from replacing other mods' 8-3 files): Chiaotzu (an `npc` whose `talk` runs event `s930`) and the soldier (an `enemy` with `condition {"value": 1}` = only created by script, an explicit list index 200 for `spawn_map_object(200)`, `on_defeat` event `s931`) |
| `scripts/events/s930.rhai` | the talk: `say` and `choice` nodes, `get_flag` / `get_menu_result` branching (a code node returning a number jumps to that node), `set_flag(1130)`, `spawn_map_object(200)` |
| `scripts/events/s931.rhai` | the reward: `set_flag(1131)`, `learn_skill(get_character(), 27)`, `give_item(21)` |
| `skills/27_dodon_ray.json` | the new skill: name, EP (`check` / `cost`), damage, cooldown, and `engine.script` = its behaviour |
| `hooks/skills/dodon_ray.rhai` | the behaviour: `on_start` / `on_frame` hooks that *ask* for effects (`fire`, `anim`, `sfx`, `spend`) |
| `projectiles/dodon_ray.json` | the ray: sheet, frames per direction, speed, life, `pierce`, hit box, damage, wall behaviour |
| `effects/mod_dodon/` | the ray's graphics: a base effect (`$extends`) with a yellow recolour of its sheet (`.derive.json`) - no game art copied |
| `icons/skills/skill_27*.png.derive.json` | the HUD icon (selected / not), recoloured from the Energy Blast's |

Ids it uses (mods should pick their own): events 930 / 931, story flags 1130 / 1131, skill 27, enemy list index 200.

Try it:

```
buus-fury --mod engine/mods-examples/rhai-story-event --story 8-3                 # play (window)
buus-fury --headless --mute --mod engine/mods-examples/rhai-story-event --story 8-3 --frames 120 --shot out.ppm
cargo run -p wf-app --example modcheck -- <content/base> engine/mods-examples/rhai-story-event
cargo test -p wf-app --test mods rhai_story_event     # the whole scenario, headless and muted
```
