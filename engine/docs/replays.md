# Replays, rewind, save states and training tools

Built on wf-core's determinism: a battle is a pure function of (config, per-frame inputs), `Sim::snapshot` /
`restore` copy the whole state, `state_hash` checks it. Nothing here touches the simulation except through
`Sim::step` / `restore`; overlays only read.

```
wf-replay   shared formats (deps: wf-view, serde): InputLog, Replay (+ Recorder, verify, Checker), SaveState,
            Timeline (periodic snapshots: seek / rewind), lz (snapshot compressor). For the app, netcode, agents.
wf-app      control.rs (Control: recording, playback, rewind, seek, save states, ghost, overlays, host requests),
            hotkeys.rs (rebindable actions), overlay.rs (training boxes, AI labels, input display, text),
            replay_menu.rs (desktop replay browser), battle::CoreSim (wf-core session as a Sim + as_any)
wf-native   replays.rs: files under <data dir>; --replay / --record headless
wf-web      wf_app_key2 / wf_app_next_request / ... exports; web/app/app.js: OPFS / IndexedDB, replay panel
```

```
cargo test -p wf-replay                                        # formats + timeline on a toy sim
cargo test -p wf-app --release --test replay -- --nocapture    # real battle: round trip, rewind, restarts, perf
buus-fury --headless --frames 1200 --input R60,A5,D40 --record run.wfr
buus-fury --headless --replay run.wfr --shot end.ppm           # verify + play + screenshot
```

## 1. Formats (wf-replay, `FORMAT_VERSION` 1)

Every file: `magic(4) | version u16 | body | fnv64(everything before)`. Readers reject newer versions and bad
checksums. Integers in the body are LEB128 varints.

**Replay `.wfr`** (`WFRP`): `blob(header JSON) | flags | [blob(lz start state)] | input log | checkpoints`.

* `Header`: `sim` (`wf-core/battle`, or `wf-app/story` for story mode), `content` (`ContentId`: engine build `wf-core 0.1.0`, extractor =
  base `pack.json.generated_by`, ROM SHA-1, mod ids in load order, `mods_hash` = FNV-1a over every mod's id, file
  paths and bytes), `config` (the sim's own JSON: wf-core `BattleConfig`; story mode: `StorySim::start` =
  `{kind: "title" | "slots" | "area", slots (the slot files, hex), slot, area, chars, seed}`, rebuilt by
  `StorySim::from_start`), `seed`, `players`, `hash_interval`,
  `title`, `created` (wall clock, metadata only), `branch` (`{parent id, frame}` for take-overs / loaded states),
  `outcome`, `frames`, `final_hash` (listing fields).
* Start state: optional serialized snapshot (recordings that begin mid-session); normally absent = frame 0 of
  `config`.
* Input log (`InputLog`): `players, start frame, frames`, then per player runs `(length, buttons)`; buttons are
  GBA KEYINPUT bits (`wf_view::buttons`). Entry `i` drives the step from frame `start + i`.
* Checkpoints: `(frame delta, u64 state hash)`; the first is the start state, then every `hash_interval` frames
  (default 60 = one per second) and the final frame.
* JSON interchange: `Replay::to_json` (`wf-replay/1`); `Replay::from_any` also reads the agent server's
  `wf-agent-replay/1` JSON (wf-mcp `get_replay`): its final hash becomes the one checkpoint. The app imports
  either (drop / upload `.json`).

**Save state `.wfs`** (`WFSS`): `blob(StateHeader JSON: sim, content, config, frame, hash, created, slot, title) |
blob(lz snapshot) | blob(replay file up to that frame)`. The embedded replay lets recording continue after a load
(and makes the state verifiable / rewindable to frame 0).

**Netcode / agents**: use `InputLog` for exchanged inputs, `Recorder` + `Checker` for desync checks (first bad
checkpoint), `Timeline` for rollback-free seeks; `ContentId` to refuse mismatched peers.

## 2. Verification

`wf_replay::verify(replay, fresh_sim)` restores the start state, compares the start hash (catches content /
config differences at frame 0), steps every logged frame and compares each checkpoint; it stops at the first
divergence: `Divergence { frame, last_good, expected, got }` = "diverged between frames last_good + 1 and frame".
With `hash_interval = 1` it is the exact frame (8 bytes per frame). During in-app playback `Checker` does the
same incrementally (any speed / seeks) and the view shows `DESYNC ...`. Different content shows a warning on load.

## 3. In the app (`wf_app::control::Control`, both shells)

* **Story mode** (launcher Story / title, `--story`): recorded, rewound, saved and loaded like a battle (the
  snapshot carries the script host: `StorySim::snapshot`); ghosts and the training overlay stay battle-only.
  `tests/story_control.rs`; headless `buus-fury --launcher --launcher-keys Enter,Enter --input .. --record F` then
  `--replay F` gives the same state hash.
* **Recording**: every live wf-core session; autosaved every 30 s, on win / loss, before switching sessions and on
  exit / page hide. Desktop `<data>/replays/<id>.wfr` (newest `keep_replays` = 50 kept), `replays/kept/` never
  pruned; web: store keys `replays/<id>.wfr`, `kept/<id>.wfr` (OPFS, IndexedDB fallback).
