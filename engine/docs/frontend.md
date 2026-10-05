# Frontend: browser + native shell around a deterministic simulation

```
wf-view     the seam (no deps): Sim trait, inputs (GBA button bits), RenderView, SimEvent / AudioEvent
wf-content  runtime content: base pack + mods (override / *.patch.json merge patch / *.delete), typed loaders
            (sprites + anim timing, Tiled maps + tilesets + GBA tile animations, collision, SoundBank from audio/*)
wf-render   software compositor with GBA semantics: RenderView + content -> ARGB framebuffer of any size
wf-app      portable core: App (fixed-step clock, per-player viewports), input bindings, AudioEngine (wf-audio
            driver -> resampler), Session (headless), the demo world
wf-native   desktop shell `buus-fury`: winit + softbuffer window, cpal audio, gilrs pads, rfd file dialog
wf-web      browser shell (cdylib, plain extern "C"): game instance (main thread) + audio instance (AudioWorklet)
web/app/    index.html, app.js (ROM flow, loop, input, settings), extract-worker.js, audio-worklet.js, store.js
```

## 1. Tech choice

**CPU compositor + thin presenters**, not Bevy, not a GPU scene graph.

* The game is 8x8 tiles + OBJs at integer positions. `wf-render` composes a frame the way the GBA PPU does — BG
  priorities, OBJ above BG of equal priority, OAM order, top-two-pixel alpha blending with the map's BLDCNT/EVA/EVB,
  semi-transparent OBJs, per-layer scroll rates/offsets, wrapping overlay layers with drift, tile animations with the
  ROM's frame durations, BLDY-style fade. Being software it is pixel-exact *by construction* and identical on every
  platform (tests hash framebuffers).
* It is cheap: 0.4–0.7 ms native / 1.4–2.9 ms wasm for a 426x240–480x320 zoomed-out view.
* Presenting is trivial and portable: the web shell `putImageData`s the RGBA buffer into a canvas of view size and CSS-
  scales it by an integer (pixelated); the native shell blits it nearest-neighbour into a `softbuffer` surface.
  No GPU API, shader or driver differences; wasm is ~0.5 MB.
* If a GPU path is ever wanted (CRT shaders, 4K at 1x), it replaces only the final upload/scale; composition stays.

Rejected: macroquad/miniquad (own JS loader fights the ROM worker / OPFS / AudioWorklet glue, sprites at float
positions), wgpu (multi-MB wasm, wasm-bindgen tooling, nothing gained for 2D integer art), Bevy (ECS would own the
state — conflicts with the deterministic core owning it).

Audio: the exact `wf-audio` driver runs on the audio thread (cpal callback / AudioWorklet), updated once per video
frame (≈267.5 samples at 15 978.3 Hz) like the game, so SFX start on update boundaries and the 16-voice drop rule
is the driver's own. Output is resampled to the device rate (linear by default; sample-and-hold option).

## 2. The seam (`wf-view`)

```rust
trait Sim {
    fn step(&mut self, inputs: &[u16 /* GBA KEYINPUT bits per player */], events: &mut Vec<SimEvent>);
    fn view(&self, out: &mut RenderView);           // &self: rendering can never mutate the state
    fn frame(&self) -> u64;
    fn state_hash(&self) -> u64 { 0 }               // desync checks
    fn snapshot(&self) -> Option<Vec<u8>> { None }  // save states / rewind / rollback
    fn restore(&mut self, s: &[u8]) -> bool { false }
    fn players(&self) -> usize { 1 }
}
RenderView { frame, map: Option<MapView{id, anim_frame, shake}>, sprites: Vec<SpriteDraw>,
             cameras: Vec<CameraView{x, y}> /* one 240x160 logical camera per player */, hud: Vec<HudView>, fade_black }
SpriteDraw { entity, sheet: "sprites/<id>" | "effects/<id>", frame: Frame{index, flip} | Anim{anim, dir, t, looping},
             x, y /* entity position = anchor */, priority 0..3, mirror, effect: None|White|SemiTransparent|Blend }
SimEvent::Audio(PlaySong | StopSong | PlaySfx{id, volume, pan} | StopSfx | StopAllSfx | SetMasterVolumes)
```

