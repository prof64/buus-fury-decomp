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

* **wf-core**: implement `Sim` for `World` (or an adapter in `wf-app`): `step` = the game frame with `InputFrame`s
  built from the bits, `view` = walk entities in OAM order and emit `SpriteDraw`s (+ shadow / halo draws, priority 3
  for actors), cameras from the world's per-player cameras, HUD values, audio from the world's frame events.
  `App::new(packs, Box::new(world))` is all the shells need; nothing else changes.
* **Netcode / rollback**: inputs are plain `u16` per player per frame; the step is the only mutation; `state_hash`
  for desync detection, `snapshot/restore` for rollback. Render and audio consume outputs only, so re-simulating
  frames during rollback just drops their events (or diffs them).
* **Replays**: (seed, content hash, per-frame inputs) → identical states, views and events. `Session` (below) runs them
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

## 5. Headless, muted, spectator (product features)

* **`--headless`** (native): no window, no event loop, no audio device. `wf_app::session::Session` steps the sim,
  keeps the sound driver running **muted** (its commands are logged in `audio_log`, voice usage observable), and
  renders on demand: `session.screenshot(player, w, h)` → `Shot` (`.ppm()`, `.bmp()`) — screenshots or video
  frames. CLI: `--frames N --input R60,D40 --size 1920x1080 --shot out.ppm [--realtime]`.
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

## 6. Demo world (until wf-core drives the shell)

`wf_app::demo::DemoWorld`: area from `areas.json` (default 8-3 Southwest Forest) → map, music (song 28), collision;
Goku (`sprites/106_goku`) with walk speed `5*SPD+102` from `characters/goku.json`, the game's walk try-move
(horizontal then vertical step, 8 px corner sliding, footprint vs walk grid, walk-bounds arena), facing table,
double-tap dash, camera easing (3/64, 9/128, 22.10) clamped to the map, idle/walk/dash animations from content, ground
shadow, N players with own cameras. Placeholder HUD (name, level, HP/EP bars) in a built-in 5x7 font.

## 7. Gaps

* wf-core's World is not wired yet (its crate had no `lib.rs` during this work); the `Sim` adapter is the remaining step.
* HUD / menus / fonts are placeholders until UI graphics are extracted (`docs/ui.md` work).
* Wrap-layer and BG0 blending follow the map's BLDCNT/EVA/EVB literally; not yet compared to emulator captures.
* Mod `index.json` regeneration is not done by the loader (listing works via `files.json` / directory walk).
* Native rebinding has no in-game UI (edit `bindings.json`); no app bundle yet (a bundle would pass `--window`).
* Rewind / save states / netcode need wf-core's `snapshot`/`restore`; the demo world does not implement them.
