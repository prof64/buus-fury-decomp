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
tools/wholerun/check.sh        # IS IT STILL EXACT? the whole real run, power-on -> credits, no lag (~1 min; exit 1 = new stop)
```

**`tools/wholerun/check.sh` is the top-level regression check**: the whole real run (1.67M rows) through the app
from power-on with no import, compared per logic iteration with the emulator (RNG, area, position, script depth,
story flags, sound requests); the known ranges of `data/realrun/known_stops.json` are skipped by restarting from a
cut after each. Details: segments.md §0.

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
  `GameData::load` directly. Data-driven for mods (docs/modding.md; with no mods every value is the built-in one):
  `GameData.fighters` / `arenas` (the `battle::FIGHTERS` / `ARENAS` tables, then `battle/fighters|arenas/*.json`;
  `build_record`, `arena_runtime`, spawn placement read these), `Skill.engine` (`skills/*.json` `engine`: `base`
  behaviour, `check`, `cost`, `damage`, `attack_sub`, `drain`, `anim`, Energy Blast frame data; `player.rs`
  dispatches on `GameData::skill_kind(id)` and reads the parameters through `skill_mult_of` / `skill_sub_of` /
  `skill_cost`, so a new skill id runs a built-in state machine with its own numbers), `FieldData` area files
  (`world/areas/*.json`: new areas, overrides, `add_objects` with automatic list indices), collision in row form,
  arena maps by folder id. All of it is static `GameData` (not hashed or snapshotted).
  **Scripted mod content** (`modkit.rs`, docs/modding.md §4b/§4c): `GameData.projectiles` (`projectiles/*.json`),
  `GameData.hooks` (`Arc<dyn modkit::ModHooks>`, the Rhai backend `wf-modscript`, attached by the frontend's
  `wf_app::battle::load_data`), skills with `engine.script` (kind `modkit::SCRIPTED`, player state
  `Gs::Skill(SCRIPT_GS)`), AI nodes `ModCondition` / `ModAction` (`CNode.call` / `args`), enemy types beyond the dense
  table (`GameData::enemy` falls back to a search; `anim_as`), story characters 9+ (`Story.chars` grows), area
  banners of mod areas. Runtime state: `Body::Mod(ModShot)` entities and `World.modk: ModState` (skill timers,
  cooldowns, per-actor script variables; snapshotted, hashed only when non-empty). A hook is a pure function of a
  `Ctx` returning commands the core executes; nothing runs without mod content.

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
| `tests/lockstep.rs` | the emulator lockstep recordings (player of every form, all skills, hurt / KO / forms, enemy dummies, injected hits, lag frames, 78 real enemies / bosses attacking incl. KOs and the game-over freeze, the 8 special holds, the Kamikaze Ghost's homing): state, anim, frame, ROM record, position, HP/EP, form, RNG, sounds, enemy position / HP, damage per frame | 703 / 703 = the reference's pass set (712 traces, the same 9 known divergences), 293k frames |
| `tests/enemy_anim.rs` | 95 enemy animation recordings through `eanim.rs`; Yakon energy absorb | 78 926 ticks, 105 unmodelled (as the reference); all Yakon runs |
| `tests/cross_ts.rs` | the TS sim and wf-core on 35 recorded-input fights (bosses, groups, skills, enemy attacks / holds: mummies, vampires, enzymes, fragments, Bio Mech, cyborgs, tank, turret, soldiers, gunmen + bomber, mechanoid), per frame: RNG, camera, player, every enemy incl. AI stack, every projectile, entity list | 46 800 frames identical (dumps from `tools/battle/buu_core_dump.test.ts`, `WF_TS_DUMPS`, default `engine/target/wf-core-ts-dumps`) |
| `tests/session.rs` | determinism, clone + byte snapshot replay, view/observe read-only, 2-player co-op, native vs wasm hash | all |

Reference version: the port matches the TS sim at commit `010de77` (lockstep round 3: every enemy-attack and special-hold
recording, game-over timing, the empty-hitbox rule, Kamikaze Ghost homing; the semantics are listed in
`tools/battle/LOCKSTEP.md` "Semantic changes for ports"). When the reference changes, re-run its tests on a snapshot of
the commit, regenerate the cross-check dumps and port the diff (`git diff <old> <new> -- site/src/battle`).

Port notes (round 3): a GotoState / PopState inside a tree does not stop the running tree - the replaced entries stay
addressable as `AiCtl::orphans` until the evaluation ends; nodes that work at tree construction (DriftContact's draws,
IfButtonMashed's counter) run in `nodes::build`; node-installed state objects with per-frame code are `enemy::Hook`;
the frame counter (`Globals::fc0`, clock = fc0 + frame) and the input object's press ring (`Globals::presses`, per
player slot) feed the escapes (freeze, bear hug, wraps, IfButtonMashed) and VampireGrab / LatchDrain's drains; the
sprite frame records' boxes (record +4..+7) come from the extractor (`sprites/*/sprite.json` step `box`); the game-over
freeze (KO + 199: modal player, message 0xF) applies to the one-player game only; a blocked hit's pre-block damage is
`Event::DamageRoll` (what the lockstep logs).

## 8. Known gaps

What the reference does not model is not modelled here either: boss defeat cutscenes, cutscene scripts in AI states,
controllers whose length the game decides (shield / charge), the INFERRED nodes listed in `nodes::NODE_NOTES`. The view
adapter draws actors, shadows, halos, effects and most projectile records; beam piece chains, the Final Flash affine
pieces and the dark sphere disc are not drawn yet.

**Uninitialised heap reads (by design, bounded).** The game reads heap bytes its objects never wrote at 18 code sites
over the whole real run (census: `segments.md` "Heap census"). Draw order, voice slots and redraw caches are not
modelled (no gameplay effect). The two gameplay sites with a fixed outcome are covered: the camera object reading a freed
actor's last position (`0x080077EC`) eases toward where that actor stood - ours holds the camera while no player
exists, and the 7 real-run segments with those reads match the game frame by frame (a rule that kept easing toward
the last target instead broke ~70 gates at loads: not added); a charging enemy beam accepts no message (its team word
is unset until the fire; `shots::on_message`, Rust + TS). **The one site whose value matters is not modelled: WanderToRandomPoint's path runtime** (`0x0803BD64` sets
speed, budget and valid = 0 only; the first step reads the heap block's leftover waypoint +0xC / +0x10 and re-plan
counter +0x18 - another object's bytes, different at each create). Its only user in the main story is Vegeta inside
Buu (20-6, state 0x313). A wander running at an import's cut takes those words from RAM (`wf-import` `import_ai_rt`,
the path followers' runtime); a wander created after the cut, or anywhere in a replay from power-on, starts from a
cleared runtime - so **a from-power-on run diverges from the game at Vegeta's first wander step in 20-6** (sub-pixel
positions, then the re-plan timing and the RNG). Reproducing it needs a byte-level model of every heap object (not
planned).

**Known whole-run stops (allow-list).** Differences accepted by design are listed in `data/realrun/known_stops.json`
(row range, stretch, reason, the `segments.md` write-up). `tools/wholerun/resync.py` logs a rule stop inside a listed
range as kind `known` and cuts the run again after the range's end; `board.py --rank` counts them apart from the rule
and import stops. Listed now: rows 1,339,017 .. 1,354,524 (062, Vegeta's wander above). A new entry needs the
coordinator's decision and a write-up first.

**Lag frames are not recreated (by design, 2026-10-04).** The game's main loop runs one iteration per logic frame and
an iteration that overruns a VBlank leaves video frames without logic (loads, text boxes, menu redraws, busy scenes).
The app runs a steady 60 fps: one logic frame per video frame. Recorded lag is injected only by the test harnesses
(segment harness, whole-run replay, soak) to line up with emulator recordings. `Options::sim_lag` (the sim's own
load / text-box lag lengths, `sndsim::load_frames` / `dialog_frames`, `Session::lag_step`) stays in the tree, off by
default and unused - deprecated, not deleted. Game logic does not observe lag: the VBlank handler (`0x0803D784`) calls
only the current mode's +0x10 hook and the system object's +0x18 hook (both `bx lr` in the field), the loop's load
meter (`[0x03000FEC]`, written by the VBlank wait `0x0803F3F4`) has no reader, the keys are read once per iteration
(`0x0803D4A0`, after the wait), and every gameplay clock is the iteration counter `0x03000F94` (`Globals::clock`),
which counts iterations, not video frames. Audit and the reader list: `segments.md` §9 "Does game logic observe lag".

**Mid-frame palette tearing is not recreated (by design, 2026-10-04).** The game rewrites the palette from its main
loop while the screen is being drawn - the fade routine `0x08007074` (512 colours, ~70 cycles each, ~29 scanlines)
and the IWRAM palette copy (pc `0x03000818..`), starting 30 k .. 90 k cycles after the VBlank depending on the
frame's work - so the lines drawn before or during the rewrite show the previous frame's palette, mixed per pixel on
the lines drawn while it runs. That is CPU time, which the app does not model (as the lag frames); the app renders
each frame with its new palette in full. The same holds for the BG scroll: the top lines can show the previous
frame's scroll (the registers written late in the frame). **The tear rule** (pixel comparison, `examples/pixels.rs` /
`tests/pixels.rs`): on a frame whose palette state (field fade / scale / BLDY, a menu's fade) or BG scroll state (the
cameras, the map's drift / shake) changed since the previous frame, a line is accepted when it equals our render with
the new state, the previous palette, the previous scroll or both previous; on a line drawn while the palette rewrite
runs, a pixel is accepted when it equals the new or the previous-palette picture. Such frames are reported as "torn
(hardware)", and the pixel gate locks them with per-line specs (`""` new, `"o"` previous palette, `"s"` previous
scroll, `"b"` both, else a 240-bit hex mask of the previous-palette pixels) next to the game picture's hash.
**Per-layer scroll** (`"s3"` / `"s2"` / `"s1"`, 2026-10-10): the game writes each map layer's scroll registers from
its main loop as it updates the layers, BG0 first - in a frame that takes long the writes fall inside the picture
(emulator, real route rkb_168 14-1 f37,530, the countdown's sphere scene: BG0VOFS at line 210, BG1VOFS at 221,
BG2VOFS at line 4 of the next picture, BG3VOFS at line 78), so the lines above a write show that layer and the ones
after it one step behind the rest. The candidates keep the map layers from that BG on at the previous scroll (a
layer offset of old - new scroll, `Renderer::bg_scrolls`), each also with the previous timer / sphere (`"s3t"`),
palette (`"s3o"`) or both (`"s3u"`); a per-pixel mask over such a base is written `"s3|mask"`. An animated tile's
new cel is uploaded from the main loop too (sb_041 21-25 f56,370: tiles 892 .. 895 written at line 71): the lines
above show the previous cel, which the previous-scroll candidates carry (the previous map clock).
The link fight's gate (`tests/dojo_pixels.rs`) applies the same rule to its recordings, checked against our own
pictures: the last lines still the previous picture, the HUD's / the friend's bars still the frame before's (their
tiles are drawn in the frame's draw pass), the lines above or below one split with the frame before's BG scroll.

**The screen shake on the OBJs** (`view::obj_shake_baked`, segments.md "the screen shake on the OBJs"): actors and
props move by ((camera + shake) >> 10) - (camera >> 10) of the side's own camera; **damage numbers too** (link
recording dojo_px1: a crit's number jumps with every shake step - the story's one "a pixel right" crit number, db6_007
f90, was this); the generic effect objects and the **Energy Blast** (class 0x0804B454: its draw check 0x08019BF4
places it from the camera's unshaken rect) do not. In a link fight every player's camera is held in the frame a shake
ends (`World::camera_held_by_shake_of`).

## 9. Coordination (in progress, 2026-10-02)

Two agents extend `World` concurrently; keep changes additive and re-read before editing.

**Byte snapshots are postcard (not self-describing)**: anything inside `World` must not use `#[serde(tag = ..)]`,
`untagged`, `flatten` or `skip_serializing_if` — they serialize but fail to restore (`tests/session.rs`,
`tests/field.rs` catch it). `#[serde(default)]` alone is fine.

