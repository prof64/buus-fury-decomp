# Content packs — the moddable data format of the Buu's Fury engine

The engine never ships game data. The player supplies their own ROM of *Dragon Ball Z: Buu's Fury* (USA,
SHA-1 `e65738e9d67688309f09811a54f495523ec9aada`); the extractor turns it into plain files — the **base pack** —
and mods are more packs layered on top. No emulator is involved at any point.

```
wf-extract <buus_fury.gba> <content dir> [--only tables,sprites,effects,portraits,items,maps,audio,ui,world,scripts]
```

* Native: `cargo run --release -p wf-cli -- roms/buu.gba content/` writes `content/base/`.
* Browser: `crates/wf-wasm` (plain `extern "C"` exports, no wasm-bindgen). `engine/web/extract.js` loads it,
  `ex.extract(romBytes)` returns `Map<path, Uint8Array>`; `engine/web/index.html` is a drag-and-drop demo.
  Build: `cargo build --target wasm32-unknown-unknown -p wf-wasm --profile release-wasm`, copy
  `target/wasm32-unknown-unknown/release-wasm/wf_wasm.wasm` next to `index.html`.
* Library: `wf_extract::extract(rom_bytes, &Options) -> (Content, timings)`; `Content.files` is a
  `BTreeMap<path, bytes>`.

Crates: `wf-rom` (ROM verification, bus-address reads, Webfoot LZ, string table, GBA graphics + PNG, the
`Content` sink), `wf-tables` (mechanics), `wf-maps` (maps, tilesets, collision), `wf-audio` (sound driver +
synthesizer), `wf-ui` (fonts, dialogue boxes, HUD, menus, title screens), `wf-world` (map objects, area
graph, dialogue text), `wf-extract` (sprites, effects, portraits, icons; orchestration), `wf-cli`, `wf-wasm`,
`wf-script` (story/event bytecode: loader, exact interpreter, decompiler to Rhai — `scripts/events/sNNN.rhai`,
`scripts/inline/inline_0xADDR.rhai`, `scripts/index.json`, `scripts/bytecode.json`; extractor category `scripts`) and `wf-script-rhai`
(the scripting runtime) — see [`docs/scripting.md`](docs/scripting.md).

## 1. Directory layout

```
content/
  base/                      ← written by the extractor (never edit; re-extract instead)
    pack.json                  pack metadata: id, format_version, source ROM SHA-1, files per category, warnings
    files.json                 every file path in the pack (for loaders that cannot list directories, e.g. HTTP)
    text/strings.json          the game's string table (index = string id)
    text/dialog.json, text/banks.json   every dialogue line / choice (id = <event>.<node>, speaker, box, control codes)
    characters/ forms/ skills/ attacks/ enemies/ ai/ bosses/ items/ equipment/   one JSON per id (+ index.json)
    levels.json formulas.json rng.json moves.json   (ai/state_XXX.json = decoded AI node trees)
    sprites/<id>/sheet.png + sprite.json, sprites/index.json
    effects/<id>/sheet.png + effect.json, effects/index.json
    portraits/<id>.png, portraits/index.json
    icons/supply/NNN.png, icons/equipment/NNN.png, icons/skills/skill_NN[_alt].png, icons/index.json
    areas.json, maps/index.json, maps/<id>/{map.json (Tiled), bgN.png, composite.png, collision.json},
    maps/tilesets/<hash>.{png,json}   (Tiled tilesets incl. tile animations, shared between maps)
    maps/<id>/objects.json     placed objects per area (enemies, NPCs, triggers, exits, save points, breakables,
                               items, shops) + map exits and props; map.json gains a Tiled object layer
    world/areas.json, world/graph.json   areas (on-enter scripts, variants) and the area graph (exits + script warps)
    ui/fonts/ images/ buttons/ hud/ mapscreen/ menu/ + *.json   fonts (atlas + metrics), boxes, HUD, menus, screens
    audio/index.json, driver.json, instruments.json, songs/song_NN.json, sfx/sfx_NN.json, samples/sample_NNN.wav
    scripts/events/sNNN.rhai, scripts/inline/inline_0xADDR.rhai   decompiled story/event scripts (Rhai; ids as in objects.json)
    scripts/index.json, scripts/bytecode.json   script index (purpose, referrers) and the original bytecode + event graphs
  mods/
    <mod name>/                ← same layout, only the files the mod changes or adds
      pack.json                  {"id", "name", "format_version": 1, "depends": ["base"], "load_after": [...]}
```

Details per category: tables → [`docs/tables.md`](docs/tables.md), maps → [`docs/maps.md`](docs/maps.md),
world (objects, graph, text) → [`docs/world.md`](docs/world.md), ui → [`docs/ui.md`](docs/ui.md),
audio → [`docs/audio.md`](docs/audio.md), scripts → [`docs/scripting.md`](docs/scripting.md); sprites / effects / portraits / icons → §3 below.

## 2. Ids, units, load order and overrides

* **Ids** are stable, lower-case slugs (`106_goku`, `fx_0871370c`, `area_08_003`, `song_44`). The file name *is*
  the id. Numeric game ids (sprite index, enemy type, item index, …) are kept as fields so the engine can map game
  logic to content. Cross-references between files use ids or paths relative to the pack root.
