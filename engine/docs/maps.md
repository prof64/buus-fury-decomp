# Maps (crate `wf-maps`)

Every Buu's Fury map decoded straight from the ROM: tilemaps, the tileset the map loader builds in VRAM, the
palette, animated tiles, layer scrolling and collision. No emulator. Extraction of all 427 maps (PNG encoding
included) takes ~0.6 s natively (release); the crate builds for `wasm32-unknown-unknown`.

All addresses are GBA bus addresses; Thumb functions without the +1. Research scripts: `crates/wf-maps/re/`
(capstone + the repo emulator; they write to `re/out/`, create it first).

## 1. Where the data comes from

### Area table `0x0808E2E0` (452 × 0x38) — looked up by `0x080089FC(world, area)`

| off | type | meaning |
|---|---|---|
| +0x00 | u8 | world |
| +0x01 | u8 | area (key `world-area`, used by exits / warps) |
| +0x02 | u8 | number of map variants (1, or 2 for six areas) |
| +0x03 | u8 | count of conditional objects at +0x10 (8-byte `{condition, object}` records, `0x08008AA0`) |
| +0x04 | u8 | count of objects at +0x14 (`[obj+4]` = condition, created by the area loader `0x08008C9E`) |
| +0x05 | u8 | count of objects at +0x18 (`0x08008AFE`) |
| +0x08 | u32 | 0/1 flag (0 in shops); not decoded |
| +0x0C | u32 | area name string id (0 = none) |
| +0x24 | u32 | song id; 0xFF = silence; a ROM pointer = bytecode script choosing the song (`tools/audio/AUDIO.md`) |
| +0x28 | u32 | not decoded |
| +0x2C | u32 | → `u32[variants]` map header pointers; `0x080089CC(area, n)` returns entry n−1 |
| +0x30 | u32 | script run when the area loads (or 0) |

### Map header (e.g. Southwest Forest 8-3 `0x08240044`), read by the map init `0x0800953E`

| off | type | meaning |
|---|---|---|
| +0x00 | u16 | BLDCNT |
| +0x02 / +0x03 | u8 | BLDALPHA EVA / EVB |
| +0x04..+0x07 | u8[4] | BG0..3 priority (added to BGxCNT) |
| +0x08 / +0x0A | u16 | camera range: world size = (w + 239, h + 159) px |
| +0x0C / +0x10 | u32, ptr | number of tile animations / → pointers to their descriptors |
| +0x14..+0x20 | ptr[4] | BG0..3 layer resources |
| +0x24 / +0x28 | u32, ptr | number / → 0x18-byte records with rectangles (not decoded) |
| +0x2C | ptr | webfoot-LZ 0x400-byte blob (looks like a 32×32 8bpp minimap; not decoded) |
| +0x34 / +0x38 | ptr | walk / projectile collision grids (webfoot-LZ, 0x2000 bytes) |
| +0x3C | u32 | display mode index: 0 = mode 0 (all four text BGs), 1 = mode 1 (BG2 affine; only map 0-50) |
| +0x48 | ptr | webfoot-LZ **tile list** |
| +0x4C | ptr | **tile chunk table** `0x08179764` (187 webfoot-LZ blobs of 0x4000 bytes = 256 8bpp tiles; shared by all maps) |

**BG control.** BGxCNT = `u16[4]` at `0x080716D8 + 8·mode` + priority: mode 0 = `0x1C80 0x1D80 0x1E80 0x1F80`
(8bpp, charbase 0, screenblocks 28..31, 256×256 hardware maps). DISPCNT `0x1F40` (mode 0) / `0x1741` (mode 1).

**Tileset (`0x08009BD4`).** The tile list inflates to `n` u16 entries, delta coded: `e[i] += e[i−1] + 1`. VRAM tile `i`
(charbase 0, 64 bytes each) = tile `e[i] & 0xFF` of chunk `e[i] >> 8` of the chunk table. Then frame 0 of every tile
animation is uploaded. Tiles past `n` are whatever an earlier map left (the extractor leaves them blank).

**Tile animations (`0x08009B80` / `0x08009CC2` / `0x08009B12`).** Descriptor: `u8 nframes, u8 ntiles, u16 dest tile,
ptr[nframes]`; each frame record = `u16 duration (60 Hz frames), u16 pad`, then a **headerless** webfoot-LZ stream
of `ntiles × 64` bytes (the decoder is called on the stream directly) copied to VRAM tile `dest`. 182 of 427 maps
animate tiles (water, flowers, lava…).

**Palette.** One global 512-colour palette, RGB888 triplets at `0x080701B0` (BG 0..255, OBJ 256..511), written
to palette RAM by the fade routine `0x08007074` (`(c·(256−level) + target·level) >> 11` per channel, BGR555). Every
map uses the BG half unchanged (verified in palette RAM for 8-3, 0-1, 18-14 at full brightness).