* **Cutscene systems** (script-host work, module `scene.rs`, field `World.scene: scene::Scene`): per-entity command
  tracks + the camera track, the director wait, script camera control (follow / pan / set), screen effects (fades,
  flashes, mosaic, tints, BG layer / blend state, HUD visibility), dialog flow (text boxes, pages, choices, menus),
  boss bar, title text, scene requests (warps / scene loads / menus / title / credits, consumed by the field layer
  or the session). **`Body::Npc(Box<scene::Npc>)`** = a scripted actor (the game's NPC class created by
  `0x0801111C`, used by `spawn_actor`, `change_sprite`, and cutscene stand-ins). Field NPC behaviour (wander,
  talk check) should extend `scene::Npc` (e.g. a `behaviour` field) instead of adding a second NPC body kind.
  Touches outside `scene.rs`: `world.rs` (variant, field, update dispatch, hash), `battle.rs::post_step` (script
  camera before the follow camera), `view.rs` (Npc draw, dialog, fade), `lib.rs`.
* **Field state** (flags, inventory, party, areas / warps, map NPCs, pickups, triggers): the field agent. The
  scene layer only *requests* warps / scene loads (`Scene.requests`) and leaves area loading to the field code.
  Field agent's footprint: see §10 (modules `story.rs`, `fieldcontent.rs`, `field.rs`, `fieldnpc.rs`,
  `fieldobj.rs`; `World.story`, `World.field`; `Body::Pickup / Breakable / MapSprite`; map NPCs are `scene::Npc`
  with `behaviour: Some(field::Behaviour)`).

