# Buu's Fury decomp

A playable Rust recreation of *Dragon Ball Z: Buu's Fury* for the Game Boy Advance. This repository is the reference
implementation and includes reverse-engineering research on all three Webfoot Dragon Ball Z GBA games. Legacy of
Goku X is a separate game that develops its own snapshot of this engine.

## How it was built

This was a human-directed, AI-assisted reverse-engineering project. Claude Opus 5.5 contributed to the research,
implementation, and tests. We read the original game's disassembly and used a scriptable build of mGBA to step
through the game, record input and game state, and trace memory reads and writes with breakpoints and watchpoints.
The [research notes](re/buu/notes.md) record ROM addresses, observations, and the scripts used to check them.

The Rust engine implements the resulting rules. We tested changes by replaying the same inputs in the original game
and the recreation, comparing state, sound events, and selected rendered frames. Short recordings isolate particular
behaviors, while whole-game recordings catch changes that only show up after a long sequence of play.

## Current state

- **Playable.** The Buu's Fury campaign runs from the title screen through the credits. The engine also includes
  battles, saves, replays, mods, and a multiplayer lobby. See the [engine quickstart](engine/README.md) for the
  macOS and web builds.
- **Measured accuracy.** The [whole-run checker](tools/wholerun/check.sh) compares three recorded playthroughs
  on game-logic ticks in its default mode. It checks area, position, random-number state, story flags, script
  depth, and audio events. Each run has a documented exception for Vegeta's movement inside Buu, where the
  original reads bytes left over from freed objects. The checker skips that interval and resumes from the
  original game's recorded state. The November 3, 2026 check passed all six compared legs, covering 5.88
  million logic iterations outside those exceptions.
- **Known differences.** Some visual details, including scanline-level HUD effects and world-map drawing, differ.
  The [replay and coverage notes](engine/docs/segments.md) and [accuracy log](engine/docs/accuracy-log.md) describe
  the evidence and remaining differences. The replays cover specific paths and inputs.

To play, bring a Buu's Fury ROM. The engine extracts its game content locally. The repository contains decoded
research tables, recorded test inputs, and test fixtures. ROMs, generated graphics and audio for the research site,
and local emulator saves are kept outside Git.

## Research dashboard setup

Needs Python 3, Node 20+, (macOS) Homebrew `ninja libpng freetype xz` for the emulator build, and `ffmpeg` (with
libmp3lame) for the Buu's Fury audio rip. Put your ROMs in `roms/` as `log1.gba`, `log2.gba`, and `buu.gba`.

    python3 -m venv .venv && .venv/bin/pip install capstone numpy pillow cmake
    sh tools/emu/build.sh                      # builds mGBA + the headless emulator shim
    .venv/bin/python tools/extract/extract_all.py   # all images for all three games (slow: maps are rendered in the emulator)
    cd site && npm install && npm run dev      # http://localhost:5317

`extract_all.py` writes `site/public/assets/<game>/` (sprites, effects, portraits, items, UI, maps) and
`data/<game>/sprites.json`, then the battle HUD icons (`tools/battle/extract_icons.py`, `log1_icons.py`,
`extract_buu_icons.py`), the Buu's Fury music + sound effects (`tools/audio/rip_buu.py` → `site/public/assets/buu/audio/`,
`data/buu/audio.json`; see `tools/audio/AUDIO.md`), and finishes with `tools/check_assets.py`, which scans `data/` and `site/src/` for every
referenced asset path and fails if one is missing. Options: `extract_all.py log2` (one game), `--skip-maps`
(keep the already-rendered maps), `--no-icons`, `--no-audio`. Run `python3 tools/check_assets.py` any time to re-check.

Site commands (in `site/`): `npm run dev`, `npm test` (vitest), `npx tsc -b`, `npx vite build`.

## What's where

| path | what |
|---|---|
| `re/<game>/` | reverse-engineering scripts and the findings: `notes.md` per game (mechanics, ROM addresses, evidence), extractors that turn the ROM into `data/<game>/*.json`, emulator verification scripts |
| `data/<game>/` | the extracted, committed game data the site reads: characters, moves, enemies, AI, items, levels, formulas, maps, `sprites.json` |
| `data/battle/` | emulator-recorded test vectors (damage rolls, level-ups, move traces) and decoded tables used by the battle simulator |
| `tools/emu/` | `emu.py`, a scriptable headless mGBA (frames, input, memory, screenshots, save states, watchpoints/breakpoints) |
| `tools/extract/` | graphics extraction (`extract_all.py`; formats documented in `GRAPHICS.md`) |
| `tools/audio/` | Buu's Fury sound driver notes (`AUDIO.md`) and the music / SFX ripper (`rip_buu.py`) |
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
