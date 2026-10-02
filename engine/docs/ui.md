# UI (crate `wf-ui`)

Fonts, dialogue boxes, HUD, pause-menu primitives, map screen and boot / title screens decoded from the ROM (no
emulator, no screenshots). Category `ui` → `content/base/ui/`. Builds for `wasm32-unknown-unknown`.

The game has no tile-based UI: everything is drawn **in software into 8bpp surfaces** from LZ pictures, 1bpp fonts
and a few procedural primitives, then uploaded to VRAM:

| screen | surface | shown as |
|---|---|---|
| pause menu pages (Status, Inventory, Equip, Options, Journal) | 240×160 | BG2, identity tilemap (tiles 1..600, screenblock 30), DISPCNT `0x0400` |
| dialogue box | 160×64 (`dialog obj + 0x80`) | three 8bpp OBJs 64×64 + 64×64 + 32×64 at x 40 / 104 / 168, uploaded each frame by `0x0800ADA0` |
| HUD | 80×16 (0x500-byte OBJ tile buffer) | OBJs at (8, 8): 32×16, 16×16, 32×16, tiles 880.., priority 1 |
| boot / title | 240×160 picture | BG, full screen |
| map screen | tiles + tilemap | BG0 frame, BG2 page, BG3 fill |

All UI pictures use the **UI palette** `0x0805652C` (256 BGR555). Menus, boot and map screen load it into both
palette halves (`0x08024BD0`); in game, the OBJ half of the global palette `0x080701B0` holds the same colours
(255 / 256 identical), which the HUD and dialogue boxes use.

## 1. Output (`ui/`)

| path | content |
|---|---|
| `index.json` | fonts, pictures (id, png, size, frames, rom, code refs), button table |
| `palette.json` | the UI palette (`#rrggbb` × 256, `c<<3 | c>>2` expansion) |
| `fonts/<id>.png` + `.json` | atlas: 16×16 grid of 16×16 cells, cell = UTF-16 code unit (< 0xFF), glyph at the cell's top-left, white on transparent; metrics per glyph (`w`, `advance` = w + 1, `rows`, `rom`), ascent, control codes |
| `images/<id>.png` | indexed PNGs (UI palette; index 0 transparent except full screens) |
| `buttons/<a,b,up,down,left,right,l,r>.png` | button prompts, 3 animation frames side by side |
| `dialog.json` | box styles, layout rules, screen placement |
| `hud.json`, `hud/frame.png`, `hud/gauge_ss.png`, `hud/gauge_ss3.png` | HUD pictures and every gauge step as pixel coordinates |
| `mapscreen.json`, `mapscreen/{frame,frame_tiles,button_l,button_r,world_map}.png`, `mapscreen/frame_map.json` | map screen |
| `menu.json`, `menu/page_skeleton.png` | menu surface, primitives, Status page constants |

## 2. Fonts — renderer `0x0803F444`, string drawer `0x0803DADA`, measure `0x0803FFE8` / `0x0804001A`

Font record (12 bytes): `+0 u16* T`, `+4 u32 wide_w`, `+8 u16* wide_data`. For a code unit `c < 0xFF`, `g = T[c]`
(0 = no glyph); the glyph at `T + 2g` is `u16 width` then `width` u16 **columns**, bit k = row k (≤ 16 rows). Advance =
width + 1. `c ≥ 0xFF` would use fixed-width glyphs at `wide_data` (no font here has any). The pen y is 12 px below
the glyph cell top. Set pixels take the surface's current colour index.

| id | record | glyphs | use |
|---|---|---|---|
| `small` | `0x0806E050` | 221 | small capitals: HUD captions, `Select` / `Cancel` |
| `dialog` | `0x0806EE6C` | 223 | dialogue boxes |
| `digits` | `0x0806F150` | 26 (0-9 drawn, 0x20-0x2F blank 1-px) | counters |
| `menu` | `0x0807012C` | 223 | menu text, title-screen menu |
| `bold` | `0x082B9BA0` | 223 | page titles |