* **wf-core** implements `Sim` for its `Session` (`crates/wf-core/src/view.rs`); `App::battle(packs, cfg)` wires it.
* **Netcode / rollback**: inputs are plain `u16` per player per frame; the step is the only mutation; `state_hash`
  for desync detection, `snapshot/restore` for rollback. Render and audio consume outputs only, so re-simulating
  frames during rollback just drops their events (or diffs them).
* **Replays** (implemented: `wf-replay` + `wf_app::control`, see `docs/replays.md` for rewind, save states, ghosts, training overlays and keys): (seed, content hash, per-frame inputs) → identical states, views and events. `Session` (below) runs them
  headless; the audio log makes sound verifiable.
* **Multiplayer view**: every local player gets its own viewport from `cameras[p]` of the shared view
  (`App::render(player, w, h)`); split screen = several renders.
* Anim timing comes from content (`FrameSel::Anim` is resolved by the renderer from `sprite.json` durations), but a
  simulation that tracks frames itself can send `FrameSel::Frame`.

## 3. Views and pacing

* `ViewMode::Pc{zoom}` (default, auto zoom = smallest >= 2 leaving <= 8 % black, max 6): view = window / zoom,
  centred on the player's GBA camera — more of the arena at the same pixel size. `ViewMode::Gba{scale}` = classic
  240x160. Game logic only ever sees the 240x160 camera.
* The sim steps at the GBA's 59.7275 Hz (fixed step, max 4 catch-up steps per host frame).
* Keys (W3C `code` names, same on both shells): arrows, X = A, Z = B, A = L, S = R, Enter = Start, Backspace =
  Select. Rebinding: web settings panel (localStorage), native `<data>/bindings.json`. Gamepads: standard mapping
  (web Gamepad API / gilrs), positional (right face = A, bottom = B), shoulders L/R, left stick = d-pad. Taps shorter
  than a frame are latched so they still reach the game.

## 4. ROM flow and content cache

Web (`engine/web/app/`): drop / pick the ROM → JS checks SHA-1 and the header (other game, EU `BG3P`, modified USA
dump: friendly messages) → `extract-worker.js` runs `wf_wasm.wasm` category by category (progress bar) → one tar of
the base pack + the ROM are stored in **OPFS** (IndexedDB fallback) → next launches untar the cache (≈10 ms). The
cache key is the SHA-256 of `wf_wasm.wasm`: a new extractor re-extracts automatically from the stored ROM. Mods: the
settings panel adds a folder (`webkitdirectory`) as a pack. "Forget ROM & content" wipes everything. Nothing is
uploaded; no ROM-derived data ships with the page.

Native: `buus-fury [rom.gba]`, or drop the file on the window / press O (file dialog). Content goes to
`<data dir>/wf-buu/content/base` (macOS `~/Library/Application Support/wf-buu`), mods in `content/mods/<name>/`, the
ROM is kept as `rom.gba`; content is re-extracted when `pack.json.generated_by` differs from this build's extractor.

Build/serve: `engine/web/app/build.sh`, then serve `engine/web/` and open `/app/`. URL params: `area=8-3`,
`players=N`, `mode=pc:0|gba:3`, `stats`, `mute`, `sound`, `autostart`; test hooks `devrom=<same-origin url>`,
`devinput=ArrowRight:60,ArrowUp:30`.

**Browser end-to-end check** (headless Chromium via `playwright-core`, `--mute-audio`, the page mutes itself under
automation; never a window): `cd engine/web/tests && npm install && node browser_check.mjs [--build] [--only 2,4,6]
[--keep DIR]`. It serves `engine/web/` plus `roms/buu.gba` as `/rom.gba` (`serve.mjs`) and runs, on a fresh browser
profile:

1. The ROM drop: extraction in the worker, the content cached in OPFS.
2. Launcher → Story → Title Screen → New Game, then about 3,200 frames of the prologue.
3. The session's recording (`wf_app_current_replay`) verified by the native build (`buus-fury --replay`: every
   checkpoint's state hash from the browser against a native session fed the same inputs) and by the page.
4. Story mode at 8-1 (`?story=8-1&autostart`): walk to the save point, save, reload the page. Launcher → the slot
   continues at the saved spot, and options written into the stored block are used.
5. Rewind, save state / load state, the recording after them verified natively, replay playback in the page.
6. A battle from the launcher, its recording verified natively.

It finishes with "no page errors or console warnings". `--build` runs `app/build.sh` first, which honours
`CARGO_TARGET_DIR`. `wf_app_story_info` (area, leader position, zenie, play time, menu, options) is the page's
probe for these checks. `probe.mjs` is a scratch tool: launcher keys in, screenshots out.

## 5. Headless, muted, spectator (product features)

* **`--headless`** (native): no window, no event loop, no audio device. `wf_app::session::Session` steps the sim,
  keeps the sound driver running **muted** (its commands are logged in `audio_log`, voice usage observable), and
  renders on demand: `session.screenshot(player, w, h)` → `Shot` (`.ppm()`, `.bmp()`) — screenshots or video
  frames. CLI: `--frames N --input R60,D40 (or --input-file F) --size 1920x1080 --shot out.ppm [--realtime]`.
* **`--mute`**: windowed but never opens an audio device; the driver is still clocked and fed.
* **Automation default**: a non-interactive launch is headless + muted unless a human asks for a window
  (`--window` / `WF_WINDOW=1`): `WF_AGENT=1`, `WF_HEADLESS=1`, `CI`, `WF_MUTE=1` (mute only), or neither stdin nor
  stdout is a terminal (`session::automation_default()`). Web: muted by default under automation
  (`navigator.webdriver`) or `?mute`; the worklet still runs the driver (`window.wfAudioLog`).
* **Spectator**: `--spectate 127.0.0.1:5372` serves a read-only viewer (`/` page, `/frame.bmp?w=&h=`, `/state` with
  frame, state hash, audio log tail) from a headless session paced in real time. Frames are rendered between steps
  from the current state, so attaching or detaching a viewer never changes the simulation.
* Tests use these: `crates/wf-app/tests/frontend.rs` (content sound bank == ROM bank sample-for-sample, demo
  determinism, render purity, per-player cameras), `examples/shot.rs`, `buus-fury --headless`, and browser checks in
  headless Chrome with `--mute-audio`.

## 6. Simulation: wf-core battle (default) and the demo world

The shells run `wf-core`'s battle `Session` (it implements `wf_view::Sim`; `wf_app::battle` feeds its
`GameData` from the same layered packs as the renderer, so mods reach both). Native: `--arena forest --enemy t68
--count N --fighter goku --form F --level L --seed S`; web: the same as URL parameters. `--demo` / `?demo` (or an
`area`) runs `wf_app::demo::DemoWorld` instead: map + collision + music of an area, Goku walking / dashing with the
game's walk rules and camera, no enemies (useful for any map).

`crates/wf-app/tests/battle.rs` drives the real battle headless and muted with an autopilot that only reads the
`RenderView`: the enemy is rendered from content, the player walks to it and punches, the core emits hit SFX
(48/49), swings (50/51) and the low-HP beep (41), and the muted driver starts SFX voices for them.

### Story mode (`wf_app::story::StorySim`)

`buus-fury --story [W-A]` / `?story[=W-A]` starts a new game in the field at area W-A (default 1-1 Snakeway): a
`wf-core` field session (areas, exits / warps, NPCs, pickups, breakables, enemies, the world map) whose scripts run
on the script host (`wf-script-core`) through the Rhai runtime (`wf-script-rhai`) from the content pack's
`scripts/*.rhai` (moddable; no ROM at runtime). Per frame: `Session::step_begin` (deferred calls: drop scripts, kill
XP, defeat scenes) and the scripts it queued, `Session::step_update`, then the blocked script resumes when
`wf_script_core::poll` says so and the frame's other `FieldEvent::Run` scripts (talk, pickups) start, then
`Session::step_post` and the trigger check's scripts. An area load whose on-enter script blocks waits for it
(`field::finish_arrival`, the host calls it). Same order as the story playthrough lockstep (docs/script-host.md).

