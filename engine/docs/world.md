# World: map objects, area graph, dialogue text (crate `wf-world`)

Everything an area places on its map, how areas connect, and every line of dialogue — decoded from the ROM, no
emulator. ~70 ms natively; builds for wasm. Research scripts: `re/buu/world/` (`verify_objects.py` is the emulator
check). Event / script ids match the script-VM catalog (`engine/annotations/script_catalog.json`, crate
`wf-script`): `sNNN` = event table `0x0879B704` index, `inline_0xADDR` = raw bytecode at ADDR.

## 1. Where the data comes from

### Area record lists (`0x0808E2E0 + 0x38·i`, see [maps.md](maps.md))

The loader (`0x08008B30` → `0x08008AA0`, `0x08008AFE`, `0x08008C9E`) creates these on **every** entry into the
area, each only if its condition holds then (`0x08004784`): enemies therefore respawn whenever the area is
re-entered, unless their condition is a story flag.

| list | count | entry | contents |
|---|---|---|---|
| +0x10 | +3 | 8 B `{u32 condition, ptr def}` | triggers (below) |
| +0x14 | +4 | ptr def (def+4 = condition) | actors: enemies, NPCs |
| +0x18 | +5 | ptr def (def+0 = condition) | breakable props |
| +0x1C | +6 | ptr def (def+0 = condition) | item pickups |
| +0x20 | +7 | 8 B inline | bobbing map sprites |
| +0x28 / +0x30 / +0x34 | | | map-variant selector, on-enter / on-leave bytecode |

**Map header** (per map, unconditional, created by `0x08009670` → `0x08012664`): +0x40 count / +0x44 → def pointers
= the map's own exits (door and screen-edge exits); +0x24 count / +0x28 → 0x18-byte **props** (OBJ sprites over the
map: rect u32×4, OAM attr0|attr1, tile word; scripts move / show / hide them, breakables refer to them by index);
+0x30 = s16 x, y (default position, INFERRED).

**Condition word**: 0 always; 1..0xFFFF never (object only created by scripts: `spawn_map_object`); `0x1xxxx`
flag set; `0x2xxxx` flag clear; `0x3xxxx` flag set → clear it and create; ROM pointer = bytecode returning bool.
Flags = bit `f&7` of `GameData+0x55+(f>>3)` (`0x03001068`).

**Callback pair** `{fn, data}`: `0x0800421D` none, `0x0800421F` run bytecode `data`, `0x0800424F` run event `data`.

### Triggers (rect objects, base `0x08009AEC`: def +4 activation getter, +8 / +0xC s16 corners x0,y0 / x1,y1)

| ctor | vtable | type | payload |
|---|---|---|---|
| `0x08007387` | `0x08049784` | `script_trigger` | +0x10 bytecode, run on activation |
| `0x0800742D` | `0x08049BD4` | `event_trigger` | +0x10 event header (blocking dialogue / cutscene) |
| `0x08007675` | `0x0804A09C` | `exit` | +0x10 low nibble = direction to push (input mask table `0x080707D8`), high nibble = facing+1 on arrival; +0x11 world, +0x12 area, +0x13 map variant (0 = area selector), +0x14 / +0x16 destination x, y |
| `0x080072CD` | `0x08049BC4` | `edge_exit` | same fields, +0x10 = push direction = arrival facing; position is **relative**: new = player − (x0, y0) + (x, y) (`0x080071F8`) |
| `0x08007499` | `0x08049794` | `save_point` | fixed event s011 ("-Save / -Switch Characters") |
| `0x080076C5` | `0x0804B070` | `message_trigger` | broadcasts world message 0x17 with +0x10 (cutscene map 0-50 only) |
| `0x08024A55` | `0x080499D4` | `region` | enter/stay/leave state machine, no-op activate; purpose not decoded |

**Activation** (the +4 getter returns a test object): `0x080077DF` touch_once (fires on overlap, then removed until
the area reloads), `0x0800776D` touch, `0x0800770D` A button, `0x0800774F/53/5F/59` A while facing up / down / left /
right, `0x08034355` / `0x0803436F` touch at altitude z ∈ (0,16) / (44,76) (flying).

### Actors (list +0x14)

| ctor | type | def layout |
|---|---|---|
| `0x0800FD55` | `enemy` | +8 enemy type (`enemies/`), +0xC s16 x, y, +0x10 sprite index, +0x14 → on-defeat callback pair, +0x18 behaviour |
| `0x080111A9` | `npc` | +8 x, y, +0xC sprite index, +0x10 behaviour |
| `0x08017DA3` | `npc` with `talk` | +8 x, y, +0xC talk event (A facing the NPC), +0x10 sprite index, +0x14 behaviour |

**Behaviour** = `u32 n` + n step pointers, looped (`0x0800A95A`); step = `{create fn, params}`: `0x0800A26F` stand,
`0x0800A223` wait (s16 frames, u8 facing), `0x0800A1AF` / `0x0800A1D5` walk_to (x, y, param), `0x0800A75B` /
`A7DF` / `A82D` / `A8E9` wander (period), `0x0800A7A1` / `A811` / `A85F` wander_rect (period, u16 rect). Names are
INFERRED from the parameters; every step keeps its raw words.

### Other lists

* **breakable** (+0x18, 0x10 B): +4 prop index, +5 u8 (not decoded), +6 u16 hits (INFERRED), +8 callback pair
  (usually loot script s497 with the loot table args). The prop shakes when hit (`0x0800C858`).
