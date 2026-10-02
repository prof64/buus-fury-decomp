# Graphics formats — Legacy of Goku I / II / Buu's Fury

Everything here is regenerated from the three ROMs by

    .venv/bin/python tools/extract/extract_all.py [log1 log2 buu] [--skip-maps]

Outputs: `site/public/assets/<game>/…` (PNG, gitignored) and `data/<game>/sprites.json`.
`--skip-maps` keeps the (slower, emulator-driven) map renders from the previous full run.

All addresses are GBA bus addresses (ROM offset = address − 0x08000000).
LoG2 and Buu's Fury share one engine ("WF" below); LoG1 is an older, different engine.

---------------------------------------------------------------------------------------------

## 1. Compression

### 1.1 Webfoot LZ (LoG2, Buu) — `gba.wf_decompress`
Container: `u32 type, u32 decompressed_size, stream…`. type 0 = stored, 1/2 = compressed.
The decoder is ARM code copied to IWRAM (`0x03000000` in LoG2 from ROM `0x087FD6BC`; `0x03000040` in
Buu from `0x087B79E4`), reached through the dispatcher `0x0801F38C` (LoG2). It is a JCALG1-style
bit-stream decoder:

* Bits are read MSB-first from little-endian 32-bit words.
* State: `literal_bits` (starts = type word, i.e. 1 or 2 — every stream begins by resetting it),
  `literal_base` (0), `index_bits` (8), `last_offset` (1).
* Commands:
  * `1` + `literal_bits` bits → output `value + literal_base`.
  * `01` + gamma(hi): if hi == 2 → length = gamma, copy from `last_offset`; else
    offset = ((hi−3) << index_bits) + `index_bits` bits, length = gamma
    (+3 if offset ≥ 0x10000, +2 if ≥ 0x37FF, +1 if ≥ 0x27F, +4 if ≤ 0x7F); offset becomes `last_offset`.
    gamma = `v=1; do v = v*2 + bit; while (bit)`.
  * `000` + 7 bits: offset ≠ 0 → copy (2 bits)+2 bytes from offset (sets last_offset);
    offset == 0 → 2 bits n: n==0 end of stream, else `index_bits` = (n+3) bits.
  * `001` + 4 bits − 1: 0 → output 0; >0 → output byte at −value; −1 → next bit: 1 = blocks of
    256 raw bytes (repeat while next bit is 1); 0 = literal mode: `literal_bits` = 7 + bit, and if 7,
    `literal_base` = 8 bits.
Cross-checked against work/ref/log2 `decode_field_chunk`.

### 1.2 LoG1
Sprite tiles are **uncompressed**. Dialogue portraits use the GBA BIOS codecs chained
(Huffman 4-bit → RLE; `gba.huffman`, `gba.rle`, `gba.bios_decompress`).

## 2. Palettes and tiles

* WF games: one global 256-colour OBJ palette — LoG2 `0x081DA6C8` (BG `0x081DA4C8`), Buu `0x0805652C`.
  Index 0 transparent; only a couple of entries are animated at runtime (e.g. LoG2 index 168).
  Palette swaps (recoloured enemies) are 256-byte **pixel index remap tables** referenced by the sprite
  definition (`remap_table` in json) and applied to the 8bpp pixels before lookup.
  LoG2 player portraits 0–17 are drawn on the menu BG layer with the menu palette `0x081D4F50`.
* All WF sprite/effect tiles are **8bpp**, OBJ 1D mapping. Portraits and item icons are **linear
  (row-major) 8bpp bitmaps** (64×64, 16×16, 32×32), not tiles.
* LoG1: 4bpp tiles, one 16-colour palette per sprite definition (`+0x24`).

## 3. Sprite definitions and frames

### 3.1 LoG2
Definition pointer table `0x083B4E90`, 150 entries; scripts address them as sprite id = index + 7
(`0x08009364`). The scouter database (`data/log2/enemies.json`, `gfx_id`) uses the same ids.
Definition:

| off | type | meaning |
|---|---|---|
| +0x00 | u8 | flags (0x40 on one effect sprite) |
| +0x01 | u8 | 1/2/4/16 — variant/flags (not a direction count) |
| +0x02 | u8 | number of animations |
| +0x03 | u8 | size class (8/16/24/48) |
| +0x04 | u32 | 8/16/24 (shadow / collision height class) |
| +0x08 | u32 | remap table pointer or 0 |
| +0x0C | 4 × {s32 x0,y0,x1,y1} | body rectangle per direction, relative to entity position (read by `0x080079B8`) |
| +0x4C | nanims × 4 × u32 | per animation, per direction: pointer to a **frame list** (0 = unused) |

Directions everywhere: **0 down, 1 up, 2 left, 3 right** (verified with the d-pad; in all three games the
index-2 frames are the ones shown while walking left — sprites.json labels follow this order, `export.DIRS`).
Frame list = consecutive 12-byte frame records; the count is not stored (state code indexes it):
the extractor runs a list until the next list start / first invalid record.
Frame record: `s8 dx, s8 dy, u8 w, u8 h, u16 attr0, u16 attr1, u32 gfx` — one OBJ at entity
position + (dx,dy); attr0/attr1 give shape/size/8bpp/flip; `gfx` → webfoot-LZ tile data, uploaded to a
per-entity VRAM slot when the frame changes (`0x08006AD0`).
Entity fields: +0x48 definition, +0x4C current direction-list table, +0x10 direction, +0x12 tick,
+0x14 frame index, +0x158/+0x15C position (24.8). Draw/cull routine `0x08007A84`.

### 3.2 Buu's Fury
Definition pointer table `0x086B6BD8`, 298 entries (287 valid). Definition:

| off | type | meaning |
|---|---|---|
| +0x00 | u8 | highest animation index (count − 1) |
| +0x01..03 | u8 | flags |
| +0x04 | u32 | remap table or 0 |
| +0x08 | u8[4] | shadow/size bytes |
| +0x0C | u32 | flags |
| +0x10 | 4 × {s32 x0,y0,x1,y1} | body rectangle per direction |
| +0x50 | u32[n] | animation struct pointers (index 0: empty, or the halo table, see below) |

Animation struct: `u32 nframes`, then `nframes × 4` frame pointers, **frame-major**: entry for
(frame f, dir d) at `+4 + 16f + 4d` (`0x08010192`). Frame: `u32 nobj, s8 bbox dx,dy, u8 w,h`, then
`nobj ×` the 12-byte OBJ record above — multi-OBJ frames (bosses up to 64×64 pieces). Entity fields:
+0x50 current anim, +0x10 direction, +0x11 frame, +0x12 tick, +0x1A0/+0x1A4 position.

Halo sprites (defs 49, 74, 93, 108, 110, 112, 152, 173, 271, 274 — e.g. 108 = Goku 106 + halo): animation slot 0
points to a table indexed by the engine anim number (raw index − 1) → per animation `nframes × 4 dirs × {s8 dx,
s8 dy}` (frame-major): the halo OBJ (16×8 raw 8bpp at `0x080725F0`) is drawn at entity position + (dx, dy), above
the body. Exported as `attach: [dx, dy]` on each step plus entity `attachment`, and the `buu_halo` effect entity.

Ground shadows: header bytes +0x0A/+0x0B (shadow w, h) select an entry of the table `0x086B759C` (4 × 16 bytes:
u8 w, u8 h, u8, u8, u32 gfx, u32 VRAM slot, u16 attr0, u16 attr1; lookup `0x08013E38`): 4×2, 10×4, 16×6, 24×10.
The OBJ (colour 1 = black) is drawn at the entity position, behind the body, and does not follow the height
offset. Exported as the `buu_shadow_<w>x<h>` effect entities.