The game's presentation timing (all measured frame by frame against the emulator) is wf-core's: `render_view`
(palette fade, banner, tile timer, greyed icon, dialogue animation, world-map props / box) and `view::Presenter`
(the one-frame OBJ lag, the world map's older BG camera). `StorySim` and `Session`'s `Sim` use them; the frontend
has no stand-ins left:
* **palette fade** of area transitions (`RenderView.palette_fade`): the fade routine's math on the original RGB888
  palette (`maps/palette.json`) — FadeOut k -> level 8k, FadeIn k -> 256 - 8k. The renderer resolves each pixel's
  palette entry exactly: BG tiles, UI pictures and the (now indexed, OBJ-palette-ordered) skill icons carry their
  index in the colour's top byte; RGBA sprite sheets fall back to the first OBJ entry with that colour.
* **OBJ layer one frame late**: sprites, HUD, banner and dialogue box show the previous frame's build (shadow OAM
  / OBJ tiles are copied at the next VBlank) over the current BG scroll; the text surface is re-uploaded every
  frame, so the typewriter count is the current one.
* **tile animations**: the map's tick routine (`0x08009B80` init, `0x08009CC2` tick, once per logic frame, not on
  load frames): animation k (header order) shows frame 0 with countdown k + 1, then each frame for its duration;
  overlapping animations show the most recent upload (`TileAnim::after_ticks`, `anim_frame` = logic frames since
  the load). Verified against the captured RAM records (0x03001A68) on every frame of every trace.
* **skill icon greyed** (`_alt`) in areas without combat; **area banner** (§7; a load removes a banner).
* **dialogue box animation** (`RenderView.dialog_anim`, `wf_core::view::dialog_anim`): the three box OBJs zoom about
  the box centre while opening / closing (affine PA = PD = 2048 / n, n = 2..8 over the 7 opening frames, 7..1
  when closing; OBJ 0 on top where the zoomed rectangles overlap) and the text types one visible character per
  frame (`wf_script_core::text::typing` counts).
* the HUD stays up behind text boxes (cutscenes hide it through `HudView.hidden`).

