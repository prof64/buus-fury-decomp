# Buu's Fury menus — reverse-engineering notes

Tools (all headless, muted; `.venv/bin/python`):

| file | what |
|---|---|
| `d.py ADDR...` / `d.py vt VT [N]` | whole-function Thumb disassembly (literal pools resolved, `bl` targets named); relative vtables (fn = vt + entry) |
| `cap.py` | `Cap(state)`: per video frame keys, sound calls (`play` id = (ptr − 0x083192A8) / 12), screen sha1; key programs `'START:1', '-:40', 'A+UP:2'` |
| `capture.py [scenario]` | ground truth for `engine/crates/wf-app/tests/menu_parity.rs`: `work/buu/menu/<scenario>/` = `fNNNN.png`, `frames.json` (keys, snd, h, `lf` = 0x03000F94, `mode` = 0x03000F90), `story.json` (GameData decoded at the START frame) |
| `timing.py SCENARIO [DRAW]` | CPU time of redraws (cycle-stamped breakpoints, `emut/` = the emulator shim with `mTimingGlobalTime` + VCOUNT on hits) |
| `scratch/` | one-off probes (t3: container children / focus / sel per frame; t4: call order; t5: blitted button frames) |

## The mode loop (`run_mode 0x0803D8A0`, loop `0x0803D7FA`)

Mode object vt (relative: fn = vt + entry): +4 enter, +8 dtor, +0xC **update**, +0x14 **step** (done when it
returns non-zero). Iteration: `update()` → VBlank wait (`0x0803F296`, returns at line 160) → frame counter
`0x03000F94` += 1 → input read `0x0803D4A0` into `0x03003300` (+2 held, +4 pressed = edges since the *previous
read*, +6 released) → a global object check → `step()`. A long `update` (a redraw) spans several video frames:
no input is read in them (taps there are lost) — the remake's `Menu.lag`. `0x0803D850(mode)` replaces the
running mode (L / R page switch), `0x0803D876(code)` ends it with a result, a nested `run_mode` runs a sub-mode
(Skills) inside an event handler.

Globals `0x086B8048`: [0] input `0x03003300`, [1] sound base `0x03003898` (+0x5C4 channel), [2] bold font
`0x082B9BA0`, [3] menu font `0x0807012C`, [4] GameData `0x03001068`, then the portrait rect `0x086B805C`
(16, 64, 80, 128). Small font `0x0806E050`, dialog `0x0806EE6C`, digits `0x0806F150`. Sounds: 40 = open / page /
confirm, 39 = cursor move, 41 = accept / buy.

## Widget toolkit (ported in `engine/crates/wf-core/src/menu/kit.rs`)

