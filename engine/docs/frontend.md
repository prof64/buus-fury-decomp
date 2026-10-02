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

## 7. Game UI: HUD and dialogue box (`wf_render::ui`, content `ui/`)

* HUD: `ui/hud.json` + `hud/{frame,gauge_ss,gauge_ss3}.png` + `icons/skills/`. Composed like `0x0800B99C`:
  skill icon and frame first, then every gauge step from `hud.json` (HP, EP with SS / SS3 colours, the 72-step
  experience line — it runs through the icon OBJ too). Screen OBJs at (8, 8), priority 1, anchored to the view's
  top-left. `HudView` carries `hp/hp_max/ep/ep_max/ss/ss3/xp72/skill/skill_alt/hidden`; hidden while a dialogue
  box is open.
* Dialogue: `RenderView.dialog = DialogView { text, style, page, position }`; the renderer runs the game's line
  fitter (`0x0803DE6A`), pages by the style's line count and lays the page out like `0x0800AEF6` with the
  `dialog` font into the 160x64 box picture; 3 OBJs at x 40, priority 0. y = 8 for top boxes (emulator OAM);
  middle / bottom use 48 / 88 (not yet confirmed on a capture) and anchor to the view's centre / bottom edge in
  the zoomed-out view.
* Without `ui/` content (old caches) the placeholder HUD is drawn.

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
| Snake Way (3), forest fight | 0 px outside the other actors' OBJs (NPCs / enemies are not rebuilt by the tool) |
| HUD (all scenes) | 0 px |

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

## 10. Gaps

* wf-core's view: no halo / attachment draws (forms with a halo show without it), no selected-skill icon, SS /
  SS3 gauges or experience steps in `HudView` (fields exist), camera is the post-update camera (one frame ahead of
  the game's screen), beam pieces not drawn (their note).
* Dialogue middle / bottom y and the portrait / speaker-name variant are unverified / not rendered.
* Fog / wrap overlay layers and semi-transparent OBJs have no emulator scene yet (no save state on such a map).
* Mod `index.json` regeneration is not done by the loader (listing works via `files.json` / directory walk).
* Gamepad buttons are rebindable only through `bindings.json` (the screen rebinds keys).