**Field parity** (`crates/wf-app/tests/field_parity.rs`; captures `crates/wf-render/re/capture_field.py`: the
field recorders' traces re-run with the emulator screen, OAM, tile-animation records and cameras of every video
frame, git-ignored work/frontend/field/). Objects traces run the game's scripts through story mode (item-get
text) except the drops trace, whose recording patched the on-break bytecode (the lockstep's stand-in hook); traces
that start mid-area get the game's camera (22.10) after their first step.

| trace | frames | exact | remaining differences |
|---|---|---|---|
| npc_yemma_crowd (King Yemma's office) | 1200 | **1200** | — |
| objects_item_8_57 (map item, item-get, "You got a Switch!" box) | 392 | 390 | frame 0 (trace start); frame 65: the item-get icon one frame too long (core) |
| npc_westcity_idle | 900 | 895 | 9-10, 21-22, 30: palette written mid-scanout (below) |
| transitions_forest_roundtrip (fades, banner, two loads) | 390 | 384 | 6 fade frames: palette written mid-scanout (7-50 px) |
| transitions_hercule_edge_up | 210 | 176 | the Hercule City wanderer is placed now (the test extracts scripts: spawn conditions); remaining: fade frames as above |
| objects_breakable_drops_8_1 | 470 | 391 | the camera holds during a shake now (the shake object takes the camera slot) and the broken rock's prop is hidden |

Not modelled (render timing below frame granularity): on some fade frames the palette RAM write overlaps the
scanout — scanlines above the write point (rows 0..~60, or a few rows of one OBJ colour) still show the previous
fade level; when that happens depends on the CPU time of the frame, which the emulator knows and the
frame-level sim does not. Same class as the world map's late HBlank lines (§9b).

Done in wf-core (2026-10-02): `dialog_anim` in `render_view`, the camera hold during shakes, the Hercule City
wanderer, the broken rock's prop. Open: item-get icon lifetime (frame 65 of objects_item_8_57).

## 7. Game UI: HUD and dialogue box (`wf_render::ui`, content `ui/`)

* HUD: `ui/hud.json` + `hud/{frame,gauge_ss,gauge_ss3}.png` + `icons/skills/`. Composed like `0x0800B99C`:
  skill icon and frame first, then every gauge step from `hud.json` (HP, EP with SS / SS3 colours, the 72-step
  experience line — it runs through the icon OBJ too). Screen OBJs at (8, 8), priority 1, anchored to the view's
  top-left. `HudView` carries `hp/hp_max/ep/ep_max/ss/ss3/xp72/skill/skill_alt/hidden`; hidden while a dialogue
  box is open.
* Dialogue: `RenderView.dialog = DialogView { text, style, page, position, portrait, portrait_left }`; the renderer
  runs the game's line fitter (`0x0803DE6A`), pages by the style's line count and lays the page out like
  `0x0800AEF6` with the `dialog` font into the 160x64 box picture (lines 14 px apart); 3 OBJs at x 40, priority 0;
  y = 8 top / 88 bottom (emulator OAM; middle 48), anchored to the view's centre / bottom edge in the zoomed-out
  view. `portrait` (a `portraits/` portrait_index) adds the 64x64 portrait OBJ beside the box — the 224-px span
  centred on x 120, left / right by `portrait_left` — and switches to the left / top-aligned text layout with
  indented `-` options (`docs/ui.md` §3); the open / close zoom covers all four OBJs. wf-core fills it from the
  text node's arg (`view::dialog_portrait`: 1..6 = the leader form's `portraits` slot, ≥ 8 = arg − 8; 7 = style 1)
  and places the box with `scene::speaker_layout` (prefix, else arg + the talk anchor `Scene.talk_anchor`, set by
  a map-NPC talk to player − NPC).
* Without `ui/` content (old caches) the placeholder HUD is drawn.
* **Area-name banner** (`wf_render::banner`, content `ui/banner.json` + `ui/images/banner.png` + `fonts/small`):
  `RenderView.banner = Some(BannerView { text, t })`. The 240x16 strip (background picture, name centred at
  x0 = 120 - (width + 1) / 2, pen y 11, shadow colour 1 at (+1, +1) then colour 2 per glyph) is 8 affine OBJs at
  y 128, priority 0, no blending; from t = 223 (object timer <= 16) the OBJ matrix d = 0x1000 / timer collapses it
  onto its centre line (rows sample `((d * (y - 8)) >> 8) + 8`); gone at t = 239. Zoomed-out view: horizontally
  centred, 32 px above the view's bottom edge (= y 128 in the classic view).
  **Rule for wf-core**: when EnterArea (`0x08008D44`) finishes a load whose area name string id (area table +0xC,
  `areas.json` `name_string_id`) differs from the last one shown (RAM `0x03001984`, persistent across loads) and is
  not 0, record it as the last shown and start a banner with that text; not on the 3D world map (map header +0x3C:
  the id is recorded, a label object `0x0800C0C8` is created instead). `t = 0` on the first frame after the load
  (the first frame the new area runs; the banner's timer counts down during the area's fade-in like everything
  else), `t += 1` every frame; drop it after t = 238. (Verified frame by frame vs the emulator, see §8.)

## 8. Emulator parity (headless, muted)

`crates/wf-render/re/capture.py` (repo `.venv`, mGBA library: no window, no audio device) plays save states with
scripted keys and dumps the screen plus player / camera / BLD registers / OAM / character record / HUD tiles into
`<repo>/work/frontend/` (git-ignored: screenshots are ROM-derived). `cargo run -p wf-app --example parity` and
the test `crates/wf-app/tests/emulator_parity.rs` rebuild each scene as a `RenderView` (map identified by search,
player frame from the captured animation step, halo, shadow, HUD) and diff it:

| scene | result |
|---|---|
| 10 Southwest Forest scenes: idle, walking, under tree tops (BG0 priority 2 blended over the OBJ with EVA 12) | **0 px** differ, whole frame |
| intro dialogue box ("normal...", style 0, top) | **0 px** |
| dialogue with portraits: intro narration + cutscene, NPC talks from 3 sides, a choice; typed out + open zoom (`re/capture_dialog_portrait.py`, `tests/dialog_portrait_parity.rs`, 36 frames) | **0 px** |
| Snake Way (3), forest fight | 0 px outside the other actors' OBJs (NPCs / enemies are not rebuilt by the tool) |
| HUD (all scenes) | 0 px |
| area-name banner, forest 8-3 -> 8-2 (`re/banner_capture.py`, `tests/banner_parity.rs`): surface vs OBJ VRAM; screen rows 128..144 every frame t = 31..238 (hold + collapse) and after | **0 px** |

Found and fixed on the way: mGBA blends on 8-bit expanded channels (`(a8 * eva + b8 * evb) >> 4`), not 5-bit —
the default `BlendMath::Mgba` matches the reference, `BlendMath::Hardware` is the PPU's 5-bit math; the HUD
experience line overwrites the icon area; **the screen shows the previous frame's camera** (BG scroll and OAM are
written before the camera update) — the demo world does this, and a sim's `CameraView` should be the camera that
is on screen; tile-animation phase is the map's own timer (the view's `anim_frame`).