**Container** (base vt `0x0804920C`, 0x108+ bytes): +4 child count, +8.. children, **+0x108 pressed button**,
**+0x10C selected control id**, **+0x110 drawn** (0 = redraw everything at the next update; pages draw the whole
surface each time: frame, content, then every child's draw vt +8, then copy 38 400 bytes to VRAM 0x06000040).
Step `0x080067F4`: every child's step (+0x14); input: only the pressed button (+0x10) if any, else the children in
order until one returns non-zero; if nothing consumed it, the page's own check (+0x28). `0x0800664E` add child
(calls its vt +0xC "added"), `0x0800666C` remove (compacts the array). Child → page events: page vt +0x20
`handle_event(page, src, code)`.

Widget vtables (slots: 0 dtor, 1 cursor point / query, 2 draw(ctx), 3 added, 4 input, 5 step):

| class | ctor / vt | fields | behaviour |
|---|---|---|---|
| ButtonPrompt | `0x08013E64` / `0x08049880` (0x3C) | +8 enabled, +0xC repeat, +0x10 visible, +0x14 code, +0x18/+0x1C repeat countdown / interval, +0x20 table entry `0x086B77A4` (A B UP DOWN LEFT RIGHT L R: key mask, w, h, 3 frames), +0x24.. rect | not pressed: key *pressed* + enabled → becomes the page's pressed button (redraw), returns 0 (others still see the key); pressed: key *released* → unpress, event (if enabled); held + repeat: 16, then halving to 4 frames per event; returns 1. Draw: frame 2 disabled, 1 pressed, else 0 |
| Label | `0x08014124` / `0x0804A254` (0x20) | x, y, colour, string, align, font | shadowed glyphs (shadow colour 1) |
| Control (base) | `0x08005FBE` / `0x0804A238` | +8 id, +0xC/+0x10/+0x14/+0x18 neighbours up / down / left / right | input `0x08005FD4`: only the selected one: UP/DOWN/LEFT/RIGHT to a neighbour ≥ 0 → sfx 39, select, redraw, consumed; else vt +0x18 |
| TextCtl (stat rows) | `0x080060F8` / `0x0804B2DC` | +0x1C string, +0x20 x, +0x24 y, +0x28 code, +0x2C align | menu font, colour 6 selected else 2; A / START → `handle_event(code)`, returns the handler's answer; cursor point (x aligned, y − 4) |
| Tabs | `0x08015D10` / `0x08049C68` (0x114) | rect +0x24, tabs +0x4C {string, value, width}, +0x34 first, +0x38 end, +0x3C cur, +0x40 marked, +0x48 x0, arrow buttons +0x1C / +0x20 (LEFT / RIGHT prompts, same code) | layout on first draw `0x08015DBE` (arrows hidden when all fit), LEFT / RIGHT move `0x08015E94` / `0x08015F1C`, event code, never consumes; tab box `0x08015BB8`, colours `0x086B8080` |
| Cursor | `0x08016208` / `0x08049898` (0x10) | +8 counter, +0xC picture `0x086B7E8C` | step: counter++, every 8 a redraw; draw while bit 3 clear at the container's cursor point (−16, −8) |

## Text (`menu/text.rs`)

`draw_string 0x0803DADA(ctx, s)`: ctx +0 flags (bits 0-1 align left / centre / right, bit 2 single line), +1 line
height, +2 shadow colour, +3 colour, +4 x, +8 y, +0xC font, +0x20 glyph fn (`0x0803F444` plain, `0x0803F5A4`
shadowed), +0x10 clip rect. Per line: width = `measure2` over the line's units (every unit width + 1; units
without a glyph — colour codes included — count 1 px); left: x (pen x += width at the string's end), centre:
x − (w + 1) / 2, right: x − w; glyph tops at y − 12; `0x0008 n` sets colour n for good. printf `0x0803DB7C`:
`%[0][w]d`, `%s`, `%S`, others dropped. Shadowed glyph: per column k ≤ w: glyph bits → colour, else bits of
column k − 1 shifted down one row → shadow colour (rect w + 1 × 17).

## Pause menu entry / exit (field main loop `0x08006E3C`)

After the objects, triggers and GameData scripts: SELECT (2D maps) → map screen (modal object vt `0x0804E384`,
0x8044 bytes, ctor `0x080256F0`); START with the current character's HP ≠ 0 → sfx 40, fade-out mode
(`0x0804A920`: palette fade `0x08007074` level += 16, 16 iterations; the world is drawn, not updated), world save
`0x08011DB4`, the Status mode (0x158 bytes, vt `0x0804AC88`), world restore `0x08011E0E`, camera, fade-in mode
(`0x0804A4B8`). Current character 8 → a message mode instead (`0x0804A014`, string from `0x0802BFBC(0x27)`).
Measured (frame 0 = START): fade 1..16, black 17..20 (frame 20 = the copy racing the scanout), page at 21, first
input read at 22; closing iteration still shows the page, 24 black frames, fade-in 16 frames, world from the next.

## Status page (`menu/status.rs`)

See the module doc. Children order (matters for input): Skills label, DOWN prompt (code 3), L (6), R (7),
Journal / Inventory labels, STR / POW / END (ids 1-3, cycling) / SPD (id −1) rows, the party tabs (id 0, code 100,
rect (16, 30)-(224, 46)) + its arrows, then the A / B prompts (`0x08014C5C`). Allocation (A "Use Points"): cursor
child, sel 1, Skills prompt disabled; LEFT / RIGHT (done check `0x080157A4`) move points (max = level); A accepts
(record +0x28.. written, RecalcStats, sfx 41), B cancels. START closes from any state (done check).

## CPU time (why the tests use an oracle)

A Status redraw is ~0.94-0.98 M cycles of work (glyphs dominate) plus the IRQ handler (`0x03000B6C`, nested,
~15 entries per frame: VBlank + DMA2 for the sound driver), whose share varies with what the sound driver mixes
(±30 k cycles per redraw). The redraw ends around the 4th VBlank: 4 or 5 frames per iteration, and the VRAM
copy may race the scanout (a torn frame). The remake uses a per-screen frame count; `menu_parity.rs` takes the
iteration boundaries and the display changes from the capture (`lf`, picture hashes) so it tests the logic and the
pictures exactly; `MENU_PARITY_FREE=1` measures the model.
