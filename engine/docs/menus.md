# Menu screens (core `wf-core::menu`, view `wf_view::MenuView`, renderer `wf_render::menu`)

The pause menu and its pages, shops, save / load, title, character switch, game over, credits, mini-games and the
map screen run **inside the deterministic simulation**: `World.menu` (plain data: hashed, cloned, postcard-
snapshotted with the world), stepped by `Session` instead of the world while a screen is up, driven only by the
leader's input. Replays, rollback and netplay therefore cover menus exactly like the field. RE notes and tools:
`re/buu/menu/NOTES*.md`.

## 1. How the game runs a screen, and the remake's model

Each screen is a *mode* (`run_mode 0x0803D8A0`): `update` (redraw the 8bpp 240x160 surface if dirty, copy it to
VRAM) → VBlank wait → input read (edges since the previous read) → `step`. Most screens are trees of widgets
(`menu/kit.rs`: container, button prompts, labels, selectable text controls, tab bars, cursor) — ported class by
class, children order included (it decides who sees a key first).

| game | remake |
|---|---|
| mode stack (`0x0803D850` replace, `0x0803D876` end with a result, nested `run_mode`) | `Menu.stack: Vec<Screen>`, `Action::{Replace, Push, Pop(r), Close}`, `Page::child_result` |
| world save / fade-out / fade-in around a screen | `Phase::{FadeOut, Enter, Run, Leave, FadeIn}` with a per-screen `Transition` (`menu::PAUSE` = 16-step fades, 5 / 25 black frames) |
| the surface redrawn whole when the container's drawn flag is clear | `Page::view` builds the display list (`MenuOp`: background rows, panel, text lines, pictures, pixels) from the state; `Menu.shown` = the state at the last copy |
| CPU time: a redraw spans several frames, input is not read meanwhile | `Menu.lag` (frames left), `Page::redraw_cost`; the new picture shows when the redraw ends |
| input `0x03003300` (+2 held, +4 pressed, +6 released between reads) | `Menu.{held, pressed, released}`, updated on logic frames only |

`wf-render` rasterises the display list with the game's routines (`0x08004F1C` rows, `0x08004F50` panel,
`0x0803F444` / `0x0803F5A4` glyphs with the (+1, +1) shadow, `0x0803F922` blits keyed on index 0) into 8bpp
surfaces, then composes BG layers / OBJs through the UI palette (`0x0805652C`) — the same pipeline as the GBA, so
every picture is exact by construction once the display list is. Pictures exported with a compact palette
(portraits, icons) are mapped back to the game's raw indices by colour (the UI palette's colours are unique except
black).

## 2. Screens

| screen | module | entry | status |
|---|---|---|---|
| Status (+ stat points, party tabs) | `status.rs` | START in the field | exact (§4) |
| Inventory, Equip, Options, Journal, Skills | `inventory.rs`, `equip.rs`, `options.rs`, `journal.rs`, `skills.rs` | L / R, DOWN | see §4 |
| Shop buy / sell, Save, Title, Switch Character, Game over | `shop.rs`, `save.rs`, `title.rs`, `charsel.rs`, `gameover.rs` | natives 0xCE / 0xCF / 0x8C / 0x8E / 0x8D, HP 0 | see §4 |
| Credits, mini-games, map screen | `credits.rs`, `minigame.rs`, `mapscreen.rs` | 0xD1 / Options, 0xA7 / 0xC6 / 0xC7, SELECT | see §4 |

## 3. Netplay and multiplayer

The GBA game has one controller and one menu. The remake keeps that: a menu belongs to the **leader** (player
slot 0): only its input is read while a screen is up, the world pauses for every player, every peer simulates and
shows the same screen (the state is in `World`, so rollback re-simulates menu frames like any other). Partners'
START / SELECT do nothing (a per-player pause would let one player's world time diverge from another's in a shared
world). Story state the menus change (stats, items, equipment, party) is shared, as in co-op generally.

## 4. Verification (`crates/wf-app/tests/menu_parity.rs`)

Traces `data/menu/<scenario>.json` (recorder `re/buu/menu/capture.py`; scenarios `re/buu/menu/scenarios_*.py`):
per video frame the keys, the mode loop's frame counter, sound calls and the sha1 of the emulator's screen, plus
the GameData block decoded at frame 0. The remake's story session gets that state and the same keys; per frame the
sounds and the picture hash must match. **CPU-time oracle**: how many frames a redraw takes depends on the work
(~0.94-0.98 M cycles for a Status redraw) *and* on the sound driver's IRQ load (15 IRQs a frame, ±30 k cycles per
redraw), so the test takes the logic frames (counter moves) and the frames a new picture appears from the trace;
frames where the VRAM copy raced the scanout (a picture differing from both neighbours) are counted, not compared.
Everything else — logic, input edges, events, sounds, every pixel — is compared exactly. `MENU_PARITY_FREE=1`
runs the remake's own frame-cost model instead (reported as agreement).

Strict (must be exact, `STRICT` in the test): `status_*`, `inventory_*`, `equip_*`, `options_*`, `journal_*`,
`skills_*`, `cr_*` (credits), `mg_*` (mini-games) — every compared frame and every sound exact. The shop / save /
title / character-switch / game-over and map-screen scenarios are reported but not strict until their screens are
finished (`charsel_*`, `map_*` open no screen yet; `shop_*` / `save_*` differ only on their first black frame,
`title_*` on the continue / new-game hand-over). The world restore's length after a screen (CPU time: 24 or 25
black frames) is taken from the trace too.

## 4b. Integration

* **Script natives** (`wf-script-core/src/natives.rs`): 0x8C `save::open_save`, 0xA7 / 0xC6 / 0xC7
  `minigame::open_watch` / `open_power` / `open_punch`, 0xCE / 0xCF `shop::open_shop_buy` / `open_shop_sell`, 0xD1
  `credits::open_credits`; the script blocks (`Pending::Menu`) until `menu::take_result` (after the fade-in) and
  pushes `result − 1` where the game does. The screens apply their effects (purchases, saves) themselves. Without
  `ui/` content the old models in `scene.rs` still run. 0x8D (character switch) still uses the scene model.
* **Equipment model**: `Story.equipment` holds every owned copy; wearers come from the characters' slots
  (`equip::marks`), unequipped copies = unmarked entries (shop sell, take_equipment).
* **Options**: `World.menu.settings` (text speed, SFX / music volume, raw 0..8, global like the game's `0x03001914`);
  a slider change emits `Event::MasterVolumes` → `AudioEvent::SetMasterVolumes`.

## 5. Gaps

* The dialogue typewriter does not read `menu::options::settings(w).text_speed` yet (fixed default speed); the
  frontend should also send the stored volumes at session start (only changes are evented).
* Character switch (0x8D), game over, the map screen and the title hand-over are being finished (see the per-area
  notes `re/buu/menu/NOTES_*.md`).

* Redraw CPU time is modelled per screen (frame counts), not per cycle: the sound driver's IRQ load is not
  modelled, so a long redraw can take one frame more or less than on the GBA and the picture of a frame where the
  copy races the scanout is not reproduced.