## 9. Desktop controls screen

F5 opens an in-game controls screen (`wf_app::rebind::RebindMenu`, drawn with the game's `menu` font): Up / Down
to pick a GBA button, Enter then the new key (Esc cancels), "Reset to defaults", "Done". Arrow keys always navigate
the screen, so a bad binding cannot lock you out. Saved to `<data>/bindings.json`. Headless preview:
`buus-fury --headless --show-controls --shot out.ppm`. (The web shell keeps its HTML settings panel.)

## 9a. Launcher and packaging (`wf_app::launcher`, `engine/package/`)

Both shells open on a launcher drawn over the title art (`ui/images/screen_title.png`) in the game's `menu` font:
Story (hand-over to the game's title screen `launcher::title_app`, or continue slot 1-3 / quick start), Battle (fighter, form, level, enemy or story preset, count, arena, 1-4
players), Online (the lobby), Replays, Mods (on / off, mods folder), Settings (view, volume, fullscreen, controls).
One state machine fed W3C key codes returns a `Choice`; the shell builds the app (`launcher::story_app` with the
save slots, `App::battle`). Story save slots are `World.saves` (the game's own Save screen writes them): shells
persist them when `launcher::saves_of` reports a new generation (native `<data>/saves/slotN.sav`, web localStorage
`wf.save.N`) and pass them back on the next start. Native: no mode flag = launcher (`--launcher` / `--no-launcher`),
Shift+Esc returns to it, headless `--launcher [--launcher-keys K,K] --shot` renders / drives it. Web: no mode query
parameter = launcher (exports `wf_launcher_*`, `wf_app_saves`); `?nolauncher` skips it.

Packaging: `package/mac-app.sh` (release build → `dist/Buu's Fury.app`, generated generic icon, ad-hoc signed,
`LSEnvironment WF_WINDOW=1` and a bundle-path check so Finder launches are never treated as headless automation),
`package/web-dist.sh` (→ `web/dist/`, static), `package/serve.py` (local server), `package/web_check.mjs` (headless
Chrome smoke test). Quickstart: `engine/README.md`.

## 9b. World map (mode 7) — `wf_render::mode7` / `mode7_draw`

When `MapView.mode7` is set (area 0-50), the renderer draws the game's mode-1 picture itself from content
(`maps/area_00_050/mode7.json` + `objects.json` props, docs/maps.md "World map"):

* **BG2 ground**: per scanline 56..159 the parameters of `0x080086D8` (pitch + per-line angle → csc / cos
  tables → distance; heading rotation; `pa / pc / x / y` exactly as the table in RAM); lines above use the
  values left by the last IRQ write (all -1). Samples read the world tile map at their true position.
* **BG1 sky**: hofs = (heading << 3) >> 10, columns taken from the two panorama chunks like `0x08007AB4`.
* **BG0** haze + mini-map outline: an ordinary fixed map layer; blending BLDCNT 0x1641 (BG0 over BG1/BG2/OBJ,
  EVA 6, EVB 16) like every map.
* **3D camera** (`0x0802ED98`): quaternions heading × 90° × pitch (1 << 15) → 1024-scale view matrix, far
  distance, ground bounding box; **projection** `0x0802F1C6` (BIOS-Div rounding). Verified against RAM for 480
  recorded frames (matrix, rotation, far, bbox all identical; tables identical).
* **OBJs** in OAM order: mini-map marker (affine, rotated by the heading, priority 0), the bottom label (bar picture
  + `small` font, centred over all code units, colour 2 with a colour-1 shadow at (+1, +1)), then the flyer group
  (halo, pieces, shadow sharing one matrix: scale `depth * 0x222 >> 17`, PA negated when the frame's first piece is
  h-flipped) and the billboards (scale `depth * 10 >> 12`, culled by the bounding box), nearest first, at most 16
  affine matrices per frame.
* View data: `MapView.mode7` = the camera the OBJs are projected with; `RenderView.world_map` (optional) adds the
  camera the BG table was built from (`bg_camera`), the exact flyer (position, frame), script prop visibility and
  the mini-map box. Without it the flyer is derived from the camera (exact inversion of `0x08034124`) and found
  among the view's sprites (the player sprite; other sprites are ignored on the world map).
* Timing (emulator captures): the BG table is built from an earlier camera than the OBJs, and the flyer's frame
  graphics (VRAM tiles, uploaded immediately) can be newer than its OAM entry (copied at the next VBlank) — the
  simulation should fill `world_map.bg_camera` / `flyer` accordingly.
* Zoomed-out PC view: the GBA picture is centred; the ground continues below it (scanline angles extended with
  atan), the sky's top row above it, BG0's haze sideways with its left column; the label sits on the bottom edge.
* Parity (`tests/emulator_parity.rs` `world_map_matches_emulator`, captures from
  `crates/wf-render/re/capture_worldmap.py` in `work/frontend_wm/`): 12 scenes (start, cruise, turns, climb,
  high / low altitude, reverse, far east / north / west, spin) — ground, sky, haze, mini-map, marker, label,
  billboards, flyer, halo and shadow pixel-identical; the only differences are 0–2 whole scanlines in some
  frames where the HBlank IRQ ran late (the line shows its neighbour's affine parameters; sound-DMA timing, not
  modelled) and one capture taken mid label-tile upload.

## 10. Gaps

* wf-core's view: no halo / attachment draws (forms with a halo show without it), no selected-skill icon, SS /
  SS3 gauges or experience steps in `HudView` (fields exist), camera is the post-update camera (one frame ahead of
  the game's screen), beam pieces not drawn (their note).
* Dialogue: middle y unverified; the choice cursor arrow (`0x080178C8`) and the box surface's fill-in during the
  opening zoom (black / partly striped interior) are not drawn; nested `run_scene` scenes inside a talk event use
  (−1, −1) by `Scene.depth` > 1 (not traced); the centred layout still draws an option's `- ` (the game skips it).
* Fog / wrap overlay layers and semi-transparent OBJs have no emulator scene yet (no save state on such a map).
* Mod `index.json` regeneration is not done by the loader (listing works via `files.json` / directory walk).
* Gamepad buttons are rebindable only through `bindings.json` (the screen rebinds keys).
* World map: wf-core does not fill `RenderView.world_map` yet (bg camera one frame older, exact flyer + frame, prop
  visibility from `prop_show` / `prop_hide`, the `spawn_box` mini-map rect, the story destination markers
  `spawn_marker_a/b` — the latter are not drawn at all); HBlank-IRQ scanline jitter is not modelled.