* **Units**: time in 60 Hz game frames (`*_frames`), positions and sizes in pixels, sub-pixel speeds in 1/256 px
  per frame (`*_subpx` or documented), audio rates in Hz. Directions are always `down, up, left, right` (the
  engine's direction index 0..3); `any` for direction-less animations.
* **Provenance**: most records carry a `rom` object with the bus addresses they were read from (`"0x086B6BD8"`).
  It is informational; mods may drop it. Values that do not come from the ROM (curated names, emulator
  measurements) say so (`name_confidence`, `timing_source`, `status` / `source` fields).

**Load order.** The engine loads `base`, then every enabled mod in a deterministic order: dependencies first
(`depends`, `load_after`), ties broken by mod id. A later pack wins.

**Overrides (by id).**
* A file at the same path replaces the earlier pack's file *entirely* (simple and predictable). JSON files may
  instead be named `<id>.patch.json`: an RFC 7396 JSON Merge Patch applied to the earlier version (objects merge
  recursively, `null` deletes a key, arrays are replaced whole) — the way to change one stat without copying the
  whole record.
* New ids are new files; `index.json` files are regenerated by the loader from the merged set (a mod does not
  need to edit them; if it ships one, its order wins for display).
* Removing content: an empty file named `<id>.delete`.
* Binary assets (PNG, WAV) are replaced whole. A sprite override may ship only `sheet.png` (same frame rects) or
  both `sheet.png` and `sprite.json`.

## 3. Sprites, effects, portraits, icons

### `sprites/<id>/sprite.json` (287 definitions → 297 entities in the ROM's sprite table `0x086B6BD8`)

| field | meaning |
|---|---|
| `id`, `sprite_index` | slug; engine sprite id (the index scripts and enemy records use) |
| `name`, `category`, `name_confidence`, `notes` | curated (annotation, not ROM): `player`/`enemy`/`boss`/`npc`/`effect`/`item` |
| `sheet` | `sheet.png` (RGBA, palette swaps already applied) |
| `frames[]` | `{x, y, w, h}` rectangle in the sheet, `anchor {x, y}` = the pixel that sits on the entity position, `rom` |
| `animations{name}` | `raw_index` (engine anim number), `loop`, `timing_source`, `observed_frame_orders`, `directions{dir: [step]}` |
| step | `{frame, duration_frames, flip, rom, box?, attach?}`; `flip` = draw the frame mirrored around its anchor; `box [ox, oy, w, h]` = the frame record's box (record +4..+7, relative to the integer position: picture-based hit / aim tests, 0x0800FE34); `attach [dx, dy]` = position of the attachment (halo) relative to the entity position |
| `body_box{dir}` | `[x0, y0, x1, y1]` px relative to the entity position |
| `footprint` | `[w, h]` px: the collision box tested against the walk grid at (x, y)..(x+w, y+h) |
| `shadow` | effect id of the ground shadow drawn at the entity position, behind the body |
| `attachment` | `{kind: "halo", effect: "halo"}` when steps carry `attach` |
| `header`, `rom` | raw definition bytes / addresses (definition, palette-remap table, OBJ palette) |

Animation names come from the curated annotation (`fighter` / `enemy` / `npc` animation sets); unknown ones are
`animNN`. **Timing**: the ROM has no animation durations — per-state code computes the frame. `timing_source`
tells where `duration_frames` came from: `decoded …` (from the game code, `engine/annotations/anim_timing_decoded.json`,
**TODO hook**: empty until the separate timing decode lands in `data/battle/buu_anim_timing.json`; entries there
override everything else), `measured (emulator, N segments)` / curated `measured …` (research annotations),
or `default` (8 frames).

### `effects/<id>/` — code-driven effect / projectile frames, shadows, halo

Same schema as sprites with one `default` animation over `any` direction (ROM order, 4 frames each).
`beam_<name>` = beam pieces (animation = piece: start, body8/16/32, head, impact, tail; step index =
VRAM slot variant, phase = t >> 2), `bbk_flare`, `debris`; `fx_<addr>` = a run of raw OBJ records found in the ROM (curated names from `annotations/effect_names.json` where known, else automatic: colour, OBJ size, frame
count; steps carry `obj` = record geometry [ox, oy, w, h, hflip, vflip]);
`shadow_4x2`, `shadow_10x4`, `shadow_16x6`, `shadow_24x10`; `halo`.
Field objects (`wf-extract/src/field_gfx.rs`; untrimmed frames in a fixed order, so the frame index is the id;
draw rules in `docs/world.md` §6): `coin_0`..`coin_3` (4 cels each), `field_items` (frame = supply index, 100 +
equipment index), `field_labels` (level-sign numbers), `props_<map id>` (frame k = map prop k).

### `portraits/<id>.png` — 64×64 dialogue portraits (155), `index.json` with `portrait_index`, name.

### `icons/` — `supply/NNN.png`, `equipment/NNN.png` (16×16; NNN = item index used by `items/`, `equipment/`),
`skills/skill_NN.png` / `skill_NN_alt.png` (32×16, NN = skill id), `index.json`.
Supply / equipment entries carry `sub_box` [x0, y0, x1, y1]: a dropped item's collision box inside its icon (opaque
pixel bounds, top raised to the middle — the game computes it from VRAM, `0x0800DB64`).

## 4. Annotations (our research, compiled into the extractor)

`engine/annotations/` (sprite / portrait / effect names, measured animation timing, the decoded-timing hook) and
`crates/wf-tables/data/` (AI node registry, boss facts, emulator measurements, docs) are snapshots of the
research files in `tools/` and `re/`; refresh with `engine/tools/make_*_annotation.py` and
`crates/wf-tables/re/dump_annotations.py`. They contain no ROM data.

## 5. What the extractor does not produce (yet)

* UI drawn entirely by code (most menu page contents, shop / save / game-over screens) — see docs/ui.md.
* Script-computed warps, world-map travel and the meaning of a few object fields — see docs/world.md.
* Scripts / events as runnable content (crate `wf-script`); map objects link to their ids.
* See the category docs for their own gaps (audio deviations, map quirks, annotation-only table fields).