**Layer resources** (resource +0x00 = its create function):

| kind | create | layout | scroll |
|---|---|---|---|
| tilemap (1439 layers) | `0x08008087` → layer object vtable `0x08049410` | +4/+8 s32 scroll rate x/y (1/1024), +0xC/+0x10 s32 offset (1/1024 px), +0x14/+0x15 u8 chunk cols/rows, +0x18 `ptr[cols·rows]` → webfoot-LZ 0x800-byte chunks of 32×32 screen entries | `0x08007E84`: scroll = `(camera · rate + offset + shake) >> 10`; chunks streamed into the 32×32 hardware ring |
| empty (226) | `0x08007A41` | — | BG off |
| wrap (41) | `0x0800897B` → vtable `0x080498B0` | +4 rate (used for both axes), +0xC/+0x10 offset, +0x14/+0x18 s32 velocity (1/1024 px per frame), +0x1C → one 0x800-byte chunk | `0x08008924`: `camera·rate + offset + velocity·frame`, the 256×256 screen wraps (fog / clouds / water overlays) |
| panorama (1, map 0-50 BG1) | `0x08007B63` → `0x08007A60`, vtable `0x08049604` | +4 / +8 → two webfoot-LZ 0x800-byte 32×32 screens | `0x08007B26`: hofs = (heading << 3) >> 10, columns from both screens (`0x08007AB4`); exported to `mode7.json` (World map below) |
| mode-7 ground (1, map 0-50 BG2) | `0x080088BD` → `0x080080C0`, vtable `0x08049C84` | +4 / +5 u8 chunk cols / rows (14 × 10), +8 `ptr[]` → webfoot-LZ 32×32 tile-byte chunks | per-scanline affine table `0x080086D8`, written by the HBlank IRQ `0x080080BE`; exported to `mode7.json` |

## 2. Output (paths relative to the content pack root)

* `areas.json` — every area record: `key` (`"8-3"`), `world`, `area`, `name` (string table), `song` (id / `null` =
  silence / `{"script": addr}`), `maps` (map ids per variant), and `rom` (raw fields above, hex).
* `maps/index.json` — `maps[]`: `id`, `areas` (keys sharing this map), `header`, `size_px` (world size), `tileset`,
  `layers` (BG indices), `tile_animations`; `tilesets[]`: id + number of maps using it.
* Map ids: `area_WW_AAA` of the first area using the header (`_vN` for variant N > 1); 427 distinct headers for
  452 areas (11 headers are shared, e.g. the five Z-Mart areas).
* `maps/<id>/map.json` — **Tiled** map (orthogonal, 8×8 tiles, version 1.10): one `tilelayer` per BG in draw order
  (back to front by GBA priority, ties: higher BG index further back). Layer data = `base64` + `zlib` little-endian
  u32 gids; gid = GBA tile + 1 with Tiled's horizontal / vertical flip flags (bits 31 / 30). All layers are padded to
  the map size (max layer / world size, tiles). `offsetx/offsety` = layer position at camera (0, 0) (= −offset px),
  `parallaxx/y` = scroll rate. Layer properties: `bg`, `priority`, `kind` (`tilemap` | `wrap`), `layer_w_tiles`,
  `layer_h_tiles`, `scroll_rate_x/y` (1.0 = moves with the camera), `scroll_offset_x/y_px`, `png`, `rom_resource`;
  wrap layers add `wrap`, `velocity_x/y_px_per_frame`. Map properties: `world_w_px`, `world_h_px`, `camera_max_x/y`,
  `display_mode`, `bldcnt`, `blend_eva`, `blend_evb` (alpha blending registers for overlays), `tileset`,
  `vram_tiles`, `unsupported_layers`, ROM addresses.
* `maps/tilesets/<hash>.png` + `.json` — Tiled external tileset (400 distinct, deduplicated by content): indexed PNG
  with the global BG palette (index 0 transparent), 16 tiles per row; tiles `0..vram_tiles−1` = VRAM after the load,
  followed by every animation frame's tiles. Animated tiles carry Tiled `animation`s (ms) and the GBA timing
  (`tile_animations`: dest tile, tile count, per-frame durations in 60 Hz frames).
* `maps/<id>/bgN.png` — each layer rendered alone (indexed, transparent index 0), full layer size.
* `maps/<id>/composite.png` — the tilemap layers over the backdrop (palette 0), **world-aligned**: pixel (x, y) = world
  (x, y), size = world size; layers at camera (0, 0). Wrap overlays are not composited (they move and blend).
* `maps/<id>/collision.json` — `tile_px` 8, `width`/`height` 256 (the full grid, lossless), `walk` and `shot`: run
  lengths over the 65 536 bits row-major, first run = blocked, LEB128 varints, base64 (the encoding of
  `data/battle/buu_collision.json`, but never clipped); `walk_bounds` (tile bbox of passable walk tiles, x1/y1
  exclusive); `origin` [0, 0]. 1 = passable. Grid tile (0, 0) = world px (0, 0) = composite px (0, 0).

