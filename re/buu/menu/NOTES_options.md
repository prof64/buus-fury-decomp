# Options, Journal, Skills (fork B)

Code: `engine/crates/wf-core/src/menu/{options,journal,skills}.rs`; extractor `engine/crates/wf-ui/src/menupics/opts.rs`
(`ui/menu/opts.json`, `ui/menu/skill_icon_NNN.png`); scenarios `scenarios_{options,journal,skills}.py`.

## Journal — ctor `0x08014188` (0x574 B), vt `0x080498C4`

* Built when the page is created: for each of the 40 quests (`0x08720564`, 16 B: u8 star, u16 string, scripts
  hidden / completed / goal) the VM `0x08004038` runs the scripts: hidden ≠ 0 → skipped; completed → Completed
  Goals (+0x1D0); else goal → Goals (+0x12C). All 120 scripts only use `get_flag` / `and` (core's
  `story::eval_condition` runs them; bytecode from `scripts/bytecode.json`).
* Children: B (code 1), "Cancel", L (6), R (7), "Options" / "Status" labels, UP (code 2) / DOWN (3) prompts at
  (212, 40) / (212, 112), two tab headers (`0x08015C62`, vt `0x080493F4`: box `0x08015BB8` + small font text,
  "Goals" at (24, 32), "Completed Goals" after it).
* Redraw `0x08014526`: frame "Journal", list `0x08014424` (empty: prompts hidden, nothing else; else prompts
  visible, UP enabled = scroll ≠ 0, DOWN = count > scroll + 3, separator at x 219, 3 rows: lines =
  `line_fit 0x0803DE6A` calls at 176 px, y = 0x43 + 28k − 7·lines, x 0x20, word-wrapped draw `0x0803DD66`, line
  height 14, colour 2 shadow 1; star 16x16 at (14, 28k + 49)), then the children.
* Events: UP/DOWN scroll ±1 (sfx 39, no bound check: the prompts are disabled at the ends), B close, L Options,
  R Status. Done check `0x080146B6`: START closes (no exit stub), LEFT / RIGHT switch the list (sfx 40, scroll 0).

## Options — vt `0x080498F0` (0x120 B, built by Journal's L / Equip's R)

* Options block `0x03001914`: [0] text speed, [1] SFX volume, [2] music volume, [3] 0; loaded at boot
  (`0x08006B10`, defaults 4 / 6 / 4), saved to the cartridge (checksum + 0x12345678) by the page's dtor
  `0x08014A9C` when changed. Text speed is read by the dialogue module (literal `0x08071728`).
* Sliders `0x08006880` (vt `0x0804BF3C`): value via the page's interface at +0x114 (vt `0x08049928`: get / set /
  max 8 / min 0 → page slots 11-13); input `0x080069E6` (selected only: LEFT if > 0, else RIGHT if < 8: sfx 39,
  set = applies the volume at once `0x0803E42A` / `0x0803F18C`, redraw); draw `0x080068B8` (fill = v·w / 8; 5
  rows: edges 1 / 0x73 (empty), 0x0A / 0x68 (filled), bar 0x70 / 3).
* Controls `0x080060A8` (string by index): Sound FX / Music / Text Speed (UP / DOWN cycle), Credits ↔ Sleep.
  The A prompt is enabled only when Credits / Sleep is selected (set before the children are drawn).
* A on Credits: `0x080335BC` credits mode run nested, then `0x08015882` clears VRAM / palettes (black until the
  page's redraw lands). A on Sleep: `0x0803D876(1)` + a deferred script object (vt `0x08049264`, event header
  `0x0877CCC4` = s007: "@To awaken from Sleep Mode, hold down the L Button, the R Button, and SELECT…",
  `sleep_mode`, "@Sleep Mode ended.") queued in GameData +0x610: runs in the field after the menu.

## Skills — ctor `0x08017160(0, char)` (0x12C B), vt `0x0804B024`

* Fields: +0x114 char, +0x118 count (record +0xC), +0x11C first, +0x120 selected. Children: A, "Select", B,
  "Cancel", (count > 2) UP (disabled) / DOWN prompts, L, R, labels, cursor (blink; cursor point `0x0801765E`).
* Redraw `0x08017350`: frame titled "Status" **with the children first** (cursor at the old first row), then
  first row = max(first, (sel & ~1) − 4) (quirk: −4, a 5th skill can leave the view), ≤ sel, ≥ 0; then the prompts'
  enabled flags (shown at the next redraw); separators (x 219 vertical, two horizontal `0x08005154`); per skill
  (k < 4): name word-wrapped 96 px centred (line height 12; colour 6 selected), icon (`0x08029138`: frame record
  `0x0874F058` + the skill's tiled icon window) at (x − 12, y − 8) after the name, "Level %d" (chosen level + 1).
* Events: A popup, B / L / R → `0x0803D876(1 / 2 / 3)`, UP / DOWN prompts sel ∓ 2. Done check: START → 4
  (run_mode returns at once), LEFT / RIGHT sel ^= 1 (sfx 39).
* Popup vt `0x0804AFF8`: BG0 (screenblock 31, tiles from 601, priority 3 over BG2) 208x104 at (16, 40): frame
  `0x08005096` (black borders, rows, panel), icon at (14, 6), name colour 5 at (40, 15), description colour 0x75
  wrapped 180 px from (14, 27); levelled skills: LEFT / RIGHT prompts (codes 4 / 5: chosen level −1 / +1, no
  sound) enabled when the chosen level ≠ 0 / ≠ the learned level, "%d" centred at (169, 88) colour 3. B closes
  (exit stub iteration; BG0 off at once).
* Record +0x2E + 2i = chosen level, +0x2F + 2i = learned level (`set_skill_level` sets both, `get_skill_level`
  reads +0x2F). Remake: `CharState.skill_levels` = learned, new `CharState.skill_cur` = chosen.

## Mode-loop details found here (mod.rs)

* `0x0803D876(r)` swaps in an exit stub: one more iteration (input read, no draw) before `run_mode` returns —
  `Menu.exit_stub`. A mode whose done check returns non-zero ends at once (`Action::Done` / `PopDone`).
* Some pages apply draw-time side effects after their children are drawn (Skills' prompts): `Page::after_draw`.