* **item** (+0x1C): +4 item (≥0 supply / story item index of `items/`, <0 equipment), +8 / +0xC u32 x, y,
  +0x10 callback pair (the "You found …" event).
* **map_sprite** (+0x20, 8 B): s16 x, y, u8 sprite resource, 3 bytes not decoded; drawn bobbing on the sine table
  `0x087B6EEC` (`0x0800C378`).

### What scripts do (`warps`, `shop`, `gives_items`)

Triggers, talk events and on-enter scripts are scanned (symbolic stack over the bytecode; native arities from
`script_natives.json`, snapshot `crates/wf-world/src/natives_gen.rs` via `engine/tools/make_world_natives.py`),
following `run_event` 0xB7 / `run_script` 0xD8 / goto nodes up to depth 4:
`warp` 0x77 / `warp_fade` 0x76 `(world, area, variant, x, y)`; `shop_add_item` 0xCD `(category, item, price, flag)`
(prices are usually `base * get_g0() / 100`: `base_price`), `open_shop_buy` 0xCE, `open_shop_sell` 0xCF (percent);
`give_item` 0x80. Arguments that are not constants stay as expressions. Note: the warp natives' third argument is the
map **variant** (it reaches `0x080089A0`), not a facing.

### Text

* Text banks: `u32[7]` at `0x087B5B64` → webfoot-LZ UTF-16LE, ~64 KB each, lines concatenated without terminators.
  Ref (event node kinds 5, 6) = `bank | offset << 6 | length << 21` (UTF-16 units) — loader `0x08012230`.
* Event nodes `{u16 next, u8 kind, u8 arg}` (handler table `0x080728AC`): 0 bytecode, 1 inline text, 2 printf text
  (format ptr +4, args bytecode +8), 3 choice (inline, `-` options), 4 compressed bytecode, 5 bank text, 6 bank text +
  printf args, 7 goto. For text nodes `arg` = speaker (`0x08012012`): 0 / 7 none, 1..6 the current player's portrait
  slot (form table `0x086B82EC` +0xF), ≥ 8 portrait `arg − 8` (`0x086F395C`).
* Control codes: `\n`, `0x0B` new box, `0x08 n` colour (7 highlight, 2 normal), `0xA0` nbsp, `%s`/`%d`; a first
  character `!` / `@` / `#` puts the box at y 40 / 80 / 120 and is not drawn. Fonts / glyphs: [ui.md](ui.md).

## 2. Output

* `maps/<map>/objects.json` — `start_pos`, `map_triggers` (exits of the map header), `props`, and `areas[]` (every
  area using the map: `key`, `name`, `variant`, `objects[]`). Each object: `type`, `list`, `condition`, type fields
  (above), `rom` (def / ctor addresses).
* `maps/<map>/map.json` — gains a Tiled `objectgroup` "objects <area>" (props, map triggers, objects of the first area
  using the map; rect objects or points; full record as the `data` property).
* `world/areas.json` — per area: key, name, maps, variant_selector, on_enter / on_leave (+ their warps), object count;
  `counts` per type.
* `world/graph.json` — `nodes` (areas) and `edges` (`map_exit`, `map_edge_exit`, `exit`, `*_warp` from triggers /
  NPC talk / on-enter scripts) with rect, push direction, destination, variant, condition.
* `text/dialog.json` — `lines[]`: `id` (`<event>.<node>`), event, node, next, kind, speaker (portrait id + name from
  `portraits/index.json`), box, `text` (control codes as `{page}` `{color n}` `{nbsp}` `{xxxx}`, `{{` = `{`;
  `wf_world::text::encode` inverts it exactly), `plain`, `ref` / `rom`; `unreferenced` bank ranges (none).
* `text/banks.json` — the 7 banks verbatim (same rendering). The UI string table stays in `text/strings.json`.

Counts: 1559 enemies, 339 NPCs (191 with a talk event), 271 script / 166 event triggers, 99 area exits + 570 map exits + 170
edge exits, 43 save points, 125 breakables, 13 items, 28 map sprites, 16 message triggers, 3 regions; 966 graph
edges reaching 423 of 452 areas; 2783 text lines (all 196 386 bank units referenced); 8 shops.

## 3. Verification

* `cargo test -p wf-world`: text-bank coverage, render/encode round trip, lines seen on screen / in EWRAM, the 8-3
  exits and the 1-1 text trigger as read from RAM, NPC placements, the Z-Mart inventory, graph consistency.
* `re/buu/world/verify_objects.py <content base>` (headless emulator): in 2 save states and 10 warped-to areas, every
  live trigger object (vtable, rect) equals an exported one, every always-present exported trigger is live, and every
  always-present standing NPC sits exactly on its exported position (wanderers inside their rect): **77 / 77**.
* The text-bank decoding was confirmed against the bank the game inflates into EWRAM (`t_dialog.py`).

## 4. Gaps

* Script-driven moves with computed arguments (16 warps), the world-map / instant-transmission travel and cutscene
  maps (29 areas have no edge: mostly world 0 cutscene / scene maps).
* `region` triggers, `message_trigger` values, map_sprite params, breakable +5, wander step semantics, prop graphics
  (props are exported as data, not rendered).
* Talk events whose speaker is "the current player" depend on who is playing (slot 1..6 of the form's set).
