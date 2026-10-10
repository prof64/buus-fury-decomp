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

**Link play screens.** The Dojo's pages are the game's own, in wf-core (`menu::link::dojo` / `chars`, [`Link`]
pages `LinkPage::Setup` / `Chars` and the nested wait box `Link::wait`), layouts measured and gated pixel for pixel
against two linked emulators (`re/buu/menu/scenarios_link.py`, `link_setup*`, `link_guest_wait`, `link_chars*`, all
exact; the guest's screen recorded with `subject: 'b'`): Select Saved Game's A sends the host (player 1) to the
setup page - "Dojo" (95), "Character Mode:" (99) over the BALANCED MODE / IN-GAME MODE boxes (97 / 98, green =
chosen), "Rounds:" (100) over 1 / 3 / 5 (101..103), START FIGHT (96) where the cursor starts, A Select / B Cancel; the
arrows move, A chooses (no sound), B goes back to Select Saved Game (no sound) - and the guest into "Waiting for a
friend..." (90) over its list; START FIGHT runs the same box over the setup page; the boxes end 11 steps after the
friend's answer (`Friend::answered`) and both go on to the character pick: "Battle with your friends in the Dojo!"
(104), the selected character's name (Balanced: the 32 of strings 116..147 - Piccolo, Broly, Majin Buu, Super Buu
1..3, Kid Buu and Uub too; In-Game: the picked save's characters with flag bits 0 and 1, names 158 + id), the
figures in a row of five with the selected one walking in a frame (BG0), LEFT / RIGHT prompts (auto-repeat, sound
39), "Choose your character!" (105); A (sound 40) runs the pick's wait box (31 steps after the friend's pick), then
the 16-step fade to the fight; B (sound 40) goes back to the setup page (host) / Select Saved Game (guest). The
match's banners and the prize (106..115) and "Disconnected" (93) are still `wf_app::link`'s own drawing.
engine/docs/netcode.md "Link play: playing it".

The **Trading Post's pages are the game's own, in wf-core** (`menu::link::trade`, a [`Link`] page
`LinkPage::Trade`), gated pixel for pixel against two linked emulators (`re/buu/menu/scenarios_trade.py`, the
`link_trade_*` scenarios, all exact): the picker (79 "Trading Post": the list of equipment, tradable supplies and
"Zenie (n)" (82), scrolling 6 rows with its UP / DOWN prompts; "Your Trade:" (80) with up to 5 entries; A Select /
B Cancel / R "Propose Trade" (84); RIGHT / LEFT between the two columns), "How many to trade?" (83; held LEFT /
RIGHT speed up by 10 and 100), "Waiting for a friend..." (90) after Propose Trade and after an answer, the proposal
("Your Trade:" / "Friend's Trade:" (81), ACCEPT / DECLINE (85 / 86) boxes with the cursor), "TRADE" "COMPLETE!"
(88 / 89) for 61 updates, then the title's Multiplayer page; B on the picker goes back to Select Saved Game.
"You have accepted trade" (87) is not used by the game. The friend's side comes in as `link::Friend` (`offer`,
`answer`), our moves go out as `LinkAction`s (Propose, CancelPropose, Answer, CancelAnswer, LeaveProposal,
Complete, Unpick); the harness feeds a trace row's `link` (`link::harness`).

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
`skills_*`, `cr_*` (credits), `mg_*` (mini-games: power struggle, punch gauge, Watch This!; the native's push is
checked too), `map_snakeway` / `map_forest` (map screen: area page, scrolling, record markers, revealed rooms, world
page with markers, page changes, close) — every compared frame and every sound exact; credits, mini-games and the
map screen are also exact with `MENU_PARITY_FREE=1` (their own frame-cost models). Also strict: `shop_*`, `save_*`,
`title_*` (title, Single Player, Select Game, New Game into the prologue, Continue into the field, Delete Game) and
`charsel_*` (Switch Character: figures, silhouettes, walk / idle timing) — `re/buu/menu/NOTES_screens.md`.
Also strict: `map_left`, `map_scouter_move`, `map_scouter_info` (the Scouter page and its info page; exact with the
oracle, near-exact free: the info page's setup takes 5 or 6 frames by the frame phase). The world restore's length after a screen (CPU time:
24 or 25 black frames) is taken from the trace too.

## 4b. Integration

* **Script natives** (`wf-script-core/src/natives.rs`): 0x8C `save::open_save`, 0xA7 / 0xC6 / 0xC7
  `minigame::open_watch` / `open_power` / `open_punch`, 0xCE / 0xCF `shop::open_shop_buy` / `open_shop_sell`, 0xD1
  `credits::open_credits`; the script blocks (`Pending::Menu`) until `menu::take_result` (after the fade-in) and
  pushes `result − 1` where the game does. The screens apply their effects (purchases, saves) themselves. Without
  `ui/` content the old models in `scene.rs` still run. 0x8D `charsel::open_charsel` (no player object = nothing):
  result 2 = chosen (`Story.current` set; the poll clears the timed buff and rebuilds the leader with
  `natives::switch_character`, as `0x080027DE` does), 1 = cancelled; nothing pushed.
* **Title / game over** (`menu/title.rs`, `menu/gameover.rs`): 0x8E queues `Request::ReturnToTitle`; the session
  replaces the world with `title::title_world` next frame (save slots + options kept, song 45). The leader's KO
  runs the engine hook script #0 at KO + 199 (`gameover::tick`: Lazarus Crystal revive, else "You have died!" →
  0x8E); Buu's Fury has no game-over screen. New Game / Continue on the title build the field world
  (`title::boot`). Launcher: `launcher::title_app` (slot files in), `launcher::saves_of` (slot files out when the
  generation changes); slots live in `World.saves` (snapshot state, postcard bytes per slot).
* **Equipment model**: `Story.equipment` holds every owned copy; wearers come from the characters' slots
  (`equip::marks`), unequipped copies = unmarked entries (shop sell, take_equipment).
* **Options**: `World.menu.settings` (text speed, SFX / music volume, raw 0..8, global like the game's `0x03001914`);
  a slider change emits `Event::MasterVolumes` → `AudioEvent::SetMasterVolumes`.

## 5. Gaps

* The dialogue typewriter does not read `menu::options::settings(w).text_speed` yet (fixed default speed); the
  frontend should also send the stored volumes at session start (only changes are evented).
* **Note for the shop / save / title / charsel captures (fork C):** `capture.py`'s story dict now also records the
  field place: `area` [world, area, variant], `player` [x, y, +0x11] (the test starts the remake's field there:
  `cfg.world/area/variant/pos`, which changes the field frames of recaptured traces), and `facing` (+0x10), `objs`
  (the other actors [def id, x, y, direction]), `cam`, `rng`. The last four are applied only when the scenario's
  extra has `live_field: True` (`meta.live_field`: actors / camera / facing before frame 0, the RNG after frame 0) —
  used by the Scouter scenarios; opt in if a screen shows or draws from the live field.
* Map screen / credits / Watch This! setup CPU time is a model (`mapscreen::setup_frames`: 200k + 25 cycles per
  decompressed tile byte; `credits::page_cost`); exact on the captured areas / all 47 credit pages.
* Not frame-checked against a capture: the field after Switch Character (leader rebuild), the death hook's start
  frame and the return to the title. The title's Multiplayer item and its OPTIONS page are not reproduced.

* Redraw CPU time is modelled per screen (frame counts), not per cycle: the sound driver's IRQ load is not
  modelled, so a long redraw can take one frame more or less than on the GBA and the picture of a frame where the
  copy races the scanout is not reproduced.
