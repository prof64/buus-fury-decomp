# Modding guide

Everything the Buu's Fury engine plays comes from plain files: the **base pack** the extractor writes from your
own ROM (`content/base/`, never edited) and any number of **mods** layered on top. A mod is a folder of JSON, PNG,
WAV, Tiled maps and [Rhai](https://rhai.rs) scripts with the same layout as the base pack, holding only what it
adds or changes. No code, no ROM bytes, no rebuild.

Nine complete examples live in [`engine/mods-examples/`](../mods-examples) — each has a README and an automated
test (`crates/wf-app/tests/mods.rs`):

| mod | shows |
|---|---|
| [`balance-tweak`](../mods-examples/balance-tweak) | merge patches: enemy stats, a skill's EP cost and damage |
| [`new-skill`](../mods-examples/new-skill) | a new skill id on an existing behaviour, own cost / damage / frame data, derived HUD icon, given to Goku |
| [`new-character`](../mods-examples/new-character) | a new fighter: `$extends` sprite + palette-remapped sheet, own form, stats, growth, skills, roster entry |
| [`new-arena-scenario`](../mods-examples/new-arena-scenario) | a Rhai story scenario: NPC with a choice, warp to a new arena area, three bosses chained by on-defeat events, dialogue with portraits, a prize and a flag |
| [`new-area`](../mods-examples/new-area) | a new field area: Tiled map on a base tileset, row collision, map exits both ways, an NPC with a Rhai talk script, its own area-name banner |
| [`scripted-skills`](../mods-examples/scripted-skills) | new skill *behaviours* in Rhai (§4b): Homing Volley (hold to gather orbs, release to fire a homing fan) and Teleport Strike (vanish, reappear behind the nearest enemy, heavy blow); a new projectile kind with a derived sheet; EP costs and cooldowns |
| [`new-boss`](../mods-examples/new-boss) | a new enemy *type* (150): recoloured sprite, own stats, an AI tree of the game's node kinds plus scripted nodes (§4c), a phase change at half HP into a volley of homing dark orbs |
| [`new-party-member`](../mods-examples/new-party-member) | a story-party character (10, Tien): record, growth, form, equipment; joins through an NPC's event and can take the lead (§5b) |
| [`hud-layout`](../mods-examples/hud-layout) | `ui/layout.json` (§6b): the HUD moved to the bottom-right corner, a recoloured skill-icon set, the area banner at the top |

## 1. Folder, pack.json, loading

```
my-mod/
  pack.json        {"id": "my-mod", "name": "…", "format_version": 1, "depends": ["base"], "load_after": ["other-mod"]}
  …                files at the same paths as in content/base/ (CONTENT.md §1)
```

* **Native**: drop the folder into `<data dir>/wf-buu/content/mods/` (macOS `~/Library/Application Support/wf-buu`),
  or pass `--mod path/to/my-mod` (repeatable; `--no-mods` ignores the mods folder). The log prints the load order.
* **Web**: Settings → Mods → *Add mod folder…* (the folder is stored in the browser next to the extracted content;
  `files.json` and a missing `pack.json` are generated). Remove it there too.
* **Load order**: base, then mods sorted by `depends` / `load_after`, ties by `id`. A later pack wins. A mod whose
  `depends` is missing is skipped with a warning.

## 2. Overlay rules (per file path)

| you ship | effect |
|---|---|
| `x.json` (or any file) at a base path | replaces the earlier version entirely |
| `x.patch.json` | [RFC 7396 merge patch](https://www.rfc-editor.org/rfc/rfc7396) onto `x.json`: objects merge recursively, `null` deletes a key, arrays are replaced whole |
| `x.delete` (empty) | removes `x`; `dir.delete` removes everything under `dir/` |
| new path | adds content (new ids are new files) |
| `x.json` with `"$extends": "other/path.json"` | **new record from an existing one**: the other file (merged, with all mods applied) merge-patched with the rest of this object. E.g. a sprite with Goku's frames: `{"$extends": "sprites/106_goku/sprite.json", "id": "900_x", "sprite_index": 900}` |
| `x.png.derive.json` = `{"from": "other.png", "recolor": {"#rrggbb": "#rrggbb", …}}` | **a derived asset**: a copy of another merged file, for PNGs with an exact colour remap (indexed PNGs: palette entries, so tilesets stay indexed; RGBA sheets: pixels, alpha kept). `from` may be the same path (recolour the base file in place). Without `recolor` it is an alias of any file type |

`$extends` and `.derive.json` are how a mod reuses base art and data *by reference*: the extracted ROM content
never has to be copied into a mod, so mods can be shared without shipping game data.

Several mods appending to the same area: name the area file `world/areas/<W-A>+<anything>.json` with
`"key": "W-A"` (§5) — different paths never replace each other.

## 3. Ids and units

* File names are ids; numeric game ids are fields. Pick new numbers above the base game's:

  | thing | base uses | field | mods |
  |---|---|---|---|
  | skill | 1..23 | `skills/*.json` `skill` | 24..255 (HUD icon `icons/skills/skill_NN.png` + `_alt`) |
  | sprite | 0..296 | `sprites/<id>/sprite.json` `sprite_index` | 300+ (forms use `gfx_id` = sprite + 9; NPCs use script sprite id = sprite + 9) |
  | form | 0..53 | `forms/*.json` `index` | 54+ |
  | character | 0..8 | `characters/*.json` `index` | 9..63 (battle fighters and story party members, §5b) |
  | enemy type | 0..149 | `enemies/*.json` `type` | 150+ (new types, §4c) or patch existing ones |
  | AI state | 0..0x33B | `ai/state_<hex>.json` `state` | 0x1000+ (file name = the id in hex, e.g. `state_2000.json`) |
  | projectile kind | — | `projectiles/<id>.json` | any name |
  | hook script | — | `hooks/**/*.rhai` (referenced by path) | any path outside `scripts/` |
  | event | 0..521 | `scripts/events/sNNN.rhai` | 600+ (the examples use 900..910) |
  | story flag | below 860 | `set_flag(f)` | 900..1143 |
  | area | worlds 0..21 | `world/areas/W-A.json` | world 90+ |
* Units: frames at 59.73 Hz, pixels, sub-pixel speeds in 1/256 px per frame, damage multipliers in 1/1024,
  directions `down, up, left, right` (0..3). Details per category: [`CONTENT.md`](../CONTENT.md), `docs/tables.md`,
  `docs/world.md`, `docs/maps.md`.

## 4. Battle content

### Stats (patch the base records)

`enemies/<id>.json`: `hp`, `STR`, `POW`, `END`, `resist_melee`, `resist_energy`, `exp`, `ai_state` (the AI tree,
`ai/state_NNN.json`), `gfx_ids`. `characters/<id>.json` `new_game` (level, hp, ep, STR..SPD, form, skills) and
`growth.raw_u32` (level-up formula, CONTENT/tables docs). `forms/*.json` `measured_bonus` (STR / POW / END / walk /
dash), `transform_up` / `transform_down` (255 = none). `equipment/*.json` stats, `levels.json` XP table.

### Skills: `skills/<id>.json` → `engine`

Every skill may carry an `engine` object; each field is optional and falls back to the built-in value:

```json
"engine": {
  "base": 7,            // the built-in skill (1..23) whose state machine runs this skill (default: its own id)
  "check": [12],        // EP needed to start, per skill level (last entry repeats)
  "cost": [12],         // EP spent on activation (and per refire for the Energy Blast family)
  "damage": [640],      // damage multiplier in 1/1024, per level
  "attack_sub": 4,      // the attack sub-type (reaction / resistance class of ComputeDamage)
  "fire_frame": 6,      // Energy Blast family: frame the shot leaves (16)
  "recovery": 18,       //   frames until refire / the end (40)
  "speed": 1280,        //   shot speed, 1/256 px per frame (768)
  "hit_box": [12, 12],  //   shot hit box centred on it (16 x 16)
  "drain": [107],       // Kamehameha family: EP drain per frame while held (1/256 EP), per level
  "anim": ["kiblast"]   // start animation names tried in order (the fighter sprite's animation names)
}
```

The HUD icon, the `SkillStart { id }` event and observations use the skill's own id; the behaviour
(`base`) decides what happens. Give a skill to a fighter through its character's `new_game.skills` or the fighter's
`skills` (below). Story scripts: `learn_skill(char, id)`. A skill with `"script"` gets a behaviour of its own (§4b).

### Fighters and arenas: `battle/fighters/<id>.json`, `battle/arenas/<id>.json`

```json
{"id": "shadow_goku", "name": "Shadow Goku", "character": 9, "forms": [54], "default_form": 54, "skills": [7, 15, 14],
 "fusion": null}
{"id": "my_arena", "name": "…", "map": "8-71", "spawn": [96, 216], "bounds": [56, 72, 424, 360]}
```

The game's fighters / arenas are built in (`wf_core::battle::FIGHTERS` / `ARENAS`); a file with the same id
replaces one, new ids are added (native `--fighter` / `--arena`, web `?fighter=` / `?arena=`, the online lobby's
fighter list). An arena's `map` is a `W-A` key or a map folder id (`maps/<id>/collision.json`).

### Sprites, effects, portraits, icons

`sprites/<id>/sprite.json` + `sheet.png` (CONTENT.md §3): frames, animations with durations, body boxes, footprint,
shadow. A recolour = `$extends` the sprite + a `sheet.png.derive.json`. A new sheet you draw yourself = your own
`sheet.png` + `sprite.json` (frame rects, anchors, animations).

## 4b. Scripted content: skills, projectiles (modkit)

New *behaviours* are data plus small Rhai hooks. The core (`wf_core::modkit`) runs them; the frontend attaches the
scripts when it loads the packs (`wf_app::battle::load_data`, crate `wf-modscript`). Hook scripts live outside
`scripts/` (that folder is the story's), e.g. `hooks/skills/x.rhai`, and are referenced by their pack path.

### Skills with a script: `skills/<id>.json` → `engine.script`

```json
"engine": {
  "script": "hooks/skills/homing_volley.rhai",   // the behaviour (instead of `base`)
  "check": [16], "cost": [10],                   // EP to start / spent at the start (scaled like the game's costs:
                                                 //   SS / SS3 gauges in those forms, Dragon Armor halves)
  "cooldown": 30,                                // frames after the end before it can start again
  "damage": [700], "attack_sub": 4,              // for the skill's projectiles / strikes (else the projectile's)
  "drain": [64],                                 // optional: the skill ends when the energy runs out
  "anim": ["special_charge", "kiblast"], "anim_shift": 3   // start animation, frames per cel = 1 << shift
}
```

The skill runs in its own player state (pressing B starts it like any skill: `can_use_skill`, `SkillStart`,
`SkillEnd`, hurt / holds / KO interrupt it). Hooks, all optional, each `fn name(s)`:

| hook | when | result |
|---|---|---|
| `on_start(s)` | the press | — |
| `on_release(s)` | the first frame B is up | — |
| `on_frame(s)` | every frame of the skill (after `on_release`) | `1` / `true` ends the skill |
| `on_end(s)` | the skill ends for any reason | — |
| `on_hit(s)`, `on_end(s)` | of its projectiles (below) | — |

`s` (all integers): `t` frames since the start, `held` (1 while B is held), `released` (the frame of the release,
-1), `skill`, `skill_level`, `me`, `x` / `y` (footprint px), `cx` / `cy` (body centre), `facing` (0 down, 1 up, 2
left, 3 right), `hp`, `max_hp`, `ep`, `max_ep`, `level`, `slot`, `frame`, `enemies` (alive); the nearest enemy:
`target` (0 = none), `tx` / `ty`, `tcx` / `tcy`, `tdx` / `tdy` (from the body centre), `tdist`, `tfacing`, `thp`,
`tmax_hp`; `vars` (8 integers kept per actor) and `args`.

Commands (queued, executed in order after the hook returns — `s` does not change during the call):

| command | effect |
|---|---|
| `fire(p)`, `fire_at(p, ox, oy)`, `fire_dir(p, vx, vy)`, `fire_from(p, ox, oy, vx, vy)` | a projectile of kind `p` from the hand (enemies: the body centre) + (ox, oy), flying along (vx, vy) (any length) or the facing |
| `anim(name)`, `anim_loop(name)` | an animation of the actor's sprite, played once (holding the last cel) / looping |
| `sfx(id)`, `sfx_stop(id)` | sounds (game SFX ids) |
| `spend(ep)`, `drain(per256)` | EP (scaled like skill costs) |
| `strike(w, h, mult)` | a hit box w x h in front of the body, damage multiplier in 1/1024 (melee sub-type unless `attack_sub`) |
| `teleport(x, y)` | move to (x, y) px, else the nearest free spot within 24 px; nothing if none |
| `face(dir)`, `face_target()` | turn |
| `hittable(bool)`, `flash(frames)`, `heal(hp)`, `shake(frames, intensity)`, `explode(dx, dy)` | the actor / screen |
| `set_var(k, v)` | the actor's variable k (0..7), read back as `s.vars[k]` from the next call |
| `end()` | end the skill (AI: the action is done) |
| `summon(type, dx, dy)`, `goto_state(n)`, `push_state(n)`, `pop_state()` | enemies (AI hooks) |

Helpers: `isqrt(n)`, `clamp(v, lo, hi)`, `abs`, `min` / `max`, `dir_x(d)` / `dir_y(d)` (the unit vector of a
facing). Constants are functions (`fn speed() { 640 }`): a hook sees nothing but its argument.

### Projectile / effect kinds: `projectiles/<id>.json`

```json
{
  "sheet": "effects/mod_orb",            // any sheet: effects/<id> or sprites/<id> (a derived recolour works)
  "frames": [[0, 1], [4, 5], [8], [9, 10]], "rate": 4,   // cels per direction (or one list), frames per cel
  "anim": null,                          //   ... or a named (looping) animation of the sheet
  "end_frames": [12], "end_time": 8,     // the spark left behind
  "speed": 640, "accel": 24, "max_speed": 1280,          // 1/256 px per frame
  "homing": 28, "homing_range": 180, "homing_delay": 6,  // turn per frame (1/256 of the direction), search px
  "life": 110,                           // frames (0 = until off camera; "offscreen": true keeps it)
  "hit_box": [12, 12], "pierce": 0,      // targets it passes through before it ends
  "wall": "explode",                     // explode | vanish | pass | bounce
  "damage": 700, "attack_sub": 4,        // 1/1024; the reaction / resistance class
  "explode": {"radius": 12, "damage": 300, "fx": true, "on_expire": true},
  "sfx_spawn": 4, "sfx_hit": 48, "sfx_end": -1,
  "script": "hooks/x.rhai"               // optional: on_hit(s) / on_end(s) / on_frame(s) (else the skill's)
}
```

Fired by a player it hits enemies, by an enemy the players. Projectile hooks get `shot`, `x`, `y`, `t`, `owner`,
`other` (the entity hit), `hits`, `skill`, `frame`, and the owner's `vars`; their commands act for the owner.

## 4c. New enemy types and scripted AI

A new type = a new `enemies/*.json` with a free `type` (150+), usually `$extends` of a base enemy:

```json
{"$extends": "enemies/majin_fighter.json", "type": 150, "display_name": "Shadow Brute", "hp": 900,
 "ai_state": 8192, "anim_as": 68, "gfx_ids": [910], "spawn_points": [], "boss": null}
```

`anim_as` = the base type whose recorded animation rules it uses; `gfx_ids` = sprite + 9 (a `$extends` sprite with a
derived sheet). Battles: `--enemy t150`; areas: `{"type": "enemy", "enemy_type": 150, …}`.

AI trees are `ai/state_<hex>.json` (`state` = the id): the game's node kinds by name with their `params` as the base
pack writes them (`{"r2": {"raw": 2500}}` = 50 px squared, `{"p": {"raw": 7864}}` = 12 % per frame, `speed` in
1/256 px, `frames`, `state`), `children`, and `aux` = the on-hit tree (`{"name": "SubState", "params": {"state":
{"raw": 12}}}` = the game's standard hit reaction). Two node kinds call Rhai:

```json
{"name": "ModCondition", "call": "hooks/ai/brute.rhai:phase_two_due", "children": [ … ]}   // fn -> true / false
{"name": "ModAction",    "call": "hooks/ai/brute.rhai:volley", "args": [3, 140]}           // fn -> 0 running, 1 done
```

They behave like the game's conditions (the child runs while true) and actions (`s.f` = frames since the node was
entered). AI hooks see the enemy (`hp`, `max_hp`, `x`, `y`, `facing`, `type`, `counter`, `children`, `vars`) and its
target player (`target`, `tx`, …, `tdist`, `thp`) and may use every command above, `goto_state` / `push_state` /
`pop_state` to change the AI state. `modcheck` reports unknown node names and calls to functions that do not exist.

**Determinism**: hooks are pure functions of `s`; every effect is a command the core executes; state that must last
is `set_var` / the AI state / the projectiles — all plain data in the `World` (cloned, hashed, byte-snapshotted), so
rollback, replays and online play work unchanged. Integers only (32-bit, wrapping), no clock, no randomness of their
own; an operation budget (200 000 per call) stops runaway scripts identically everywhere. A script error is reported
as an `Info` event and the call does nothing. native == wasm: §7.

## 5. Field content (story mode)

### Areas: `world/areas/<W-A>.json`

```json
{
  "key": "90-2",                       // world-area (defaults to the file name before '+')
  "name": "Hidden Grove",
  "maps": ["mod_hidden_grove"],         // per variant; a base map id ("area_08_071") or your own map folder
  "song": 28, "combat": false,          // combat false: A talks / interacts and never punches
  "on_enter": "my_enter_script",        // scripts/inline/<id>.rhai, run after each load (on_leave too)
  "banner": "Hidden Grove",             // the area-name banner (default for a new area: its name; false = none)
  "objects": [ … ],                     // replaces the area's object list (a new area's objects)
  "add_objects": [ … ]                  // appended to it (add to a base area without replacing its objects)
}
```

For an existing area only the fields you give change. The banner shows on entry like the game's (239 frames, not on
the world map); areas with the same banner text count as one place (no banner between them), like the game's shared
name ids. Objects (`docs/world.md` §1, the same JSON as the base
pack's `maps/<map>/objects.json`; `list` / `rom` may be left out — a new object goes at the end of its type's list):

| `type` | fields |
|---|---|
| `npc` | `pos [x, y]`, `sprite_index` (script sprite id = sprite + 9), `talk {"event": "sNNN"}`, `behaviour []` (stand) |
| `enemy` | `enemy_type`, `pos`, `on_defeat {"event": "sNNN"}` or `{"script": "id"}`, `behaviour []` |
| `exit` / `edge_exit` | `rect {x, y, w, h}`, `activation {"mode": "touch"}`, `to {world, area, variant, x, y, facing}`, `push` (the key to hold) |
| `script_trigger` / `event_trigger` | `rect`, `activation` (`touch`, `touch_once`, `button` [+ `facing`]), `script` / `event` |
| `item` | `item` (supply index; negative = equipment), `pos`, `on_pickup` |
| `breakable` | `prop`, `param_a` (HP), `hits` (attack mask), `on_break` |

`condition`: `{"always": true}` (default), `{"flag_set": f}`, `{"flag_clear": f}`, `{"flag_set_consume": f}`,
`{"value": 1}` = never placed by the load (only `spawn_map_object(i)` creates it — `i` = its index in the actor
list), `{"script": "id"}`.

### Story party characters (`characters/*.json` with an index 9+)

A character record with a free `index` is a story character as well as a battle one: the persistent records
(`World.story.chars`) grow to hold it; `join_party(i)` / `leave_party(i)` / `in_party(i)` / `set_character(i)`
(scripts), the character menu, growth on level-up, equipment and skills work as for the game's nine.

```json
{"index": 10, "name": "Tien",
 "new_game": {"level": 60, "hp": 150, "ep": 70, "STR": 30, "POW": 22, "END": 28, "SPD": 46, "form": 56,
              "skills": [8, 7], "xp": 2200, "equip": [0, 44, 77, 112], "in_party": false},
 "growth": {"raw_u32": [393216, 458752, 640, 1024, 192, 131072, 96, 1024]}, "forms": [56]}
```

`equip` (body, hands, feet, accessory; default the game's `[0, 44, 77, 112]`) and `in_party` (in the party at a
new game) are optional. Its forms (`forms/*.json` with `char_index` = the index) give the sprite (`gfx_id`), bonuses
and transformations (`transform_up` / `_down`). The example joins through an NPC's talk event and switches with a
choice (`mods-examples/new-party-member`).

### Maps: `maps/<id>/`

* `map.json` — Tiled JSON (orthogonal, 8 x 8 tiles, layers `bg0`..`bg3` with properties `bg` and `priority`,
  base64/zlib or plain array data). Reference a base tileset: `"tilesets": [{"firstgid": 1, "source":
  "../tilesets/<hash>.json"}]` (any `maps/<base map>/map.json` names the one it uses). Map properties
  `world_w_px`, `world_h_px`, `camera_max_x`, `camera_max_y`.
* `collision.json` — `{"rows": ["####", "#..#", …]}`: one string per tile row, `.` open, `#` wall, `~` blocks
  walking but not shots (or the extractor's run-length form).
* `objects.json` — the map header: `start_pos`, `map_triggers` (exits that belong to the map in every area using
  it), `props`, `areas: []`. Required: a map without it is not known to the field game.

The example generates its map from ASCII art (`mods-examples/new-area/tools/make_grove.py`); Tiled edits the same
file.

## 6. Scripts (Rhai)

Story scripts are Rhai files: `scripts/events/sNNN.rhai` (an event graph: dialogue tree + code nodes, run by NPC
talk, event triggers, on-defeat callbacks, `run_event(N)`) and `scripts/inline/<id>.rhai` (`fn main()`, run by
on-enter / on-leave, script triggers, `{"script": id}` callbacks). Full API: [`docs/scripting.md`](scripting.md)
§4 (all 248 natives of the game + the multiplayer extensions); semantics: [`docs/script-host.md`](script-host.md).

```rhai
// scripts/events/s910.rhai — box = 8 + portrait index (portraits/index.json), 0 = no portrait
fn event() {
    [
        #{ node: 0, code: "n0_", entry: "main", next: 1 },
        #{ node: 1, say: "Take this. A traveller should never go without one.", box: 28, next: 2 },
        #{ node: 2, choice: "Rest here?\n-Yes\n-No", box: 28, next: 3 },
        #{ node: 3, code: "n3_", entry: "main", next: 4 },
        #{ node: 4, end: 0 },
    ]
}
fn n0_main() { if get_flag(1101) != 0 { return 2; } }        // an int return picks the next node
fn n3_main() {
    if get_menu_result() == 0 { heal_party(); }              // the choice made in node 2 (0 = first option)
    give_item(21);                                           // item-get pose, then the inventory
    set_flag(1101);
}
```

Rules (they make scripts rollback-safe and identical on every machine):

* A handler **runs to completion** in one frame. Calls with an effect (`say`, `walk_to`, `warp_fade`,
  `give_item`, `spawn_map_object`, …) are queued and run in order, possibly over many frames; calls with a result
  (`get_flag`, `actor_x`, `rand`, `get_menu_result`) see the world after everything queued before them. If you need
  a result after an effect, split: `return next(1);` continues in `step_1(result)`.
* Only integers; randomness only through `rand(n)` / `chance(pct)` (the game's RNG); no clock, no I/O. Keep state
  in flags (`set_flag`), the four globals `g(i)` / `set_g`, or thread locals `local(i)` — not in Rhai variables
  that must survive a frame.
* Common calls: dialogue = event `say` / `choice` nodes; `warp(w, a, variant, x, y)` / `warp_fade(…)`;
  `spawn_map_object(i)`; `give_item(i)`, `add_item(i, n)`, `take_item(i)`, `give_equipment(i)`; `give_xp(n)`;
  `learn_skill(c, s)`; `heal_party()`; `play_music(n)`, `play_sfx(n)`; `walk_to(actor, x, y)`,
  `face(actor, dir)`, `begin_parallel()` … `end_parallel()`; `fade_out(n)` / `fade_in(n)`; `wait(frames)`.
  Actors: 0 = the leader, ≥ 10 = a sprite id on screen, −(n+1) = map placement n.
* Multiplayer: `player_count()`, `player(n)`, `who()` (the player that triggered the handler), `give_item_to(n,
  i)`, `say_to(n, portrait, text)` — never "which player am I" (§8).

## 6b. HUD layout: `ui/layout.json`

Where the frontend puts the HUD and banner, and which skill icons it shows. The base pack has no such file (every
default is the game's); a mod ships one, every field optional:

```json
{
  "hud": {"pos": [152, 136], "anchor": "bottom_right", "priority": 1, "hidden": false},
  "skill_icons": {"dir": "icons/skills_night", "pos": [0, 0], "files": {"7": "icons/hud_mod/energy_blast.png"}},
  "banner": {"y": 8}
}
```

* `hud.pos` is the 80 x 16 HUD's top-left in the 240 x 160 screen (the game's: `[8, 8]`); in the zoomed-out PC view
  it keeps its distance to the `anchor` corner (`top_left` like the game, `top_right`, `bottom_left`,
  `bottom_right`). `priority` = the GBA OBJ priority against the map's BG layers.
* `skill_icons`: the set's folder (`<dir>/skill_NN.png` + `_alt`; a skill missing there falls back to the game's
  icon), single files per skill id (the `_alt` icon = the same name with `_alt`), the icon's place inside the HUD
  picture.
* `banner.y`: the area banner's y (the game's 128, bottom-anchored; a value in the upper half anchors to the top).

## 7. Testing a mod

* **Check it**: `cargo run -p wf-app --example modcheck -- <content/base> path/to/my-mod` lists what the mod adds,
  replaces, patches and derives, and checks every reference the engine will follow: fighters → characters / forms →
  sprites + sheets, skills → behaviour + HUD icons, areas → maps (Tiled, tileset, collision) → talk / trigger /
  defeat scripts, and that every Rhai file compiles; scripted content: skill hooks, projectile sheets / cels, AI node
  names and `ModCondition` / `ModAction` functions, new enemy types' AI state and sprite, new characters' forms.
  Exit code 1 on errors.
* **Look at it headless** (no window, no sound): `buus-fury --headless --mod my-mod --story 90-2 --frames 120
  --input U30,A2 --size 480x320 --shot out.ppm` (battle: `--fighter`, `--arena`, `--enemy`).
* **Automate it**: copy a test from `crates/wf-app/tests/mods.rs` — load packs with `wf_app::mods::load(base,
  &[mod_dir])`, then drive `wf_app::battle::session(&packs, cfg)` (battle: inputs per frame, `Event::Damage`,
  `SkillStart`, observations) or `wf_app::story::StorySim::new(&packs, (w, a), …)` (story: step with buttons, read
  `world().story.flag(f)`, `field.area`, entities, `errors` for script errors), and render a frame with
  `wf_render::Renderer` to catch missing art. Runs headless and muted; needs only `roms/buu.gba`.

* **native == wasm**: `cargo run --release -p wf-modcheck --example digest -- <content/base> <mods…> > native.txt`,
  then `cargo build -p wf-modcheck --target wasm32-unknown-unknown --profile release-wasm` and `node
  engine/web/mods_check.mjs engine/target/wasm32-unknown-unknown/release-wasm/wf_modcheck.wasm <content/base> <mods…>
  --expect native.txt`: a two-player battle against the mods' enemy types with the mods' skills and a story session,
  1200 frames each, every frame's state hash folded into a digest. The example mods (scripted-skills + new-boss +
  new-party-member + new-area): identical digests (the values change with the engine; compare the two runs).

Mods never touch the unmodded game: with no mods loaded every parity suite (emulator lockstep, the TS
cross-check, field / cutscene lockstep, netcode sync tests, replays) runs exactly as before — the mod runtime state
hashes as nothing while empty, and every new code path is reached only through mod content.

## 8. Multiplayer, replays, limits

* **Every peer must load the same mods.** Online play simulates on each machine (rollback), so different content
  would desync. The lobby compares a content fingerprint — the hash of the ROM, the extractor and every mod's id and
  file bytes (`wf_app::battle::content_id`; headless / web lobbies also hash the loaded game data:
  `wf_net::netsim::content_id`) — and refuses peers that differ ("different game content (ROM / mods)").
  Share the mod folders before playing.
* Replays and save states record the mod ids and that hash; playing them with other mods is refused.
* Mod content changes the content fingerprints too: projectile kinds and the hook scripts' sources are part of
  `wf_net::netsim::content_id`.
* Dialogue portraits are drawn like the game's (`docs/ui.md` §3; a new one = `portraits/portrait_NNN_<name>.png`,
  node `box` = 8 + NNN, see CONTENT.md §3); speaker names are part of the text (the game draws none).
* Limits (engine code, not data): hooks can only *ask* for the commands of §4b (no direct world access, no new
  hit-reaction kinds, no beams drawn as segment chains — a projectile is one sprite with a box); the AI's 195 node
  kinds + the two scripted ones (a scripted node cannot wrap children other than as a condition); the pause menu /
  shop / save screens are the frontend's and not data; a new enemy type's special behaviours keyed by the game's
  type ids (Kid Buu splits, Janemba parts, ...) are not inherited by `$extends`; more than 64 story characters;
  index files (`*/index.json`) are not regenerated (the engine discovers content by listing, so mods do not need
  them).
