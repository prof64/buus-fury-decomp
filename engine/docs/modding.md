# Modding guide

Everything the Buu's Fury engine plays comes from plain files: the **base pack** the extractor writes from your
own ROM (`content/base/`, never edited) and any number of **mods** layered on top. A mod is a folder of JSON, PNG,
WAV, Tiled maps and [Rhai](https://rhai.rs) scripts with the same layout as the base pack, holding only what it
adds or changes. No code, no ROM bytes, no rebuild.

Five complete examples live in [`engine/mods-examples/`](../mods-examples) — each has a README and an automated
test (`crates/wf-app/tests/mods.rs`):

| mod | shows |
|---|---|
| [`balance-tweak`](../mods-examples/balance-tweak) | merge patches: enemy stats, a skill's EP cost and damage |
| [`new-skill`](../mods-examples/new-skill) | a new skill id on an existing behaviour, own cost / damage / frame data, derived HUD icon, given to Goku |
| [`new-character`](../mods-examples/new-character) | a new fighter: `$extends` sprite + palette-remapped sheet, own form, stats, growth, skills, roster entry |
| [`new-arena-scenario`](../mods-examples/new-arena-scenario) | a Rhai story scenario: NPC with a choice, warp to a new arena area, three bosses chained by on-defeat events, dialogue with portraits, a prize and a flag |
| [`new-area`](../mods-examples/new-area) | a new field area: Tiled map on a base tileset, row collision, map exits both ways, an NPC with a Rhai talk script |

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
  | skill | 1..23 | `skills/*.json` `skill` | 24+ (HUD icon `icons/skills/skill_NN.png` + `_alt`) |
  | sprite | 0..296 | `sprites/<id>/sprite.json` `sprite_index` | 300+ (forms use `gfx_id` = sprite + 9; NPCs use script sprite id = sprite + 9) |
  | form | 0..53 | `forms/*.json` `index` | 54+ |
  | character | 0..8 | `characters/*.json` `index` | 9+ (battle fighters only; the story party is the game's nine) |
  | enemy type | 0..149 | `enemies/*.json` `type` | patch existing ones (types index a dense table) |
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
`skills` (below). Story scripts: `learn_skill(char, id)`.

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

## 5. Field content (story mode)

### Areas: `world/areas/<W-A>.json`

```json
{
  "key": "90-2",                       // world-area (defaults to the file name before '+')
  "name": "Hidden Grove",
  "maps": ["mod_hidden_grove"],         // per variant; a base map id ("area_08_071") or your own map folder
  "song": 28, "combat": false,          // combat false: A talks / interacts and never punches
  "on_enter": "my_enter_script",        // scripts/inline/<id>.rhai, run after each load (on_leave too)
  "objects": [ … ],                     // replaces the area's object list (a new area's objects)
  "add_objects": [ … ]                  // appended to it (add to a base area without replacing its objects)
}
```

For an existing area only the fields you give change. Objects (`docs/world.md` §1, the same JSON as the base
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

## 7. Testing a mod

* **Check it**: `cargo run -p wf-app --example modcheck -- <content/base> path/to/my-mod` lists what the mod adds,
  replaces, patches and derives, and checks every reference the engine will follow: fighters → characters / forms →
  sprites + sheets, skills → behaviour + HUD icons, areas → maps (Tiled, tileset, collision) → talk / trigger /
  defeat scripts, and that every Rhai file compiles. Exit code 1 on errors.
* **Look at it headless** (no window, no sound): `buus-fury --headless --mod my-mod --story 90-2 --frames 120
  --input U30,A2 --size 480x320 --shot out.ppm` (battle: `--fighter`, `--arena`, `--enemy`).
* **Automate it**: copy a test from `crates/wf-app/tests/mods.rs` — load packs with `wf_app::mods::load(base,
  &[mod_dir])`, then drive `wf_app::battle::session(&packs, cfg)` (battle: inputs per frame, `Event::Damage`,
  `SkillStart`, observations) or `wf_app::story::StorySim::new(&packs, (w, a), …)` (story: step with buttons, read
  `world().story.flag(f)`, `field.area`, entities, `errors` for script errors), and render a frame with
  `wf_render::Renderer` to catch missing art. Runs headless and muted; needs only `roms/buu.gba`.

Mods never touch the unmodded game: with no mods loaded every parity suite (emulator lockstep, the TS
cross-check, field / cutscene lockstep, netcode sync tests) runs exactly as before.

## 8. Multiplayer, replays, limits

* **Every peer must load the same mods.** Online play simulates on each machine (rollback), so different content
  would desync. The lobby compares a content fingerprint — the hash of the ROM, the extractor and every mod's id and
  file bytes (`wf_app::battle::content_id`; headless / web lobbies also hash the loaded game data:
  `wf_net::netsim::content_id`) — and refuses peers that differ ("different game content (ROM / mods)").
  Share the mod folders before playing.
* Replays and save states record the mod ids and that hash; playing them with other mods is refused.
* Not moddable yet (engine code, not data): new skill *behaviours* (a skill reuses one of the 23 state machines;
  skills cannot run Rhai — the battle core has no script host), new enemy AI node kinds (AI trees are data in
  `ai/state_*.json`, but only the game's 195 node kinds exist), new projectile / effect kinds, menu and HUD layouts,
  new characters in the story party (nine story slots), enemy types beyond the dense table (patch existing ones),
  the area-name banner for new areas (it uses the game's string ids), and portrait / speaker-name rendering in
  dialogue boxes (the frontend draws the text box only). Index files (`*/index.json`) are not regenerated; the
  engine discovers content by listing, so mods do not need them.