* **Timeline**: a snapshot every 30 frames (LZ-compressed, ~2–4 KB each) for the rewind window
  (`rewind_seconds`, default 60), one per 240 frames before it, 64 MB cap. Seek = restore the nearest snapshot at or
  before the target + re-simulate (≤ 239 steps, ~1 µs each).
* **Rewind** (hold): steps back one frame per frame of wall time (×`fast_forward` with Tab). Live input after a
  rewind truncates the recording there and continues (the rewound-over part is dropped, like an emulator);
  seeking forward again before playing redoes it.
* **Playback**: pause, frame advance, speeds ×1/8 … ×8, fast forward (SFX dropped above ×2, music kept), seek
  ±5 s / start, web scrubber. **Take over** (B) = a new recording with the replay's inputs up to the current frame
  and `branch = {replay, frame}`.
* **Save states**: 4 slots; load works across sessions / restarts / configs (the sim is rebuilt from the state's
  config, the hash is checked after restore).
* **Ghost**: a second wf-core session plays another replay in lockstep with the main frame number (own timeline,
  follows seeks); its player sprites are drawn blended (eva 7 / evb 9) over the view.
* **Training overlay** (F4): body boxes (green; dark dashed = not hittable), footprints (blue), projectile rects
  (red enemy, orange player), rocks; player state names; enemy name, HP and AI label `state leaf-node frames-in-
  state` (the running node is found by following composite cursors); damage log (last 8 `frame attacker>target hp
  [CRIT]` from `Event::Damage`). **Input display** (I): a pad per player with the frame's buttons. Status line:
  REPLAY / REC time, PAUSED, << REWIND, speed, GHOST, frame.
* **Replay browser**: desktop F10 overlay (Enter play, G ghost, V verify, K keep, D delete, E export, C save the
  current recording, O import; or drop a `.wfr` / `.json` on the window); web F10 / "Replays" panel (Play, Ghost,
  Verify, Keep, Download, Delete, Upload, Download current).
* The shells feed `Control::key` (before game keys), `advance`, `render`, and service `take_requests()`
  (`SaveReplay`, `SaveState`, `LoadState`, `OpenReplays`). Demo world and net sessions (`CoreSim::of` = None) leave
  everything inert.

### Keys (defaults, `Hotkeys`; desktop `<data>/hotkeys.json`, web settings panel / `localStorage wf.hotkeys`)

| key | action | | key | action |
|---|---|---|---|---|
| hold `` ` `` | rewind (mGBA's key) | | F6–F9 | load state 1–4 |
| hold Tab | fast forward ×4 (mGBA's key) | | Shift+F6–F9 | save state 1–4 |
| P | pause / resume | | F10 | replay browser |
| N | frame advance (repeats while held) | | B | take over replay |
| − / = | slower / faster | | F4 | training overlay |
| [ / ] | seek −5 s / +5 s | | I | input display |
| Home | back to start | | G | ghost on / off |

None is an mGBA game key (arrows X Z A S Enter Backspace); a key bound to a GBA button never triggers a hotkey
without a modifier. Save slots follow mGBA's plain = load / Shift = save, moved to F6–F9 because F1–F5 are taken
(view, zoom, stats, training, controls). Combos are W3C `code` names with `Shift+` / `Ctrl+` (Ctrl or Cmd) /
`Alt+`.

## 4. Numbers (Apple M-series, release; `tests/replay.rs`, headless browser)

| what | result |
|---|---|
| 1 min fight (3600 frames, 1 player, autopilot) | 2.3 KB file (1.1 KB inputs, 61 checkpoints) |
| verify 3600 frames | 2.2 ms native |
| snapshot | 67 KB raw → 2.0 KB LZ; snapshot 32 µs, compress 35 µs, decompress 45 µs, restore 46 µs |
| save state file | 3.9 KB; save 0.09 ms, load 0.35 ms |
| seek anywhere (1 min replay) | 0.09–0.12 ms |
| rewind | 0.09 ms per host frame; timeline for 1 min = 121 snapshots, 236 KB |
| render + ghost + training + input overlays (480x320) | 0.5 ms |
| browser | rewind / save / load across reload / playback / verify / take over checked in headless Chrome (muted) |

## 5. Gaps

* Melee strike rectangles are transient inside wf-core (computed and applied in one call): the overlay shows
  hurtboxes, footprints and projectile rects, not punch hitboxes (needs a debug event from the core).
* `ContentId.engine` is the crate version; engine changes between commits are only caught by the checkpoints.
* Divergence is reported to a checkpoint window (60 frames) unless recorded with `hash_interval = 1`.
* Music after loading a mid-fight state into a different arena restarts the area song; exact song position is not
  restored. Ghosts from another arena are drawn at their own coordinates (warned).
* Desktop hotkeys are rebindable only via `hotkeys.json` (the F5 screen covers GBA buttons); web has a panel.
* Netplay sessions do not record through Control (rewinding is meaningless online); wf-net can use `InputLog` /
  `Recorder` directly.
