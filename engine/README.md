# Buu's Fury engine — quickstart

A from-scratch Rust engine for **Dragon Ball Z: Buu's Fury** (GBA). It ships **no game data**: you bring your own
ROM (USA release, `BG3E`, SHA-1 `e65738e9d67688309f09811a54f495523ec9aada`). The ROM is converted on your machine
on first launch and never uploaded.

Two ways to play: a **macOS app** and a **static web build**. Both open on the same launcher.

## macOS app

```sh
cd engine
sh package/mac-app.sh          # or: just app      -> dist/Buu's Fury.app
```

Needs Rust (stable) and the Xcode command line tools (`sips`, `iconutil`, `codesign`); Python 3 with Pillow draws
the icon. The app is **unsigned** (ad-hoc signature only), so macOS blocks the first double-click:

* right-click **Buu's Fury.app → Open → Open** (once), or
* `xattr -dr com.apple.quarantine "dist/Buu's Fury.app"` and open it normally.

First launch: drop your `.gba` on the window (or press **O** for a file picker). The game content is extracted into
`~/Library/Application Support/wf-buu/` (a few seconds) and kept there with your ROM, so later launches start
immediately and engine updates re-extract by themselves.

| what | where |
|---|---|
| extracted content | `~/Library/Application Support/wf-buu/content/base/` |
| mods | `~/Library/Application Support/wf-buu/content/mods/<mod>/` (launcher → Mods → Open mods folder) |
| story saves | `.../wf-buu/saves/slot1.sav` … `slot3.sav` |
| settings, controls, hotkeys | `.../wf-buu/settings.json`, `bindings.json`, `hotkeys.json` |
| replays, save states | `.../wf-buu/replays/` |

From source without bundling: `cargo run --release -p wf-native` (same launcher; `-- --help` lists the flags, e.g.
`--story`, `--fighter goku --enemy t60`, `--online`, `--mod DIR`).

## Web

```sh
cd engine
sh package/web-dist.sh         # or: just web      -> web/dist/
python3 package/serve.py       # or: just serve    -> http://localhost:8370/app/
```

Needs the `wasm32-unknown-unknown` Rust target (`rustup target add wasm32-unknown-unknown`). `web/dist/` is plain
static files: put it on any static host. No special headers (no COOP/COEP: nothing uses `SharedArrayBuffer`); it
needs a secure context (`https://` or `http://localhost`) and `.js` served as JavaScript. The ROM, the extracted
content, mods, saves and replays stay in the browser (OPFS / IndexedDB / localStorage of that site).

## The launcher

Arrows / Enter / Esc (or the game's A = **X**, B = **Z**; gamepad d-pad, A, B):

* **Story** — *Title screen* hands over to the game's own title (START → Single Player → New Game with the
  prologue, or Continue / Delete Game on the three save slots); or continue a slot / quick-start at Snakeway
  directly. The game saves at its own save points.
* **Battle** — any fighter, form and level (50–200) against any enemy or boss (bosses first; L/R or PageUp/PageDown
  jump 10), how many, or a story preset (Kid Buu, Janemba → Super Janemba, Pui Pui → Yakon → Dabura, …); any
  arena (or automatic); 1–4 local players (gamepads).
* **Online** — the in-game lobby: host a room or join one by code (see `docs/netcode.md` for the signaling server).
* **Replays** — every fight is recorded; play, ghost, verify, keep.
* **Mods** — turn installed mods on / off; open the mods folder (app) or add a mod folder (web). See
  `docs/modding.md` and `mods-examples/`.
* **Settings** — view (PC zoomed-out view at auto / 1–6x, or classic 240×160), volume, fullscreen (app), controls.

In a game: **Shift+Esc** returns to the launcher. Esc pauses (web: settings panel), F1 PC / classic view, F2 zoom,
F5 controls (app), F10 replays, F11 fullscreen (app), F12 online lobby (app). Hold **`** to rewind, Tab to fast
forward; Shift+F6–F9 save state, F6–F9 load. Default keys are mGBA's: arrows, X = A, Z = B, A = L, S = R,
Enter = Start, Backspace = Select.

More: `docs/frontend.md` (shells), `docs/replays.md`, `docs/netcode.md`, `docs/modding.md`, `docs/menus.md`.
