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

### Gap: the Scouter page (`0x080261D4`, vt `0x0804A570`)

The page L of the area map shows the live field objects (player, NPCs, enemies: their sprites) on the grid with a
target cursor; the D-pad selects one and A opens its info (portrait, name, description: strings "No Portrait
Data", "Database Empty"). Not reverse-engineered: the remake shows a stand-in (frame without L, the grid, the fades,
R back to the area page). Captured in `map_left` (frames 79..150 differ).
