# Mini-games, staff credits, map screen — RE notes (menu fork D)

Code: `engine/crates/wf-core/src/menu/{minigame,credits,mapscreen}.rs`; content `wf-ui/src/menupics/minigame.rs`
(`ui/menu/mg_bar.png`, `mg_marker.png`), `wf-ui/src/mapscreen.rs` (`ui/mapscreen/area_maps.json`, `area_NN*.png`,
`bg0_<l><r>.png`, `grid.png`, `marker.png`, `arrow_h/v.png`). Scenarios: `scenarios_minigame.py`, `scenarios_credits.py`,
`scenarios_mapscreen.py` (natives are injected through the script runner, `scenarios_minigame.native`).
Tests: `engine/crates/wf-app/tests/menu_parity.rs` (prefixes `mg_`, `cr_`, `map_`).

## The mini-games of Buu's Fury

Every script native that runs a game mode (annotations/script_natives.json, scripts/events/*.rhai):

| native | game | used by | status |
|---|---|---|---|
| 0xA7 `watch_this_game(level)` | Watch This! (memorise 5 buttons, repeat them) | #13 (`rand(6)`), #501 (`p(2)`), #502 (2) | **exact** (`mg_watch_*`) |
| 0xC6 `run_power_struggle()` | power-struggle gauge (button mash) | #463, #464 (Spirit Bomb vs Kid Buu) | **exact** (`mg_power`) |
| 0xC7 `run_punch_minigame(speed)` | "Press A Button To Punch" gauge | #266 (`500 - rand(250)`) | **exact** (`mg_punch1/3`) |

No other script native opens a mode except menus (save 0x8C, character 0x8D, shops 0xCE/0xCF, credits 0xD1). The
link-cable modes (Dojo fights, Trading Post, wireless) are out of scope.

### Watch This! — mode vt `0x0804A3E8`, game object `0x08028144` (vt `0x0804C9C8`)

* Construction (in the native's frame): five `rand(6)` (World.rng, the game's order) = the buttons (0 A, 1 B,
  2 RIGHT, 3 LEFT, 4 UP, 5 DOWN), period = (8 − level) · 3 frames, the box (`dialog_box_0`) + "Watch This!" (bold font,
  outlined glyphs `0x0803F70C`, colour 2, outline 1, centred at (80, 28)). Its VRAM work makes the first iteration
  2 frames long (`Page::enter_lag`).
* The state function (+0x30) runs in the mode's *update* (before the VBlank wait → it reads the previous
  iteration's input): open zoom (+0x14 = 1..8, `0x0802812C`), the icons one by one every period at (20 + 24i, 44)
  then removed again (`0x08028034`), "Your Turn!" and five presses (`0x08027FB8`: any of A B RIGHT LEFT UP DOWN
  pressed; the button is the lowest pressed bit with SELECT / START folded onto A / B: `b ≥ 2 ? b − 2 : b`), 16
  frames (`0x08027F74`), "Fusion-Ha!" blinking 32 frames (`0x08027EEE`), the close zoom (`0x08027EB0`), verdict
  2 = the sequence or the code UP B LEFT DOWN A (`0x0874EA60`), else 1. The native pushes verdict − 1.
* Draw `0x08028224`: three affine OBJs (64, 64, 32 wide, 64 high) at y 48, PA = PD = 2048 / k, x = (40k − 88(k − 8))
  / 8, (104k − 88(k − 8)) / 8, (168k − 104(k − 8)) / 8 (truncating), OAM one frame late.

### Power struggle — mode vt `0x0804B5D8`, gauge `0x0803018C` (vt `0x0804D5BC`)

Gauge at (72, 144): each iteration any of A B RIGHT LEFT UP DOWN *pressed* → value += 10 (85 = full: done), else
value = value · 31 >> 5; the peak marker follows the maximum. Done → the wait mode `0x08049978` (30 iterations),
then the mode ends; the native pushes nothing.

### Punch — mode vt `0x0804D8F0`, gauge `0x080309F4` (vt `0x0804D20C`), label `0x08030860`

The marker sweeps the full gauge at (72, 128) by `speed · 0x290 >> 8` / 256 px per frame, bouncing at 0 and 85; A
pressed stops it; then it blinks with bit 2 of the boot frame counter (hidden = −1) for the 30-frame wait; the mode
returns marker + 1, the native pushes marker (−1 when it ended on a hidden frame). The counter is the game's boot
frame counter (`0x03000F94`): the remake uses the play time (a harness passes the capture's value).

Gauge drawing `0x08030274` (both): 3 OBJs 32×8 8bpp, priority 1, steps ≥ value of table `0x08757EE8` cleared to
colour 1 in the template `0x08757BE8`; the 8×16 marker OBJ (`0x08757B68`) at (x + peak + 1, y − 3). The label: the
dialog font with outlined glyphs centred in a (width + 33) & ~31 wide strip, OBJs at (120 − w/2, 140).

All three draw over the frozen field (the "field drawn, not updated" mode class): `MenuView.overlay`.

## Staff credits — mode `0x080335BC(0, unskippable)`, vt `0x08049338`

Callers: `run_credits` 0xD1 (unskippable; script #312), the title screen and Options (skippable). See credits.rs.
47 pages of string pairs 1357/1358 .. 1447/1448 after the first (1355/1356): heading in the dialog font on BG1
(y 48), lines in the small font on BG0 (y 80), shadowed glyphs; both BGs alpha-blended over black (BLDCNT 0x2043,
EVB 0), EVA ramps. t starts at 144: A / B pressed (skippable) → quit + sound 38; t < 128: DOWN held or quitting →
t = 127; t = 128: next pair; 129..144 fade out (EVA = 144 − t), at 144 the next page (or the end); 145..160 fade
in, 160 → 0. A page's draw takes 2 frames when `2·width(heading) + width(lines) > 158` (fitted on all 47 pages;
`credits::page_cost`), and its new state shows at once (EVA 0). The enter takes 2 frames.

## Map screen (SELECT) — field main loop `0x08006E3C`

* Open: SELECT with HP ≠ 0 on a 2D map → sound 40, the 16-frame fade, world save, `0x080256F0` (vt `0x0804E384`,
  enter `0x0804EC2C`: BLDALPHA 0, BLDCNT 0x3F5F, setup `0x0802574C`). The setup's CPU time decides when the first
  iteration runs (`mapscreen::setup_frames`: 200k + 25 cycles per tile byte, Snakeway 2 frames, the forest 4).
* Area page: record of table `0x0874CA24` (`0x08004B08`, first record with a room of the current area), the
  rooms into a 128×128 entry buffer (visited: story flag `0x1C0 + bit` → rect2 / map2; with game bit 0x40 the others
  as rect1 / map1), bounds = union (px), scroll limits (x0 − 223, y0 − 143, x1 − 17, y1 − 17), arrow limits
  (x0 − 16, y0 − 16, x1 − 224, y1 − 144). Camera = the player's px position scaled into the current room's rect1
  (`(w·8·px + mapw/2) / mapw + x0·8`) − (120, 80); the player marker (`0x08026EE6`) there − 4. BG2 = the buffer
  scrolled by the camera *before* the iteration's step (the scroll registers are written at the iteration's
  start), BG3 empty (grid on the other pages), BG0 the frame + L / R prompts (`bg0_<l><r>`, pressed = blob tiles
  4..7). OBJs: the marker (prio 3, frames 0 1 2 3 2 1 every 8 updates), the record's markers (`0x08026C9C`, prio 3),
  four arrows (`0x08026B4C`, prio 0, blinking (c >> 2) & 7 ping-pong while that direction can scroll, else frame 3);
  OAM shows the previous update.
* Step `0x08024C54` (see mapscreen.rs): page step (D-pad held: 1 px per frame inside the limits; A closes on the area
  / world page), fade (EVA ±1, EVB 0), L / R page change (BLDCNT 0x3E5E: BG0 stays lit), B / SELECT close.
* Pages: Scouter ←L— Area —R→ World. World page (`0x0802641C`): world picture (`0x08732604`) over the grid,
  script #15's markers (`add_map_marker`, current world blinking); its enter takes 4 frames. Returning to the area
  page uses the base vt `0x0804A4E8` (no BLDCNT reset).
* Close: EVA reaches 0 → the mode ends; 23 black frames (world restore), the field's 16-frame fade-in.

### The Scouter page (`0x080261D4`, vt `0x0804A570`; remake `menu/scouter.rs`)

* Records: message 0x16 over the saved field world (object list last to first; handler `0x080102FC`): actors with a
  database entry (`0x08027948`, table `0x0874D380`, 232 × 16 bytes {name string, description string, def id,
  portrait arg, stats}; def 0x95 counts as 0x92; characters / NPCs: the first entry with the def and stats < 0, else
  the first with the def; enemies: stats == their type) → 40-byte records at obj+0x10 {db, HP (enemies) / −1,
  direction, x, y (position >> 8 − camera `0x030019A4` +0x28 / +0x2C), box, blip}.
* Enter `0x08026212`: DISPCNT 0x1940 (BG0 frame, BG3 grid, OBJ). Per record a blip (`0x08027390`): a plain actor of
  the entry's def, idle — its state enter draws rand(255) (the only RNG use while the map screen is up) — in the
  record's direction; the record box = the idle's first cel record +4..+7 around (x, y). Then the cursor
  (`0x08026F94`, vt `0x0804D5E4`) at (120, 80).
* Update: the selection (`0x0802627E`, vt+0x3C): records whose box holds the cursor point, the nearest box centre
  (squared distance); old blip → priority byte 0x38 (`0x08027448`), new → 0x28 plus the remap controller
  (`0x08027424`, vt `0x0804AE38`: c += 1, remap = `0x0874D360[(c >> 2) & 7]`, 4 distinct tables). OBJ priority =
  byte >> 4. The cursor (`0x08027020`): A pressed → closing, counter 0; A released → opening; counter ±1 within 0..4,
  frame c >> 1 uploaded at even counts; D-pad held ±0x80 to the velocity, pos += vel, vel = vel·3/4 (toward 0),
  clamp x 0x1000..0xE000, y 0x1000..0x9000 (velocity 0). The cursor's OAM and the blips' cels / remaps show the
  previous update.
* Step +0x40 `0x08026360`: A pressed with a selection → BLDCNT 0x3F5F fade, then (`0x0802638A`) the info page.

### The info page (`0x08025E50`, vt `0x0804A52C`, enter `0x08026048`)

* Ctor: the scouted bit (GD +0xC5 + db / 8), description lines (dialog font, 104 px) → max scroll = lines·16 − 56.
* Enter: DISPCNT 0x1C40. BG3 (0x1F83) = the picture `0x08732384` + content `0x08024D94` (small font, shadow 1,
  colour 2: name centred (0x38, 0x90); labels = string entries 190..193 at x 0x70, y 0x16 0x20 0x2A 0x34 (the
  code passes byte offsets 760..772); values right-aligned at 0xA0, `%d` or `??`: characters (stats −2 − c) record
  HP / RecalcStats STR POW END, enemies record HP else type HP, type STR POW END (`0x08711274` +4, +0xD..+0xF);
  portrait (`0x08011F20(arg)`, content index arg − 8) at (168, 8), else entry 194 "No Portrait Data" wrapped to 0x38
  at (0xC8, 0x22)). BG2 (0x1E83) = the description window (15, 11)–(28, 18): the text surface (y 0xE, 16 px lines)
  rows scroll..scroll+56 copied to tiles `0x06009800`.
* The sprite (`0x080272EC`, vt `0x0804CE7C`): idle enter (rand(255)), then vt+0x28(2) → the walk state 0x03001B58
  when def byte 0 > 1 and def+0x58 is set: it ticks the idle's cels (8 per cel, looping) — the picture stays the
  idle's. Update: direction `0x0874D35C[(c >> 7) & 3]` = 0 3 1 2; position from the cel record: x = 0x38 − w/2 − ox,
  y = h > 32 ? 0x38 − h/2 − oy : 0x40. Shown one update late like the blips.
* Arrows (`0x080271D0`, vt `0x0804D60C`): up (+0x24, OAM word `0x0874D248`+8) active while scroll > 0, down (+0x28,
  +4, vflipped) while scroll < max; both use the tiles at +0x14; active: counter += 1, frame = bit 4; else 0 / 0.
* Step `0x08025FAC`: UP / DOWN held ±1 px; B → +0xC (the common step closes; at the fade's end `0x0802601A` builds a
  new Scouter, vt `0x0804E3C8`, new blips / RNG draws); A closes. Enter CPU time 5–6 frames.
* Captures: `map_left`, `map_scouter_move`, `map_scouter_info` (scenario extra `live_field`: the test sets the field
  actors, camera, player direction and RNG as captured).