(`0x087B5BA4` is a copy of the `small` table inside the IWRAM code image; not exported.)

**Encoding.** Text is plain UTF-16 (string table `0x0806BCE8`, dialogue banks — see `docs/world.md`); a code unit
indexes the font directly (ASCII, Latin-1, Œ/œ at 0x8C/0x9C). Control codes handled by the drawers:

| unit | effect |
|---|---|
| `0x0000` | end |
| `0x000A` | new line (line height per text box: 15 in dialogue boxes) |
| `0x000B` | end of box / page (the dialogue line fitter stops) |
| `0x0008, n` | set colour index n (2 = normal white; menus use 3, 5, 6, 7, 0x1D) |
| `0x0020` | space — the only word-wrap break |
| `%s` / `%d` | printf slots filled before drawing |

The width routines sum `width + 1` per unit; the line fitter `0x0803DE6A` skips colour pairs, breaks at the last
fitting space; a too-long line is cut with `..` (`0x0803DCC4`).

## 3. Dialogue box — module `0x0800AD00..0x0800B300`, globals `0x08071700`

Styles (`0x08071730`, 20 bytes: `max_lines, wrap_width, x, y, image`): style 0 = 4 lines, wrap 144, text at (8, 15),
image `0x08071758`; style 1 = 2 lines, wrap 128, (16, 30), image `0x08071A6C`. Images: webfoot-LZ 160×64 8bpp.
Layout (`0x0800AEF6`): lines from the line fitter; each line centred `x = style.x + (wrap − width) / 2`; the block is
centred vertically, first pen y = `style.y + 7·(max_lines − lines)`, +15 per line; colour 2. Box placement by the
text's first character (`!` top, `@` middle, `#` bottom, see `docs/world.md`); OBJs are alpha-blended.

## 4. HUD — draw routine `0x0800B99C..0x0800BCC0` (every frame)

Buffer (OBJ tile order): `0x000..0x200` selected skill icon (`icons/skills/skill_NN[_alt].png`), `0x200..0x500` frame
`0x087137D8` (raw; full HP and EP bars drawn in). Gauges (character record +0x16..+0x20):
`hp = HP·41/maxHP`, `ep = EP·41/maxEP`, `ss = SS·41/maxEP`, `ss3 = SS3·41/maxEP`, `ss = max(ss, ss3)`, `ep = max(ep, ss)`.
* HP (`0x08071CB0`, 41 × 3 pixel offsets): steps ≥ hp → colour 1.
* EP (`0x08071F40`): steps < ss3 from `0x08713FD8`, steps < ss from `0x08713AD8` (full-HUD pictures in the gauge
  colours), steps ≥ max(ep, …) → colour 1.
* 72-step experience line (`0x080721D0`, 2 offsets): steps ≥ `0x0801363E(player, 72)` → colour 1.
* HP·5 ≤ maxHP plays sound `0x08319488` every 60 frames.
`hud.json` lists every step's pixels in the 80×16 picture.

## 4b. Area-name banner — ctor `0x0800BF34`, vt `0x0804B528`, surface `0x0800BE90`, draw `0x0800BFCE`

Created by EnterArea after the player and the HUD when the new area's name string id differs from the last one
shown (`0x03001984`) and is not 0 (the 3D world map only records the id). Surface 240x16 8bpp: background =
webfoot-LZ `0x0807248C` (type 1, 0xF00 bytes; `images/banner.png`), then the area name with the `small` font through
the string drawer in centre mode (x 120: x0 = 120 - (width + 1) / 2; pen y 11, glyph tops at pen y - 12), shadow
colour 1 at (+1, +1) and colour 2 per glyph. Shown as 7 x 32x16 + 1 x 16x16 8bpp affine OBJs at x 0..224, y 128,
priority 0. Timer 240, -1 before every draw (239 visible frames); matrix a = 0x100, d = 0x100 while the timer is > 16,
else 0x1000 / timer. Exported: `images/banner.png`, `banner.json` (all constants). VERIFIED: surface = OBJ VRAM,
screen identical every frame (frontend `tests/banner_parity.rs`).