### 3.3 LoG1
Sprite definition pointers `0x085C0C80` (86) + projectile definitions referenced from code
(`0x086B3AA8/3AD4/3C50` ki blast large/medium/small, `0x086B376C` Kamehameha, `0x086B3FFC` Solar
Flare, `0x086B3400` Vegeta's ki blast). Definition (0x2C bytes):
`u16,u16; 4 × {s16 x0,y0,x1,y1}; u32 palette; u32 animation table`.
Animation table: rows of 4 strip pointers (one per direction); **row index = entity state** with the
names stored in ROM at `0x08010F08`: Standing, Walking, Death, Melee1, Melee2, Ki, Excited, Sad,
Transform, SSaiyaJin, Monkey, Monster, Raditz, Bubbles, Flying, Bored, Mallet.
Strip (16 bytes): `s16 dx, s16 dy, u16 w, u16 h, u8 tiles/frame, u8 flags (bit3 hflip), u8 OAM size,
u8 OAM shape, u32 gfx` — frames back-to-back at gfx + n·tiles·32 (DMA `0x0800B0C4`). Frame counts are
inferred from ROM layout. Developer labels: `u32[109]` at `0x080109F0`, sprite→definition `u8[109]` at
`0x0815A709` (tables from re/log1). Entity: +0x08 anim row, +0x12 frame, +0x13 facing, +0x1C def.

## 4. Animation timing / "bytecode"

There is **no animation bytecode** in any of the three games: frame lists carry no durations.
Each entity state's update function advances a tick counter and computes the frame index in code,
e.g. LoG2 walk = `(tick >> 3) & 3` (`0x08007532`), idle blink = frame 1 while `(tick & 0xFF) > 0xF0`
(`0x08007570`). Durations in `sprites.json` therefore come from **emulator measurement**
(`tools/extract/autobattle.py`, cached in `tools/extract/data/anim_obs_<game>.json`): the player is
driven into fights on every map while the per-entity animation state is logged each frame
(`tools/extract/animrec.py`). `timing_source` says `measured …`, a curated per-index default
(`names_<game>.json` `set_timing`), or `default` (8 frames). `observed_frame_orders` lists the frame
index sequences actually seen (e.g. kiblast `0,1,2,1,2` = repeated firing).

Semantic animation names come from: ROM strings (LoG1), and emulator observation of which index each
input/state selects (LoG2 Piccolo/Trunks, Buu Goku), applied per shared engine class (`anim_sets` in
`names_<game>.json`); unknown indices keep `animNN` / `special_NN` and always carry `raw_index`.

## 5. Effects, projectiles, portraits, items, UI

* **WF effects/projectiles** are code-driven: 12-byte records `s8 ox, s8 oy, u8 w, u8 h, u16 attr0,
  u16 attr1, u32 raw_gfx` (OBJ top-left = position − (ox,oy)), raw 8bpp tiles, e.g. LoG2 ki blast table
  `0x0840E47C` (used by projectile objects, OAM writer `0x08013CDE`). The extractor scans the ROM for runs
  of such records; grouping into per-direction animations is not in data (runs are exported in ROM order).
  LoG2 effects are named; Buu's ~285 runs get an automatic colour/size label.
* **Portraits**: LoG2 table `0x083EC9F4` (107), Buu `0x086F395C` (155) — webfoot-LZ, 64×64 linear 8bpp.
  LoG2 scouter `portrait_id − 8` = portrait index. LoG1 table `0x0815A55C` (47 → 36 unique) —
  {palette, Huffman→RLE 32×32 4bpp}.
* **Items**: LoG2 `0x086ADE24` (50 × 56 B; 16×16 field sprites + 32×32 menu icon), Buu supplies
  `0x083BD2F8` / equipment `0x086F82EC` (16×16 icons), LoG1 `0x085C224C` (34 × 24 B, {palette, 4bpp}).
* **UI**: HUD/menus are assembled at runtime; `ui_export.py` captures screens, HUD OBJs and the OBJ layer
  from states reached from boot.

## 6. Maps / tilesets

* WF: map layer = ROM *field resource* (LoG2 vtable `0x080056A1`): +0x14 u8 chunk columns, +0x15 rows,
  +0x18 chunk pointers → webfoot-LZ 0x800-byte chunks = 32×32 text-mode screen entries. LoG2 layer
  objects (vtable `0x08023868`, resource at +0x10, allocated BG3 first); Buu layer objects (vtable
  `0x08049410`, +4 BG index, +8 resource, +0x24/+0x28 chunk position; resource +4/+8 parallax rate).
  Tiles (8bpp) and palette are whatever the game loaded: maps are rendered after loading each map in the
  emulator (LoG2: hidden Debug menu MAP TEST, reached from boot by setting title object +8 = 2;
  Buu: the opening Snakeway "too far" trigger converted into an exit trigger — technique from re/buu).
* LoG1: map table `0x081CA09C` (44): {u16 screens w,h; u16 tiles w,h; 3 flag layers; entrances; …;
  +0x20/+0x24/+0x28 three BG planes as `u32[w*h]` → 30×20 raw screens (0x4B0 bytes); +0x2C palette}.
  Rendered after forcing `load_map(map, default entrance)` (`0x08001FA8`) at boot.
* Output per map: `composite.png`, per-layer `bgN.png`, `screenshot.png`, and per-tileset sheets.
* Buu collision: each map header (found through its field resources at header + 0x14 + 4·bg) points at a walk grid
  (+0x34) and a projectile grid (+0x38), webfoot-LZ 256x256 bits of 8x8 tiles (1 = passable). Format, how actors and
  shots test them, enemy path finding and the two composites that are offset from the world grid (7-6, 18-14):
  `tools/battle/COLLISION.md` (extractor `tools/battle/buu_collision.py` -> `data/battle/buu_collision.json`).

## 7. sprites.json

    {game, conventions, entities:[…], maps:[…], tilesets:[…], ui_captures:[…]}

Entity: `id` (slug, = folder), `name`, `category` (player|enemy|boss|npc|projectile|effect|item|portrait),
`name_confidence`, `sprite_index` (engine id), `rom{…}`, `header`, `sheet`, `frames[{x,y,w,h,anchor{x,y},
rom,png}]`, `animations{name:{raw_index, loop, timing_source, observed_frame_orders,
directions{down|up|left|right|any:[{frame, duration_frames, flip, rom}]}}}`, `hitboxes`.
`anchor` = pixel of the frame PNG that sits on the entity position; `flip` = mirror the referenced frame
horizontally around its anchor (mirrored duplicates are stored once).

## 8. What was extracted

| game | sprite entities | effects/projectiles | portraits | items | maps |
|---|---|---|---|---|---|
| LoG1 | 84 (3 player, 14 boss, 17 enemy, 50 npc) + 6 projectile defs | (projectile defs) | 36 | 34 | 42 |
| LoG2 | 150 (21 player, 16 boss, 52 enemy, 60 npc, 1 item) | 30 runs (249 frames) | 107 | 50 | all MAP TEST maps |
| Buu  | 287 (30 player, 18 boss, ~119 enemy, ~100 npc …) | ~285 runs | 155 | 244 | all reachable areas |

Names: LoG1 = ROM developer labels (high confidence). LoG2 = scouter database names where present
(high), the rest visual. Buu = character/portrait names from the ROM string table where obvious, most
enemies/NPCs visual/descriptive (low confidence).

## 9. Known gaps

* Animation durations are measured only for animations the auto-battler triggered; other animations use
  per-index defaults. Exact frame logic lives in per-state code (not in data).
* Attack hitboxes are code-driven (see `data/log2/enemies.json` from the mechanics work); only per-direction
  body rectangles are exported.
* WF effect runs are not split into per-direction animations; Buu effects are not individually named.
* Many Buu enemy/NPC names are descriptive guesses; LoG2 NPC names likewise.
* Fonts, the title screens, and full-screen cutscene art (LoG2 `0x087FBAD4`, 181 × 240×160 8bpp blobs) are
  not decoded beyond the UI captures.
* Maps: alpha-blended overlay layers are composited opaquely; parallax layers align at capture camera.