**scene.rs status**: done as described above. Frame split: `Session::step_world` (no camera) → script host →
`Session::step_post` (cameras). `Scene.song` follows the frame's music events at the end of `step_post` (byte
snapshots carry no events). AI `RunScript` / `InlineScript` nodes emit `FieldEvent::Run { cause: Cause::Ai }`
(`CNode.script` = header address / inline bytecode address).

**Round 2 (script host agent)**: view — `render_view` now provides the area banner (`Scene.banner`, rule of
docs/frontend.md §7), the field transition palette fade, the tile-animation timer from the area load, the greyed
skill icon in non-combat areas, and `RenderView.world_map` (script prop visibility, spawn_box mini-map box);
`view::Presenter` gives the one-frame OBJ lag and the world map's older BG camera (`Session.presenter`, used by
`impl Sim for Session` in field sessions; wf-app's `patch_view` / `BannerTracker` / `ObjLayer` are now
redundant). Field — a broken breakable hides its map prop; `field::refresh_triggers`, `respawn_breakables`,
`load_area_with(.., objects)`; `Cause::Ai`, `Cause::LevelUp`. Player — `Player.boost` (the timed buff's values).

## 10. Field game (`story.rs`, `fieldcontent.rs`, `field.rs`, `fieldnpc.rs`, `fieldobj.rs`)

Buu's Fury fights in the field: a field session is the same `World` with `World.field = Some(..)`; players and enemies
are the battle entities, the area's objects are added around them. Mechanics: re/buu/notes.md §11 and the "Field: ..."
sections; ground truth `data/field/*.json` (recorders `tools/field/rec_*.py`).

**Persistent state** `World.story: story::Story` (GameData `0x03001068`, layout `data/field/gamestate_layout.json`):
story flags (143 bytes, bit `f & 7` of byte `f >> 3`), script words `l` (0x03001020) and VM globals `g` (0x03001058),
`game_bits`, `variant_byte`, supply counts (u8[83], cap 255), the equipment list (cap 250), zenie, the nine
`CharState`s (record, form, skills + levels, selected skill, equipment slots, SS cooldown, flags bit 0 = in party),
`current` character, 40 quest slots (the game stores no quest progress: its conditions are flag scripts), play time in
frames. `Story::new_game(data)`; active fighters write their record back every field frame (`sync_from_players`). In
battle mode it is `Story::default()` and unused.

**Content** `GameData.field: Option<Arc<FieldData>>` (None without the pack's `world/` category): areas (maps per
variant, song, combat byte, on-enter / on-leave, objects in list order), maps (header exits, props, start position,
camera limits; collision decoded at area load), supplies, and the bytecode of the 115 spawn-condition scripts.

**Session**: `Session::new_field(data, FieldConfig { world, area, variant, pos, facing, chars, story, rng, xp, arrive })`
(`arrive` = start like an exit arrival: the load's RNG draws then the 32-frame fade-in); `Session::step` as in
battle. `Session.hook: Option<FieldHook>` = the script host's synchronous callback, invoked for every
`FieldEvent::Run` right after the phase that produced it (deferred calls at the start of the world update; triggers,
defeats, item-get ends after it), so loot scripts / trigger scripts draw RNG and spawn drops where the game does.
wasm: `wf_core_reset_field(json)`.

**Frame** (field): `scene::pre_step` → `field::pre_step` (an event / item-get holds the players' input; deferred
calls due) → enemy grid marks → `World::step_entities` (every mode; the world updates during fades) → `scene::tick` →
`field::post_step`: item-get timer, then by `Field.mode`: `Normal` → the trigger check (`0x080126D4`); `FadeOut{k}`
(32 frames, the player autowalking 0.5 px/frame, then idle + load in the same frame) / `FadeIn{k}` (32 frames, autowalk,
idle at the end); cameras (script camera first, then `field::camera`: the battle ease with the game's integer-part
clamp to `camera_max`); placed enemies' defeats; story sync; play time.

**Area load** `field::load_area(w, AreaId, x, y, facing)` (EnterArea `0x08008D44`): everything but the players goes;
map + collision; camera snap `(x - 112, y - 76) << 10`; map sprites, breakables, items; the trigger list (area
triggers, then map-header exits, in scan order); song; actors (enemies and NPCs, one rand(255) each); on-leave /
on-enter as `Run` events; the players re-created at the destination (2 x rand(255) each, after the actors in the list,
so they update first). Conditions: always / never / flag set / clear / set-consume / bytecode (`story::eval_condition`).
Enemies respawn on every load unless a flag gates them.

**Triggers**: point = player footprint top-left (integer px), `x0 <= px < x1`; first match fires, one per frame; no
check while busy / item-get / fading or at HP 0. Activations: touch (every frame inside), touch_once, A (pressed),
A + facing, on-entry (`0x08007791`), UP-held-once, A-once, altitude (no field altitude exists: the world-map player
only). Exits need the push key held; door: dest = def (x, y), facing = high nibble or push; edge: player - (x0, y0) +
(x, y), facing = push. Coins' touch triggers are scanned first.

**A press** (player melee hook): in combat areas the hit first; nothing accepted → interact message (talk NPC footprint
or A pickup under the facing box) → an A trigger under the point; any of them suppresses the punch. Areas with combat
byte 0 never send the hit and do not punch (INFERRED). Talk: NPC faces the player, SetAnim(0), event start (walking
NPCs and players go idle), `Run { Event(sNNN), Talk }`.

**Map NPCs** (`fieldnpc.rs`): `scene::Npc` + `Behaviour` (step runner `0x0800A95A`: stand, wait, face_player,
walk_to, wander / wander_rect variants; idle / walk animation; talk NPCs mark their footprint on the walk + enemy
grids, so `Collision.walk` is a per-World copy-on-write grid). While the field is busy (event, host attached) only
their animations tick.

**Objects** (`fieldobj.rs`): breakables (HP, attack mask, solid box, 23-frame shake with rand(5)/rand(3), break =
explosion + a call 16 frames later: loot position / velocity, grid restored, `Run { on_break, Break }`); coins
(`drop_zenie(size)`: 5 draws, 60 frames of bounce, 600 frames life, touch pickup, SFX 44); dropped items
(`drop_item(i)`, negative = equipment: 4 draws, bounce until landed, A pickup); map items (solid, A pickup); item-get
(SFX 45, 61 frames holding the players; equipment counted at once, supplies and the item's event after it).

**Events**: `Event::Field(FieldEvent::{AreaEnter, AreaLeave, Run { run: Callback, cause: Cause, slot }, Collected,
Defeated, ConditionUnknown, Transition})`. **Host API**: `field::warp`, `load_area`, `spawn_map_object(ObjRef,
force)`, `set_busy` / `attach_host` (blocking events hold the field until the host clears it), `fieldobj::set_loot /
drop_zenie / drop_item`, `fieldnpc::start_event`. **Observation**: `Observation.field` (area, objects with rects and
exit destinations, inventory, party, play time).

**Verification** (`tests/field_lockstep.rs`, emulator recordings): transitions (door / edge exits, round trip, arrival
facing; load RNG, autowalk, fades, camera, enemy positions) 4 traces; NPCs 10 traces (wander / wander_rect / walk_to /
wait / talk / blocking / crowds, 11k frames: positions, facing, animation frame, RNG); objects 6 traces (touch_once
event trigger, A script trigger, save point, map item + item-get, breakable break + loot call, dropped supply /
equipment physics and A pickups). Known divergences: `transitions_forest_up_exit` from frame 257 (the battle sim's
enemy walk→idle one frame early, inherited from the reference), `npc_capsule_walk` from 136 (4-20 v2 live collision ≠
content). `tests/field.rs`: conditions, exits, determinism + clone / byte snapshots, story.

**Round 2** (`fieldpatrol.rs`, `fieldworld.rs`, `fieldview.rs`):
* *Enemy patrols*: field enemies carry their def +0x18 step runner (`Field.patrols`, never reset); the AI node
  `RunMotionController` runs one pass per update (enter nop, exit = the step's pause hook); enemy walk_to / wander /
  wander_rect / `cutscene_move` (3D, world-map encounters) on the enemy layer. Enemies are built constructor first
  (idle rand(255), runner) then their AI state.
* *World map* (map header "3D" flag = area_00_050): the flying player (`Field.flight`: altitude 22.10, heading, speed,
  turn; A/B altitude, UP/DOWN thrust, LEFT/RIGHT turn, wrap 3584 x 2560), the 3D camera (`Flight.cam`, exposed as
  `MapView.mode7`), the 64-frame arrival fade with control, altitude triggers on the flyer's z, message triggers =
  the bottom label (`Field.label`, `Mode7View.label`), encounters (`IfPlayerOnTop`: px distance <= 16 and |dz| < 16 px;
  `TriggerSelf` in the field = `field::trigger_self`: defeated event + on-defeat script, object kept). Landing and the
  capsule take-off are scripts (host). Visited record: loading an area sets story flag `0x1C0 + bit` from
  `world/visited.json` (table 0x0874CA24). The map screen (SELECT, a room map of visited rooms) is UI: no core state
  beyond the flags; travel = exits, capsule flights, scripted warps (nothing else exists in the game).
* *Collision*: breakables mark the grids only when the prop is solid; `open_area` / `block_area` set walk + enemy
  layer, the `_alt` forms the shot grid (scene.rs); on-enter scripts must run inside the load:
  `Session::new_field_hooked(data, cfg, hook)` runs them before frame 0 (the hook also runs after every load).
* *Host API added*: `field::actor_by_tag(w, tag)` (map placement actor ids -(tag+1)), `fieldpatrol::set_z / z_of`
  (native 0xF0 `set_actor_field_1a8`: obj+0x1A8 = v << 10, positive = up), `field::trigger_self`.
* *View* (`fieldview.rs`, content from `wf-extract/src/field_gfx.rs`): coins (`effects/coin_s`, cel from the phase,
  bounce, shadows), dropped / map items (`effects/field_items`), props and breakables (`effects/props_<map>`, shake),
  level signs (`effects/field_labels`, bob), debris chips, the item-get icon and hold pose; breakable hit boxes and
  dropped items' collision sub-boxes come from the graphics (content `hit_box`, `icons/index.json sub_box`).
* *Verification*: `tests/field_lockstep.rs` 25 recordings, 24 exact (+ world map flight 593 frames, landing, an
  encounter; enemy patrols 1500 frames; Capsule Corp fixed: the recorder had the variant wrong); known: one
  battle-AI animation detail in `npc_enemy_patrol` (frame 2). `tests/field.rs`: view draws, visited flags, flyer.

**Gaps**: breakable boxes approximate the prop graphic's pixel bounds (prop rect lower half, 1 px inset); dropped
items' collision sub-box (icon pixels) likewise; coin scatter verified only through the loot script (needs the
script host); enemies' def behaviour (`RunMotionController` node) not run; enemy loot position INFERRED (their position,
no velocity); debris chips and prop / pickup / map-sprite graphics are not drawn by the view; map-screen visited bits
(table `0x0874CA24`) not set; no variant selector exists in the ROM. Round 2 remaining: the area-name banner and the world map's 94 billboard props are not drawn; the mode-7 ground must be rendered by the frontend from `MapView.mode7`; debris motion past the start INFERRED; the event freeze after an encounter / landing is the host's.

**Frontend requests (field render parity, docs/frontend.md §6 "Field parity")**: (1) put the dialogue box
animation in `render_view` — `RenderView.dialog_anim` (`wf_view::DialogAnim { scale 1..8, typed }`; the mapping the
frontend uses meanwhile is `wf_app::story::dialog_anim`: Opening t -> scale min(t + 2, 8) with typed 0, Shown ->
8 / `typed`, Closing t -> 8 - t, hidden below 1; OBJ-lagged like the box, `typed` from the current state); (2) the
item-get icon is drawn one frame longer than the game (objects_item_8_57 frame 65); (3) the camera keeps easing
during the breakable hit-stop, the game holds it (objects_breakable_drops_8_1 frames 4..18); (4) Hercule City 3-3's
wandering NPC is not placed (transitions_hercule_edge_up frames 58..80).
