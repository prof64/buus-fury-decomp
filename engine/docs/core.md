# wf-core — the deterministic battle simulation

`engine/crates/wf-core` is the simulation core of the Buu's Fury remake: the battle as plain data, stepped only by
player inputs, bit-identical on every target (native, `wasm32-unknown-unknown`), cheap to snapshot and hash every
frame, observable by agents, rendered and heard by separate consumers. It is a port of the TypeScript reference sim
(`site/src/battle/rulesets/buu/`) whose behaviour is verified against emulator recordings; the port reproduces it
frame for frame (§7).

```
cargo test -p wf-core                                  # parity suites (needs roms/buu.gba or WF_CONTENT)
cargo run --release -p wf-core --example bench         # step / snapshot / hash timings
cargo build -p wf-wasm --target wasm32-unknown-unknown --profile release-wasm   # extractor + session in one module
node engine/web/core_check.mjs <wf_wasm.wasm> roms/buu.gba                      # headless wasm session check
```

## 1. Design requirements (non-negotiable)

1. **Deterministic everywhere.** Integer / fixed-point math with the game's own semantics (24.8 positions, 22.10
   camera, 1/1024 HP, 32-bit wraparound where the game wraps, `Math.imul`-style products, arithmetic shifts,
   truncating division), the game's RNG only (`rng.rs`, state in the world), no wall clock, no hash-map iteration
   (only `Vec` / `BTreeMap`), entity update order = the game's object list order.
   *One documented exception*: three **fitted** hit models of the reference that are not game mechanics (the
   Flame/Kame `GrowBeam` head speed, the Kamikaze ghost homing, which uses V8's `Math.hypot` algorithm) accumulate
   decimal fractions in `f64`. They use IEEE-exact operations only (`+ - * / sqrt`, comparisons, JS rounding), never
   transcendental functions, so they are bit-identical on every IEEE-754 target incl. wasm (Rust never fuses
   multiply-adds). Everything the reference computes with `sin` / `cos` / `hypot` in game logic is an exact integer
   here (`fixed::round_div_sqrt`, V8-generated integer tables in `tables.rs`).
2. **One plain-data `World`** (`world.rs`): every bit of state (entities, RNG, globals, the enemy collision layer,
   cameras, battle phase, the frame's events) is owned data. `World: Clone + Hash` (+ serde). Static game data
   (sprites, AI trees, maps) is shared behind one `Arc<GameData>` and excluded from hashing / serialization.
3. **Inputs are the only external input.** `Session::step(&[held buttons per player])` (or `step_frames` with explicit
   edges) is the only mutation. N player slots (`MAX_PLAYERS = 4`); every player is its own entity with its own
   camera; enemies target a player slot (`Enemy::target`, round-robin at spawn; the original is slot 0 only).
4. **Rendering / audio / UI read state only.** Sounds leave a step as `world::Event` data (`Sfx`, `SfxStop`, `Song`,
   `Damage`, `SkillStart/End`, ...); visual effect objects are entities with the game's lifetimes; `view.rs` builds a
   `wf_view::RenderView` from `&World`. Nothing in wf-core opens a window or touches an audio device: headless,
   muted runs are the default and can still assert on every sound.
5. **Agent interface from the start**: `observe(&World, player) -> Observation` (serde: positions, HP/EP, facing,
   states, skills, gauges, enemies with AI stacks, projectiles with boxes, rocks, a walk-grid window around the
   camera) and the headless `Session { new/reset(config), step(inputs), observe, snapshot, restore, hash }`, native
   and through the wasm exports (§6). Python bindings: planned as a thin `pyo3` crate over `Session` (config and
   observations are already JSON/serde), not built yet.
6. **Rollback-ready**: steps are pure functions of (state, inputs); `World::clone()` is the snapshot, `state_hash()`
   the per-frame desync check, `snapshot::to_bytes / from_bytes` the serialized form (GGRS integration next).

## 2. Architecture

| module | what |
|---|---|
| `content.rs` | `GameData::load(&dyn ContentSource)` from the extractor's pack (`DirSource`, `MapSource` for in-memory / browser packs): enemies, AI trees, characters, forms, skills, equipment, levels, sprite animations / body boxes / footprints, effect records, collision grids (all 256x256), map sizes, areas / songs. Two research tables are compiled in (`data/anim_timing_player.json`, `data/enemy_anim_sim.json`), like the extractor's annotations. |
| `world.rs` | `World`, `Entity { id, removed, body }`, `Body` = `Player` / `Enemy` / `Shot` / `Fx` / `Rock` / `Hand` / `Injector`; update loop (last to first, added entities wait a frame, removed dropped after the pass), `broadcast` (0x08011C70), collision layers, cameras, `StableHasher` |
| `rng.rs`, `damage.rs`, `progression.rs`, `fixed.rs`, `geom.rs` | PRNG 0x0803FAAC, ComputeDamage 0x0800E9D8 (crit, resist, block, HP accumulator), LevelUp / RecalcStats / fusion, integer helpers |
| `player.rs` | the player state objects and animation ticks (panim), walk / dash / melee / block + shield / hurt / KO, forms (SS / SS3 gauges, transformations, Saiyaman, story-form and fusion timers, defuse), all skills, Instant Transmission's modal pick, hold states, every PlaySfx / StopSfx |
| `enemy.rs`, `eanim.rs`, `path.rs` | enemies: stats, enemy-grid marks, verified steps (path follower, back-off, step-away, wander, straight walk), knockback / launch / wall splat, the game's enemy animation states and their sounds, attack helpers, death / heal (incl. the Yakon wrap) |
| `ai.rs`, `nodes.rs` | AI trees compiled once per state (`SubState` inlined); each pushed state gets a fresh `Vec<NodeRt>` (plain data). Node code addresses its runtime by (stack level, entry serial, node index): writes after a replace / pop are dropped, exactly like the reference's orphaned objects. All 195 behaviour node kinds + the on-hit trees. |
| `shots.rs`, `fx.rs` | projectiles / attack objects (enemy and player: ki blasts, beams, Final Flash, Big Bang, Masenko, ghosts, volleyball, waves, flames, bombs, arms, candy beam, Super Buu ball, dark sphere, ...) and effect objects (puffs, explosions, debris, Dragon Fist chain, Flame Kamehameha links, auras) |
| `battle.rs` | `BattleConfig` (serde), arenas (curated map + spawn), fighter records, enemy spawning, phases, the game camera 0x080077EC, low-HP beep, outcome |
| `session.rs`, `observe.rs`, `snapshot.rs`, `view.rs` | the headless API, observations, byte snapshots, the `wf_view::Sim` adapter |

**Frame** (`Session::step_frames`): clear events → `battle::pre_step` (enemy grid marks follow teleports) →
`World::step_entities` (frame + 1; entities last to first; the player sits behind the initially placed actors, as in the
game's object list) → `battle::post_step` (cameras, HUD beep, phases) → outcome.

## 3. Snapshots and hashing

* In-process: `Session::snapshot()` = `World::clone()` (plain data + one `Arc`): ~1.5 µs for typical battles.
* Serialized: `snapshot::to_bytes` (postcard, ~66 KB, the 64 KB enemy collision layer dominates) ~30 µs;
  `from_bytes` re-attaches the static parts (game data, map grids, compiled AI trees) from `GameData`.
* `World::state_hash()`: `std::hash::Hash` over all state with `StableHasher` (fixed little-endian widths, `usize` as
  u64): ~13 µs. Same value native and wasm (checked: `tests/session.rs` + `web/core_check.mjs`).

Benchmark (`--example bench`, Apple M-series, release): 3600 frames of 4 / 12 Majin Fighters, Janemba with parts, Kid
Buu: step 0.3–1.3 µs, clone snapshot 1.3–1.6 µs, restore 1.1–1.8 µs, hash 11–15 µs, bytes 26–30 µs.

## 4. Seams

* **Scripts (wf-script / wf-script-rhai).** Script state is engine data: the plan is a `World::scripts` field holding
  the running `wf_script_rhai::ThreadState`s, their action queues and the script globals (all `Clone + Serialize`,
  snapshot = clone), plus the engine-side action executors (walks, fades, text boxes) as world state. The `Host`
  implementation is a function of `&mut World` (queries read the world, actions append to the queue; the engine runs
  queued actions inside `step_entities` in object order). Nodes `RunScript` / `InlineScript` (AI) are the hook points
  today and are no-ops; the defuse script of the cutscene fusions is hand-ported in `player.rs`. Dialogue goes to
  `RenderView::dialog` once wired.
* **Netcode.** Inputs are `u16` per player per frame; `state_hash` per frame; `snapshot/restore` for rollback. A
  spectator only reads (`view`, `observe`). The camera used by projectiles (`offCamera`) is the union of all players'
  cameras (identical to the game for one player).
* **Multiplayer rules beyond the original** (documented divergences only when N > 1): enemy targets per slot; candy /
  sphere / waypoint stay world globals as in the game; the screen-shake task runs from player 0.
* **Mods.** Everything comes through `ContentSource`; a layered pack source (the frontend's `wf-content`) can feed
  `GameData::load` directly.

## 5. Content

`GameData` is built from the extractor's base pack (`wf-cli roms/buu.gba <dir>` or `wf_extract::extract` in memory).
The tests extract the needed categories (`tables, sprites, effects, maps`) once into the cargo target dir; set
`WF_CONTENT=<content/base>` to use an existing pack. Verified identical to the reference's inputs (data/buu/*.json +
data/battle/*): enemy stats / AI / on-hit / reaction trees (822 states), sprites' animation steps and timings,
characters / forms / skills / equipment / levels, and collision inside every map (the pack's grids are the game's full
256x256; the reference clipped them to the image size).

## 6. wasm

`wf-wasm` (`src/core_api.rs`) exposes the session next to the extractor (plain `extern "C"`, no wasm-bindgen):
`wf_core_load_last` (game data from the last `wf_extract`) / `wf_core_load_tar`, `wf_core_reset(config JSON)`,
`wf_core_step(keys, n)` → events JSON, `wf_core_observe(player)` → JSON, `wf_core_snapshot` / `wf_core_restore`,
`wf_core_hash_lo/hi`, `wf_core_frame`. The frontend (`wf-app`) uses `Session` through `wf_view::Sim` directly.

## 7. Parity (all exact equality)

| suite | what | result |
|---|---|---|
| `tests/vectors.rs` | RNG; 78 ComputeDamage calls with RNG state; 3162 enemy hits are formula outputs; LevelUp | all |
| `tests/traces.rs` | player try-move on the map grids, enemy path follower (waypoints + positions), back-off / step-away / wander / straight-walk steps, knockback / critical launch / wall splat | all traces |
| `tests/lockstep.rs` | the emulator lockstep recordings (player of every form, all skills, hurt / KO / forms, enemy dummies, injected hits, lag frames): state, anim, frame, ROM record, position, HP/EP, form, RNG, sounds, enemy position / HP, damage per frame | 616 / 616 of the reference's pass set (633 traces, 17 known divergences), 244k frames |
| `tests/enemy_anim.rs` | 95 enemy animation recordings through `eanim.rs`; Yakon energy absorb | 78 926 ticks, 105 unmodelled (as the reference); all Yakon runs |
| `tests/cross_ts.rs` | the TS sim and wf-core on 22 recorded-input fights (bosses, groups, skills), per frame: RNG, camera, player, every enemy incl. AI stack, every projectile, entity list | 29 700 frames identical (dumps from `tools/battle/buu_core_dump.test.ts`, `WF_TS_DUMPS`) |
| `tests/session.rs` | determinism, clone + byte snapshot replay, view/observe read-only, 2-player co-op, native vs wasm hash | all |

Reference version: the port matches the TS sim at commit `b670c49` (enemy animation / SFX timing, decoded skill damage,
story-form timers, lag-frame-aware lockstep). When the reference changes, re-run its tests on a snapshot of the commit,
regenerate the cross-check dumps and port the diff (`git diff <old> <new> -- site/src/battle`).

## 8. Known gaps

What the reference does not model is not modelled here either: boss defeat cutscenes, cutscene scripts in AI states,
controllers whose length the game decides (shield / charge), the INFERRED nodes listed in `nodes::NODE_NOTES`. The view
adapter draws actors, shadows, halos, effects and most projectile records; beam piece chains, the Final Flash affine
pieces and the dark sphere disc are not drawn yet.
