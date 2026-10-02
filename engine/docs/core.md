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

**Gaps**: breakable boxes approximate the prop graphic's pixel bounds (prop rect lower half, 1 px inset); dropped
items' collision sub-box (icon pixels) likewise; coin scatter verified only through the loot script (needs the
script host); enemies' def behaviour (`RunMotionController` node) not run; enemy loot position INFERRED (their position,
no velocity); debris chips and prop / pickup / map-sprite graphics are not drawn by the view; map-screen visited bits
(table `0x0874CA24`) not set; the world map 0-50 flying player is not modelled; no variant selector exists in the ROM.
