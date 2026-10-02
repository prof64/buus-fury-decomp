# Legacy of Goku X

Reverse engineering of the three Webfoot DBZ GBA action RPGs (The Legacy of
Goku, The Legacy of Goku II, Buu's Fury), a local dashboard of everything
learned, and a browser battle simulator built on the real mechanics.

ROMs are not in the repo. Put them in `roms/` as `log1.gba`, `log2.gba`, `buu.gba`.
Everything derived from them (images, emulator save states) is gitignored and rebuilt locally.

## Setup (fresh checkout)

Needs Python 3, Node 20+, and (macOS) Homebrew `ninja libpng freetype xz` for the emulator build.

    python3 -m venv .venv && .venv/bin/pip install capstone numpy pillow cmake
    sh tools/emu/build.sh                      # builds mGBA + the headless emulator shim
    .venv/bin/python tools/extract/extract_all.py   # all images for all three games (slow: maps are rendered in the emulator)
    cd site && npm install && npm run dev      # http://localhost:5317

`extract_all.py` writes `site/public/assets/<game>/` (sprites, effects, portraits, items, UI, maps) and
`data/<game>/sprites.json`, then the battle HUD icons (`tools/battle/extract_icons.py`, `log1_icons.py`,
`extract_buu_icons.py`), and finishes with `tools/check_assets.py`, which scans `data/` and `site/src/` for every
referenced asset path and fails if one is missing. Options: `extract_all.py log2` (one game), `--skip-maps`
(keep the already-rendered maps), `--no-icons`. Run `python3 tools/check_assets.py` any time to re-check.

Site commands (in `site/`): `npm run dev`, `npm test` (vitest), `npx tsc -b`, `npx vite build`.

## What's where

| path | what |
|---|---|
| `re/<game>/` | reverse-engineering scripts and the findings: `notes.md` per game (mechanics, ROM addresses, evidence), extractors that turn the ROM into `data/<game>/*.json`, emulator verification scripts |
| `data/<game>/` | the extracted, committed game data the site reads: characters, moves, enemies, AI, items, levels, formulas, maps, `sprites.json` |
| `data/battle/` | emulator-recorded test vectors (damage rolls, level-ups, move traces) and decoded tables used by the battle simulator |
| `tools/emu/` | `emu.py`, a scriptable headless mGBA (frames, input, memory, screenshots, save states, watchpoints/breakpoints) |
| `tools/extract/` | graphics extraction (`extract_all.py`; formats documented in `GRAPHICS.md`) |
| `tools/battle/` | battle-simulator helpers: HUD icon extraction, vector recorders, `play.py` (drives the running page) |
| `site/` | the Vite + React + TS app: dashboard (`src/pages/`) and battle simulator (`src/battle/`, see `src/battle/RULESET.md`) |
| `work/` | local scratch (save states, disassembly listings, screenshots); gitignored |

The dashboard (`#/g/<game>/...`) has per-game tabs: overview, sprites (animation player), maps, characters,
enemies (+ AI), moves, formulas (calculators), move lab, items, notes, raw data; plus `#/compare` (how the
mechanics changed across the three games).

## Battle simulator (`#/battle`)

Pick a game (the last one played is remembered; `#/battle?game=log1|log2|buu` picks one), a fighter, and either
any enemy / boss or a story preset. Keyboard defaults are mGBA's (rebind them under "Change key bindings" on the
setup screen or the ⌨ Keys toolbar button; Gamepad API also works):

| GBA button | key |
|---|---|
| D-pad | arrows |
| A | X |
| B | Z |
| L | A |
| R | S |
| START (pause) | Enter |
| SELECT | Backspace |

Hotkeys: F1 debug panel, F2 hitboxes, F3 state/AI labels, `.` frame step while paused, Esc back to setup.

**The Legacy of Goku** — D-pad: walk 1 px/frame, one direction at a time (no diagonals). A: punch / pick up an item.
B: selected ki move (tap = Ki Blast, hold to charge). L: cycle ki move. R: take off / land (flight energy drains
while moving). SELECT: use a herb / Senzu bean.

**The Legacy of Goku II** — D-pad: move (8-way), double-tap: dash. A: punch; keep holding after a punch and release
when charged: special melee. B: selected special (hold to charge / sustain beams, tap again to rapid-fire ki
blasts). L: cycle special.

**Buu's Fury** — D-pad: move (8-way), double-tap within 20 frames: dash. A: melee (+1 EP per damaging hit). B:
selected skill (hold to charge / sustain beams; release to fire Big Bang / Masenko / Sword). R: block (1/16 damage
from the front); R+B: energy shield (drains EP). L: cycle skill.