## 5. Pause menu

Page surface 240×160 (BG2). Procedural primitives reproduced in `menu.rs`:
* `0x08004F1C` background: rows alternate colour 0x01 / 0x30.
* `0x08004F50` panel(rect): bevel 0x1D / 0x6E / 0x3F, two dark rows 0x01, inner line 0x30, 3-px shadow (0x01).
Status page (`0x08006718`): background (0, 0, 240, 160), panel (8, 24)–(232, 152), title in `bold`, centred at
(120, 16), colour 5; portrait rect (16, 64)–(80, 128) (`0x086B805C`), portrait from `portraits/`. Pages cycle
Status → Inventory → Equip → Options → Journal with L / R (header shows neighbours + `buttons/l.png`, `r.png`).
Pieces: `cursor` (`0x086B7E8C`), journal stars (`0x086B7ED8`/`F3C`/`FAC`), HP/EP gauge 53×9 (`0x086B8294`),
equipment-slot / item icons = `icons/` (wf-extract), emote strips 8×8 (`0x086B814C..0x086B8244`, names visual).

## 6. Map screen (SELECT) — setup `0x08024A9C`

Frame BG0: tiles `0x087315E8` (40) at tile 0x378, tilemap `0x087317C8` (32×32, +0x378). L / R prompts: first 4 tiles
of `0x087318B4` / `0x08731914` as 2×2 blocks at (0, 0) / (28, 0) when a previous / next page exists. BG3 = tile 0x377
(raw `0x0873198C`, green grid) on the world page, empty on the area page. World map page: `0x08732604` = 600 tiles,
row-major 240×160 picture (BG2); visited-area outline drawn by script `0x0877D034`.

## 7. Boot screens

`screen_legal` `0x08055620`, `screen_atari` `0x086F9EC8`, `screen_funimation` `0x086FA398`, `screen_webfoot`
`0x086FABF4`, `screen_title` `0x083BE3D4` — 240×160 linear 8bpp (BG, UI palette). The title's START / OPTIONS menu
(box (80, 104)–(160, 144), `menu` font, `cursor`) is drawn over the art at runtime.

## 8. Parity (`cargo test -p wf-ui -- --nocapture`)

References in `crates/wf-ui/tests/data/` come from `re/buu/ui/make_refs.py` (emulator states built by
`re/buu/ui/{cap,tour,boot}.py`).
* Boot screens: legal / Atari / FUNimation / Webfoot **pixel-identical** to emulator screenshots (RGB); title
  identical outside the runtime menu box.
* Dialogue: the 160×64 surface of the intro's last page ("normal...") — box picture + text laid out by the rules of §3
  from the full sentence (wrap, paging, centring) — **identical** (0 / 10 240 px).
* HUD: the 0x500-byte OBJ tile buffer **identical** to VRAM (frame + gauges from the character values).
* Map screen: BG3 + BG0 frame with L / R **identical** to VRAM; world-map tiles identical (SHA-1).
* Menu: procedural panel frame identical on all 4 816 frame pixels of the Status page (page content differs, as
  expected).
* Headerless UI LZ streams decode identically whatever the initial literal width.

## 9. Gaps

* Page contents other than Status (Inventory, Equip, Options, Journal lists, header tabs, probably `0x08005096`),
  shop / save / character-switch menus, game over, chapter cards, credits: drawn by code; pieces exported where
  found, positions not tabulated.
* Area-map pages of the map screen (tile + map blob pairs `0x08733040..0x0874F058`, records like `0x08733AE4`) are
  not decoded; nor the outline script.
* Title-screen menu overlay and the dialogue portrait / speaker-name variant (wrap − 0x14) are documented, not rendered.
* Emote strip names are visual; their frame timing is in the (unexplored) emote object.
* Buu's Fury has no scouter / encyclopedia screen; the Journal is the quest log.