**Composite origins (7-6, 18-14).** The emulator composites were cropped to their content and aligned at the capture
camera, so COLLISION.md had to measure `origin` for the World Tournament stadium 7-6 (+16, 0) and HFIL 18-14
(−5, 0). Here every composite and grid shares the world origin by construction, so no origin is needed (always
[0, 0]). Check: the ROM-derived content crop of 7-6 starts at world (16, 0), exactly the measured origin; 18-14's
offset came from its 1.2× parallax BG0 aligned at the capture camera (not a world offset).

### World map (`maps/area_00_050/mode7.json`, `wf-maps/src/worldmap.rs`)

Area 0-50 is the only header with +0x3C != 0 (display mode 1, DISPCNT `0x1741`: BG0-2 + OBJ). Its two special layers
and the screen pieces the renderer needs:

| key | content |
|---|---|
| `ground` | `bg`, `priority`, `width_tiles` × `height_tiles` (448 × 320 = 3584 × 2560 px), `tiles` = base64 u8 VRAM tile index per 8×8 world tile (row-major; 8bpp tiles of the map's tileset, affine BG: no flips) |
| `panorama` | `bg`, `priority`, `chunks` = two base64 32×32 text screens (u16 entries: tile, hflip 0x400, vflip 0x800), scroll rule |
| `tables` | `sin` (s32[256], `0x087B6EEC`), `csc` (65536 / sin, `0x087B72EC`), `line_angle` (s16[160] per-scanline pitch offset, `0x08071420`), `line_cos` (u16[160], `0x08071560`) |
| `label` | `mode7_label.png` (240×16 bar, webfoot-LZ `0x0807248C`, OBJ palette), font `small`, pen (120, 11) centred, colour 2; OBJs at y 136, priority 1 |
| `marker` | `mode7_marker.png` (8×8, `0x0874EA00`), the mini-map box of the map script's `spawn_box` (0-50: 160, 0, 240, 56) and the placement rule |

Billboards are the 94 header props of `objects.json` (record +0x16 = height class: 0x28 → z 110 px, 0x3C → on the
ground, else 60 px; ctor `0x0800CB14`, update `0x0800CBA8`, draw `0x0800CC6C`). The camera / projection / scanline
formulas are documented in `crates/wf-render/src/mode7.rs` and engine/docs/frontend.md §9b. Every on-screen ground
sample equals the world tile at its true position (the game streams 256 px chunks of the frustum into its 512 px
ring), verified against VRAM.

## 3. Parity (`cargo test -p wf-maps`, run `--nocapture` for the summary)

* **Collision: 418 / 418 maps identical** to `data/battle/buu_collision.json` (walk + shot, inside its clip rectangle;
  this output keeps the full 256×256 grids, the reference clips to the walkable bounding box).
* **Map list:** all 418 rendered emulator maps matched to a ROM header (resources per BG); the ROM has 427 distinct
  headers (the extra 9: second variants and maps the warp could not reach).
* **Layers vs emulator renders (1415 layers):**
  * 1282 pixel-identical.
  * 129 differ only on animated tiles: 10 828 of the 10 970 differing 8×8 cells show exactly another frame of that
    tile's animation (the capture was taken later than frame 0); the other 142 cells are mid-upload / overlapping
    animation ranges.
  * 4 (all of 15-9, Nataday Village) differ everywhere: the emulator capture was taken mid fade-in (every colour of
    palette RAM, BG and OBJ, at exactly half intensity at capture time); the ROM palette is right.
  * 0 differences from tiles past the loaded tile list (VRAM leftovers) or anything else.
* **Composites:** 375 maps whose layers all scroll at 1× and whose emulator composite has the same crop: 281
  pixel-identical; the other 94 differ only through the animated-tile / fade differences above (no composite differs
  when its layers are identical). 54 emulator composites have another size: cropped to content (interiors), or
  parallax layers aligned at the capture camera (0-1, 12-x, 21-x, 18-14, …); the world-aligned output is the
  intended one.
* `tests/content.rs`: map.json gids, tileset PNG + layer PNGs and collision.json decode back to exactly the decoded
  ROM data for all 427 maps.

## 4. Not covered / open

* Map 0-50: the composite / bgN PNGs contain BG0 only (the ground and sky are drawn per frame from `mode7.json`).
* Header +0x24/+0x28 rectangles, +0x2C minimap blob, +0x30, +0x40, +0x44 and area +0x08/+0x28 are recorded raw,
  not interpreted. Area objects (NPCs, triggers, exits) are not exported here.
* Blending (BLDCNT/EVA/EVB) is exported as properties; composites draw overlays opaquely like the old renders.
* Palette effects driven by scripts (fades, flashes) are runtime behaviour, not map data.
