# Script host ↔ wf-core: the integration seam

How Buu's Fury scripts (spec `re/buu/SCRIPT.md`, runtime `docs/scripting.md`) drive the deterministic simulation
core `wf-core`. Code: `crates/wf-script-core` (adapter; depends on `wf-core` and `wf-script` — the dependency
points script → core, core never sees scripts).

## 1. Status (2026-10-02, round 2)

All **248** natives are bound (`wf_script_core::binding::binding(n)`, table in §8): VM 16, core 32, field 22,
scene 112, story 53, **partial 13**. Partial = a model or a picture the frontend shows, not frame-exact:
* `fire_wave_beam` 0x3B, shots 0x3E / 0x41 / 0x42: timing and animations lockstep-exact; the projectile / wavy beam
  is a scene `Visual` (the game's objects could hit things; not verified, so not spawned as shots);
* `prop_poof` 0x62: timing and vanish exact, the dust frames are not drawn;
* save / character menus 0x8C / 0x8D, mini-games 0xA7 / 0xC6 / 0xC7, credits 0xD1: deterministic models (the
  screens are not reverse-engineered);
* broadcasts 0xC9 / 0xDC: the receivers are not identified (`scene::Request::Broadcast`).

Made exact in round 2: cutscene beams 0x46 / 0x5C / 0x5E use the game's beam object (charge effect,
`shots::KameBeam` grows while on screen, retracts, its sounds); tint 0x52 picks from the full 20-entry table;
props 0x63-0x69 drive the area's map props (`Field.props`: visibility + rect; jump arc in 1/256 px);
refresh_map_triggers 0x8B / respawn_breakables 0xAD (`field::refresh_triggers` / `respawn_breakables`);
give_xp 0xC2 runs the level-up hook #3 inline (`NativeResult::RunEvent`, both runtimes) and kill-XP level-ups in
a hosted field queue #3 (`Cause::LevelUp`); boost_stats 0xF2 / start_timed_special 0xF7 set the timed buff
(`Player.holy_left` + `Player.boost`, `progression::recalc_stats_with`); 0xF0 is the height (field).

Systems (`wf-core/src/scene.rs`, docs/core.md §9): actor command tracks, director, script camera, screen effects,
dialog, menus, boss bar, music dedupe, spawn / despawn (`Body::Npc`), form changes, battle starts, warps, scene
loads (players offstage: `Scene.offstage`), area banner (`Scene.banner`). Enemy AI `RunScript` queues its event
(`header_0xADDR`, `wf_script_rhai::Runtime::event_id`); `InlineScript` queues its bytecode and the node waits
(`Scene.ai_wait`) until the host calls `scene::ai_script_done` (wf-app StorySim does).

## 2. Execution: blocking on core's frame model

* A script thread (`wf_script::vm::Thread` for the original bytecode, `wf_script_rhai::ThreadState` for Rhai) runs
  **inside one simulation step** until it finishes or a native/node **blocks**.
* A blocking call stores a [`Pending`] in `StoryState` and returns `NativeResult::Block`:
  * `Frames { left, result }` — `wait(n)`, item-get popups (60 f), and in the future every timed action;
  * `Button` — `wait_button()`, text boxes: done on the leader's A press (`World.input[0].pressed & A`);
  * `Choice { options, cursor }` — choice nodes: UP/DOWN move, A confirms; the index goes to
    `StoryState.menu_result` (read by `get_menu_result()`);
  * future: `Track { actors }` — done when every listed actor's command track is idle (`end_parallel`, and every
    queued actor command outside a batch); `ScreenFx`, `Camera`, `Menu`, `Warp` (§6).
* Frame order: `Session::step_world` (input, objects, `scene::tick`: screen, dialog, menus, command tracks) →
  the host polls / resumes blocked scripts and starts queued ones → `Session::step_post` (cameras, field / battle
  rules). This is the game's order (a script resumes from its wait mode before the frame's camera update);
  `Session::step` = both halves with no scripts.
* Each frame, after `Session::step_world` (so the World advanced and the input of this frame is visible):
  `if let Some(v) = wf_script_core::poll(&world, &mut story) { thread.resume(v, …) }`. `poll` also ticks the
  script timers. The resume runs the script until its next block in the same frame — exactly the GBA, where the
  nested main loop returned and the script continued at once.
* The director batch (`begin_parallel` … `end_parallel`): `StoryState.director.batch`; queued commands block
  only when it is off.
* While a script blocks, the World keeps simulating (the GBA's nested loop ran the world update too). Modal
  presentation (dialog freezes enemies on the GBA) is the frontend's/core's `World.modal` decision.

## 3. Where script state lives (snapshots, rollback)

| state | where | snapshot |
|---|---|---|
| simulation (players, enemies, RNG, input, cameras) | `wf_core::World` | `World::clone` / `snapshot::to_bytes` |
| story state: flags, inventory, zenie, party, timers, director, pending action, drops, presentation queue | `wf_script_core::StoryState` | `Clone + Hash + Serialize` |
| running threads | `wf_script::vm::Thread` (exact VM) / `wf_script_rhai::ThreadState` (Rhai) | `Clone + Serialize` |
| script globals G0..G3 | `wf_script::vm::Globals` | `Clone + Serialize` |
| compiled scripts / event graphs | `ScriptSet` / `wf_script_rhai::Library` (static, shared) | not snapshotted |

Composition: a game session = `{ core: Session, story: StoryState, threads: Vec<Thread>, globals: Globals }`;
snapshot = clone of all four; state hash = core hash ⊕ hash(story, threads). Nothing script-side keeps hidden
state (Rhai runs integer-only, no clock, no globals between calls). Proposed: core's `Snapshot` gains an
`ext: Vec<u8>` (postcard) so the layer above can ride along in byte snapshots, or the game session owns both
(no core change needed for in-process rollback).

## 4. RNG

`rand(n)` / `chance(pct)` use **`World.rng`** (the game PRNG `0x0803FAAC`, already in core with the exact
`rand`/`chance` formulas; `chance` gets `pct * 0x28F5D >> 8`). Scripts and combat share one stream, as on the GBA
(loot rolls interleave with damage rolls). Verified: the loot script run by the VM and by Rhai on the core host
leaves identical drops and identical `World.rng` states (`tests/events.rs`).

## 5. Dispatch: what runs which script (ids as in `annotations/script_catalog.json` and wf-world's `objects.json`)

| trigger | script id | params | runner | core hook (status) |
|---|---|---|---|---|
| NPC talk (A facing an NPC, object type `0x08017DA3`, rec+0xC) | event `sNNN` | npc | whole event (`Thread::new_event`) | MISSING: field NPCs + talk check |
| map trigger with inline code (`0x08007387`, rec+0x10) | `inline_0xADDR` | object block | unit | MISSING: trigger rects in World |
| map trigger running an event (`0x0800742D`) / fixed event #11 | `sNNN` | object block | event, blocking | MISSING |
| area on-enter / on-leave (area +0x30/+0x34) | `inline_0xADDR` | zero | unit | MISSING: area loading |
| spawn / visibility conditions (cond word or code) | `inline_0xADDR` | — | unit, result `stack[0] != 0` | MISSING: map objects |
| enemy on-defeat callback `{0x0800424F, event}` (boss handovers: #450) | `sNNN` | — | event | needs core `Event::Defeated { entity, record }` + the callback pair in `EnemyType` |
| enemy drop script (enemy type +0x18) | `inline_0xADDR` → `run_script(497, level, item)` | zero | unit | needs `Event::Defeated` with position → `StoryState.loot_pos` |
| item use (supply +0x30) | `inline_0xADDR` | zero | unit | needs an item menu in core/ui; adapter-tested |
| equipment effect (+0x28) | `inline_0xADDR` | zero | unit | core already folds these effects from content (`recalc_stats`); scripts only needed for modded equipment |
| per-melee-hit hook #2, level-up #3, death #0 | `s002`, `s003`, `s000` | P per hook | unit | needs core events `Hit`, `LevelUp`, `PlayerDown` carrying the record |
| AI RunScript / InlineScript nodes (#121, #474–476, Vegeta 0x88) | `sNNN` / inline | actor+0x214 | event / unit | core `nodes.rs` must emit `Event::RunScript { id, actor }` instead of skipping |
| quest log conditions (40 × 3) | `inline_0xADDR` | zero | unit, bool | UI |

The engine resolves ids through `ScriptSet` (`run_script_target(idx)`, `ScriptId::Inline(addr)`) or, for Rhai,
the exported files `scripts/events/sNNN.rhai`, `scripts/inline/inline_0xADDR.rhai`.

## 6. Multiplayer mapping

* Actor 0 = **the leader** = `World.players[0]` (the GBA's player). All original scripts address the leader;
  co-op partners are not touched by them (except `heal_party`, which heals every `World.players` fighter).
* Actor 1..8 = the fighter currently playing character id−1 (any slot); ≥ 10 = first live entity with that
  sprite; negative ids (map placements) need map objects (MISSING).
* `player(n)` (1-based) = `World.players[n-1]` (`wf_script_core::player`); `player_count()` =
  `World.players.len()`. Per-player events pass the triggering slot (`on_talk(who)`, `on_defeat(by)`, …).
* Input-dependent natives (`a_pressed`, `wait_button`, text boxes, choices) read the **leader's** input; for
  co-op dialogs any player may advance (frontend option, deterministic because inputs are part of the
  synchronized InputFrame array).
* Story flags, inventory, zenie, globals are shared by all players.

## 7. Requests to wf-core

All answered (2026-10-02): 1-3, 6 by the field agent (§10); 4, 5, 7, 9, 10 by `scene.rs`; 8 by `FieldEvent::Run`
with `Cause::Ai` (no `Hit`/`LevelUp`/`PlayerDown` hooks yet); 11 kept in `HostState.boost` (core applies only the
Holy Water bonus); 12 not needed (host state is plain data next to the snapshot); 13 exists. Original list:


1. **Field state in `World`** (or a sibling `Story` struct owned by the game session): flags, supplies,
   equipment list, zenie, party roster with all 9 `Record`s and per-character forms/skills/skill levels/equipment.
   Until then the adapter's `StoryState` holds them.
2. **`Event::Defeated { entity, ty, x, y, by }`** (enemy death with position) so on-defeat events and drop
   scripts can be dispatched; `EnemyType` should carry the drop script address and the on-defeat callback.
3. **`Body::Pickup { kind, x, y }`** with collection by players (zenie / supply / equipment) to realise
   `StoryState.drops`.
4. **Scripted actors and command tracks**: `Body::Npc(Npc { a: Actor, track })`, `Track { queue: Vec<Cmd> }` on
   Npc/Player/Enemy, `Cmd` = walk/glide/jump/move/face/anim/pose/shoot/beam/emote/wait/remove/change_sprite/
   become_map_object/…, `Cmd::update(&mut World, entity) -> bool done`; `World::tracks_idle(&[EntityId])`.
   Command semantics are in `re/buu/SCRIPT.md` §B3.5 and `annotations/script_natives.json`.
5. **Camera control**: `Camera.follow: Option<EntityId>`, scripted pans (`MoveTo { x, y, speed }`) with a done flag.
6. **Area loading / warps**: `World::warp(world, map, variant, x, y)` (3rd argument is the map variant), map
   objects of the area from wf-world `objects.json` (`spawn_map_object(index, force)`), collision overrides
   (`Collision::set_rect`).
7. **Presentation state** (data, rendered by the frontend): screen effect timer (fade/white/mosaic/flash with
   length), HUD visibility, boss bar target, BG layer visibility/priority/blend, title text overlay.
8. **Events for script hooks**: `Event::RunScript { id, actor }` from AI RunScript/InlineScript nodes; `Hit`
   with the record for hook #2; `LevelUp`; `PlayerDown`.
9. **The game's sine table** (`fixed::sin_q`) for natives 0xB3/0xB4.
10. `Event::SfxStopAll`, `Event::SongStop`.
11. Timed stat boost values (`boost_stats(seconds, str, pow, end, spd)`): `EffStats` applies a fixed +25 Holy
    Water bonus; scripts pass arbitrary values.
    **Its count** (GameData +0x4DC, 60 a second) is the field update's, in the routine that counts the countdown and
    the stopwatch (`0x0800425C`): the host counts it (`StoryHost::step_inputs`, the leader's `holy_left`; the host
    state's `boost` holds it while no leader object exists) in every frame that ends with the field free - no event,
    no item-get pose, no exit / arrival fade (the frame such a hold ends in counts, the one it starts in does not) -,
    not under a menu (the frame START opens it counts, the frame its close ends in does not) nor while Instant
    Transmission's target is chosen. `set_character` (0x95) and the Switch Character screen's choice clear it (the
    same character chosen again too). Gate: wf-import `tests/holy_timer.rs` - the game's count along 550 segments
    replayed on the emulator with the count poked in (`tools/segments/holy.py` -> `data/segments/timers/holy.json`),
    824,000 rows, all but two segments exact (the file's `known`). The countdown follows the menu rule too.
12. Optional: `Snapshot.ext: Vec<u8>` for byte snapshots of the layers above core.
13. `docs/core.md` (referenced by `wf-core/src/lib.rs`) does not exist yet.

## 8. Native → system table

Generated by `cargo run -p wf-script-core --example coverage`.

| id | native | kind | where | system / note |
|---|---|---|---|---|
| 0x00 | `G0` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x01 | `G1` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x02 | `G2` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x03 | `G3` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x04 | `P0` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x05 | `P1` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x06 | `P2` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x07 | `P3` | query | VM | VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3 |
| 0x08 | `get_l0` | query | story | World.story.l[i] (0x03001020) |
| 0x09 | `get_l1` | query | story | World.story.l[i] (0x03001020) |
| 0x0A | `get_l2` | query | story | World.story.l[i] (0x03001020) |
| 0x0B | `get_l3` | query | story | World.story.l[i] (0x03001020) |
| 0x0C | `get_hp` | query | core | leader Player.rec.hp / .max_hp |
| 0x0D | `get_max_hp` | query | core | leader Player.rec.hp / .max_hp |
| 0x0E | `get_energy` | query | core | leader Player: SS3 gauge / SS gauge / EP by form |
| 0x0F | `get_max_ep` | query | core | leader Player.rec.max_ep |
| 0x10 | `get_zenie` | query | story | World.story.zenie |
| 0x11 | `get_variant_byte` | query | story | World.story.variant_byte |
| 0x12 | `get_world` | query | field | World.field.area (world, area, variant) |
| 0x13 | `get_map` | query | field | World.field.area (world, area, variant) |
| 0x14 | `get_map_variant` | query | field | World.field.area (world, area, variant) |
| 0x15 | `get_character` | query | core | leader's character (form -> char) |
| 0x16 | `get_form_name` | query | core | string handle (text.rs) resolved by formatted text nodes |
| 0x17 | `get_form_label` | query | core | string handle (text.rs) resolved by formatted text nodes |
| 0x18 | `get_menu_result` | query | scene | HostState.menu_result, set when a choice window closes |
| 0x19 | `rand` | query | core | World.rng (game PRNG 0x0803FAAC) |
| 0x1A | `chance` | query | core | World.rng (game PRNG 0x0803FAAC) |
| 0x1B | `get_flag` | query | story | World.story flags |
| 0x1C | `not_flag` | query | story | World.story flags |
| 0x1D | `set_flag` | immediate | story | World.story flags |
| 0x1E | `clear_flag` | immediate | story | World.story flags |
| 0x1F | `play_sfx` | immediate | core | Event::Sfx / SfxStop |
| 0x20 | `play_sfx_volume` | immediate | core | Event::Sfx / SfxStop |
| 0x21 | `stop_sfx` | immediate | core | Event::Sfx / SfxStop |
| 0x22 | `stop_all_sfx` | immediate | core | Event::SfxStopAll |
| 0x23 | `play_music` | immediate | core | Event::Song |
| 0x24 | `stop_music` | immediate | core | Event::SongStop |
| 0x25 | `show_bg` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x26 | `hide_bg` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x27 | `set_bg_priority` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x28 | `set_blend_targets` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x29 | `set_bg_offset` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x2A | `set_blend_alpha` | immediate | scene | scene.screen.layers (BG visibility, priority, blend targets, offset, alpha) |
| 0x2B | `open_area` | immediate | scene | scene::set_area: collision overrides (copy-on-write grids, re-applied on restore) |
| 0x2C | `block_area` | immediate | scene | scene::set_area: collision overrides (copy-on-write grids, re-applied on restore) |
| 0x2D | `open_area_alt` | immediate | scene | scene::set_area: collision overrides (copy-on-write grids, re-applied on restore) |
| 0x2E | `block_area_alt` | immediate | scene | scene::set_area: collision overrides (copy-on-write grids, re-applied on restore) |
| 0x2F | `show_hud` | immediate | scene | scene.hud_hidden |
| 0x30 | `hide_hud` | immediate | scene | scene.hud_hidden |
| 0x31 | `camera_center_on` | modal | scene | scene::start_scroll: modal camera scroll |
| 0x32 | `camera_scroll_to` | modal | scene | scene::start_scroll: modal camera scroll |
| 0x33 | `camera_follow` | immediate | scene | scene.cam.mode Follow / Fixed |
| 0x34 | `camera_release` | immediate | scene | scene.cam.mode Follow / Fixed |
| 0x35 | `spawn_actor` | immediate | scene | scene::spawn_npc: Body::Npc |
| 0x36 | `spawn_map_object` | immediate | field | field::spawn_map_object (area lists 1 / 0 / 3) |
| 0x37 | `count_actors` | query | scene | objects with that script sprite id |
| 0x38 | `begin_parallel` | immediate | scene | director batch / wait_for_actor group |
| 0x39 | `wait_for_actor` | immediate | scene | director batch / wait_for_actor group |
| 0x3A | `end_parallel` | modal | scene | director commit: block until the batch (group) is idle |
| 0x3B | `fire_wave_beam` | enqueue | partial | Cmd::Beam (wave): timing and actor animation checked; the wavy beam 0x080286D4 is a scene Visual |
| 0x3C | `play_anim` | enqueue | scene | Cmd::PlayAnim / SetAnim / SetPose / PlayAnimFrames on the actor state animation |
| 0x3D | `set_anim` | enqueue | scene | Cmd::PlayAnim / SetAnim / SetPose / PlayAnimFrames on the actor state animation |
| 0x3E | `fire_ball_at` | enqueue | partial | Cmd::Shoot: timing and animation checked; the projectile is a scene Visual |
| 0x3F | `puff` | enqueue | scene | Cmd::Puff: fx::puff |
| 0x40 | `puff_offset` | enqueue | scene | Cmd::Puff: fx::puff |
| 0x41 | `shoot_at_actor` | enqueue | partial | Cmd::Shoot: timing and animation checked; the projectile is a scene Visual |
| 0x42 | `shoot_at` | enqueue | partial | Cmd::Shoot: timing and animation checked; the projectile is a scene Visual |
| 0x43 | `hit_flash` | enqueue | scene | Cmd::HitFlash |
| 0x44 | `walk_to` | enqueue | scene | Cmd::MoveTo (step math 0x080192CE) |
| 0x45 | `jump_to` | enqueue | scene | Cmd::JumpTo |
| 0x46 | `fire_beam` | enqueue | scene | Cmd::Beam: charge effect + the game's beam object (shots::KameBeam: grows on screen, retracts, its sounds); lockstep-exact |
| 0x47 | `face_actor` | enqueue | scene | Cmd::FaceActor (holds) / Face |
| 0x48 | `puff_at` | enqueue | scene | Cmd::Puff: fx::puff |
| 0x49 | `glide_to` | enqueue | scene | Cmd::GlideTo |
| 0x4A | `actor_wait` | enqueue | scene | Cmd::Wait |
| 0x4B | `queue_sfx` | enqueue | scene | Cmd::Sfx |
| 0x4C | `screen_flash` | enqueue | scene | Cmd::ScreenFlash: scene.screen.flash |
| 0x4D | `remove_actor` | enqueue | scene | Cmd::Remove |
| 0x4E | `change_sprite` | enqueue | scene | Cmd::ChangeSprite: new Npc, the queue moves to it |
| 0x4F | `become_map_object` | enqueue | field | Cmd::BecomeMapObject: field::spawn_map_object(actor list) at the stand-in's place |
| 0x50 | `move_to` | enqueue | scene | Cmd::MoveTo (step math 0x080192CE) |
| 0x51 | `set_pose` | enqueue | scene | Cmd::PlayAnim / SetAnim / SetPose / PlayAnimFrames on the actor state animation |
| 0x52 | `tint` | enqueue | scene | Cmd::Tint: the nearest of the 20 presets of table 0x08752EEC (Npc.tint = entry; no palette = restore) |
| 0x53 | `face` | enqueue | scene | Cmd::FaceActor (holds) / Face |
| 0x54 | `set_actor_speed` | enqueue | scene | Cmd::SetSpeed / SetAttachment / SetByte8 (Npc fields) |
| 0x55 | `set_attachment` | enqueue | scene | Cmd::SetSpeed / SetAttachment / SetByte8 (Npc fields) |
| 0x56 | `set_actor_byte8` | enqueue | scene | Cmd::SetSpeed / SetAttachment / SetByte8 (Npc fields) |
| 0x57 | `shake_screen` | enqueue | scene | Cmd::Shake: World::shake + track wait |
| 0x58 | `show_item_get` | enqueue | scene | Cmd::ItemGet (60 frames, SFX 45, hold-up pose) + World.story inventory |
| 0x59 | `play_anim_frames` | enqueue | scene | Cmd::PlayAnim / SetAnim / SetPose / PlayAnimFrames on the actor state animation |
| 0x5A | `emote_wait` | enqueue | scene | Cmd::Emote (balloon Visual) |
| 0x5B | `emote` | enqueue | scene | Cmd::Emote (balloon Visual) |
| 0x5C | `fire_beam_alt` | enqueue | scene | Cmd::Beam: charge effect + the game's beam object (shots::KameBeam: grows on screen, retracts, its sounds); lockstep-exact |
| 0x5D | `set_pos` | enqueue | scene | Cmd::SetPos |
| 0x5E | `fire_blast` | enqueue | scene | Cmd::Beam: charge effect + the game's beam object (shots::KameBeam: grows on screen, retracts, its sounds); lockstep-exact |
| 0x5F | `walk_to_actor` | enqueue | scene | Cmd::MoveTo (step math 0x080192CE) |
| 0x60 | `walk_to` | enqueue | scene | Cmd::MoveTo (step math 0x080192CE) |
| 0x61 | `walk_to_speed` | enqueue | scene | Cmd::MoveTo (step math 0x080192CE) |
| 0x62 | `prop_poof` | enqueue | partial | prop track command on the area's map prop: timing and the vanish exact; the dust frames (0x087581FC) are not drawn |
| 0x63 | `prop_hide` | enqueue | scene | prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px) |
| 0x64 | `prop_show` | enqueue | scene | prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px) |
| 0x65 | `prop_effect_at` | enqueue | scene | Cmd::Puff: fx::puff |
| 0x66 | `prop_move_to` | enqueue | scene | prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px) |
| 0x67 | `prop_jump_to` | enqueue | scene | prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px) |
| 0x68 | `prop_wait` | enqueue | scene | Cmd::Wait |
| 0x69 | `prop_set_pos` | enqueue | scene | prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px) |
| 0x6A | `camera_follow_wait` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x6B | `camera_pan_to` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x6C | `camera_release_wait` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x6D | `camera_set_param8` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x6E | `camera_set_param12` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x6F | `camera_set` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x70 | `camera_set3` | enqueue | scene | camera track commands (follow / pan / release / set) |
| 0x71 | `save_field_on_next_scene` | immediate | field | saved field + return (area reload, camera) |
| 0x72 | `load_scene_fade` | modal | field | scene loads: fades + field::load_area + fixed camera |
| 0x73 | `load_scene` | immediate | field | scene loads: fades + field::load_area + fixed camera |
| 0x74 | `load_scene_fade_noobj` | modal | field | scene loads: fades + field::load_area + fixed camera |
| 0x75 | `return_to_field` | modal | field | saved field + return (area reload, camera) |
| 0x76 | `warp_fade` | modal | field | field::warp (transition with fades for 0x76) |
| 0x77 | `warp` | immediate | field | field::warp (transition with fades for 0x76) |
| 0x78 | `fade_out` | modal | scene | scene::start_fade (modal) |
| 0x79 | `fade_in` | modal | scene | scene::start_fade (modal) |
| 0x7A | `fade_out_white` | modal | scene | scene::start_fade (modal) |
| 0x7B | `fade_in_white` | modal | scene | scene::start_fade (modal) |
| 0x7C | `mosaic_out` | modal | scene | scene::start_fade (modal) |
| 0x7D | `mosaic_in` | modal | scene | scene::start_fade (modal) |
| 0x7E | `flash_white` | modal | scene | scene::start_fade (modal) |
| 0x7F | `item_count` | query | story | World.story.supplies (u8 counters) |
| 0x80 | `give_item` | enqueue | scene | Cmd::ItemGet (60 frames, SFX 45, hold-up pose) + World.story inventory |
| 0x81 | `add_item` | immediate | story | World.story.supplies (u8 counters) |
| 0x82 | `take_item` | immediate | story | World.story.supplies (u8 counters) |
| 0x83 | `take_items` | immediate | story | World.story.supplies (u8 counters) |
| 0x84 | `drop_zenie` | immediate | field | field::spawn_pickup at the loot position (battle mode: HostState.drops) |
| 0x85 | `drop_item` | immediate | field | field::spawn_pickup at the loot position (battle mode: HostState.drops) |
| 0x86 | `drop_equipment` | immediate | field | field::spawn_pickup at the loot position (battle mode: HostState.drops) |
| 0x87 | `drop_zenie_at` | immediate | field | field::spawn_pickup at the loot position (battle mode: HostState.drops) |
| 0x88 | `wait` | modal | scene | modal wait (Pending::Frames) |
| 0x89 | `wait_button` | modal | scene | modal: leader's A press |
| 0x8A | `spawn_trigger_force` | immediate | field | field::spawn_map_object (area lists 1 / 0 / 3) |
| 0x8B | `refresh_map_triggers` | immediate | field | field::refresh_triggers (area triggers by condition + exits) |
| 0x8C | `save_menu` | modal | partial | scene::Menu Save / Character (deterministic model, not frame-checked) |
| 0x8D | `character_menu` | modal | partial | scene::Menu Save / Character (deterministic model, not frame-checked) |
| 0x8E | `return_to_title` | immediate | scene | scene::Request ReturnToTitle / Sleep (session level) |
| 0x8F | `show_boss_bar` | immediate | scene | scene.boss_bar |
| 0x90 | `show_boss_bar_ex` | immediate | scene | scene.boss_bar |
| 0x91 | `hide_boss_bar` | immediate | scene | scene.boss_bar |
| 0x92 | `equipment_count` | query | story | World.story.equipment |
| 0x93 | `give_equipment` | enqueue | scene | Cmd::ItemGet (60 frames, SFX 45, hold-up pose) + World.story inventory |
| 0x94 | `take_equipment` | immediate | story | World.story.equipment |
| 0x95 | `set_character` | immediate | story | switch_character: story record -> leader Player |
| 0x96 | `in_party` | query | story | World.story.chars[c].flags |
| 0x97 | `join_party` | immediate | story | World.story.chars[c].flags |
| 0x98 | `leave_party` | immediate | story | World.story.chars[c].flags |
| 0x99 | `set_char_flag2` | immediate | story | World.story.chars[c].flags |
| 0x9A | `clear_char_flag2` | immediate | story | World.story.chars[c].flags |
| 0x9B | `is_super_saiyan` | query | core | leader form SS / SS3 |
| 0x9C | `char_sprite` | query | story | character's current form gfx / set by sprite |
| 0x9D | `set_char_form` | immediate | story | character's current form gfx / set by sprite |
| 0x9E | `spawn_marker_a` | immediate | scene | scene.visuals (markers, box, map marker, energy sphere) |
| 0x9F | `spawn_marker_b` | immediate | scene | scene.visuals (markers, box, map marker, energy sphere) |
| 0xA0 | `spawn_box` | immediate | scene | scene.visuals (markers, box, map marker, energy sphere) |
| 0xA1 | `actor_x` | query | core | resolved actor position / facing |
| 0xA2 | `actor_y` | query | core | resolved actor position / facing |
| 0xA3 | `actor_dir` | query | core | resolved actor position / facing |
| 0xA4 | `learn_skill` | immediate | story | World.story.chars[c].skills / skill_levels (+ the live fighter) |
| 0xA5 | `forget_skill` | immediate | story | World.story.chars[c].skills / skill_levels (+ the live fighter) |
| 0xA6 | `replace_skill` | immediate | story | World.story.chars[c].skills / skill_levels (+ the live fighter) |
| 0xA7 | `watch_this_game` | modal | partial | scene::Menu mini-games (Watch This!, power struggle, punch): deterministic models, not the game's |
| 0xA8 | `fuse_characters` | immediate | story | fusion record (60% of the summed stats), member flags |
| 0xA9 | `spawn_map_item_at` | immediate | field | field::spawn_map_object (area lists 1 / 0 / 3) |
| 0xAA | `spawn_map_item` | immediate | field | field::spawn_map_object (area lists 1 / 0 / 3) |
| 0xAB | `show_radar` | immediate | scene | scene.radar |
| 0xAC | `radar_add_target` | immediate | scene | scene.radar |
| 0xAD | `respawn_breakables` | immediate | field | field::respawn_breakables (missing breakables whose condition holds) |
| 0xAE | `start_countdown` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xAF | `stop_countdown` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xB0 | `show_countdown_hud` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xB1 | `hide_timer_hud` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xB2 | `show_title_text` | immediate | scene | scene.title |
| 0xB3 | `sin_scale` | query | core | scene::SIN_TABLE (0x087B6EEC) |
| 0xB4 | `cos_scale` | query | core | scene::SIN_TABLE (0x087B6EEC) |
| 0xB5 | `run_event` | modal | VM | run_event / run_script |
| 0xB6 | `run_event` | modal | VM | run_event / run_script |
| 0xB7 | `run_event` | modal | VM | run_event / run_script |
| 0xB8 | `run_event` | modal | VM | run_event / run_script |
| 0xB9 | `run_event` | modal | VM | run_event / run_script |
| 0xBA | `get_flag_bits` | query | story | World.story flags |
| 0xBB | `set_flag_bits` | immediate | story | World.story flags |
| 0xBC | `show_stopwatch_hud` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xBD | `hide_stopwatch_hud` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xBE | `reset_stopwatch` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xBF | `start_stopwatch` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xC0 | `stop_stopwatch` | immediate | scene | HostState.timers + scene.timer_hud |
| 0xC1 | `get_stopwatch` | query | scene | HostState.timers + scene.timer_hud |
| 0xC2 | `give_xp` | immediate | core | player::gain_xp_levels; a level-up runs hook #3 inline (NativeResult::RunEvent); kill XP in the field queues #3 (Cause::LevelUp) |
| 0xC3 | `get_xp` | query | core | leader Player.rec.xp / level |
| 0xC4 | `set_game_bit_20` | immediate | story | World.story.game_bits |
| 0xC5 | `clear_game_bit_20` | immediate | story | World.story.game_bits |
| 0xC6 | `run_power_struggle` | modal | partial | scene::Menu mini-games (Watch This!, power struggle, punch): deterministic models, not the game's |
| 0xC7 | `run_punch_minigame` | modal | partial | scene::Menu mini-games (Watch This!, power struggle, punch): deterministic models, not the game's |
| 0xC8 | `get_level` | query | core | leader Player.rec.xp / level |
| 0xC9 | `broadcast_1f` | immediate | partial | engine broadcast with an unknown receiver: scene::Request::Broadcast |
| 0xCA | `heal_party` | immediate | story | every story record and live fighter: HP / EP full |
| 0xCB | `screen_rect_x_at_actor` | query | scene | scene::screen_rect_at (camera-clamped) |
| 0xCC | `screen_rect_y_at_actor` | query | scene | scene::screen_rect_at (camera-clamped) |
| 0xCD | `shop_add_item` | immediate | scene | scene.shop |
| 0xCE | `open_shop_buy` | modal | scene | scene::Menu ShopBuy / ShopSell + purchases on World.story |
| 0xCF | `open_shop_sell` | modal | scene | scene::Menu ShopBuy / ShopSell + purchases on World.story |
| 0xD0 | `set_form` | immediate | core | leader Player.apply_form |
| 0xD1 | `run_credits` | modal | partial | scene::Menu Credits (fixed length) + Request::Credits |
| 0xD2 | `a_pressed` | query | core | scene.keys[0] (the leader's raw input) |
| 0xD3 | `form_name_ptr` | query | core | string handle (text.rs) resolved by formatted text nodes |
| 0xD4 | `sleep_mode` | modal | scene | scene::Request ReturnToTitle / Sleep (session level) |
| 0xD5 | `is_equipped` | query | core | leader Player.equip |
| 0xD6 | `run_script` | immediate | VM | run_event / run_script |
| 0xD7 | `run_script` | immediate | VM | run_event / run_script |
| 0xD8 | `run_script` | immediate | VM | run_event / run_script |
| 0xD9 | `show_destination_label` | immediate | scene | scene.dest_label |
| 0xDA | `set_destination_label` | immediate | scene | scene.dest_label |
| 0xDB | `set_area_flag_c` | immediate | story | World.story.game_bits |
| 0xDC | `set_entity_value` | immediate | partial | engine broadcast with an unknown receiver: scene::Request::Broadcast |
| 0xDD | `party_xp_total` | query | story | summed XP of characters 0..4 |
| 0xDE | `string_ptr` | query | core | string handle (text.rs) resolved by formatted text nodes |
| 0xDF | `eq_add_hp_regen` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE0 | `eq_add_thorns` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE1 | `eq_add_end` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE2 | `eq_add_pow` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE3 | `eq_add_spd` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE4 | `eq_add_str` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE5 | `eq_halve_block_damage` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE6 | `eq_halve_energy_cost` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE7 | `eq_halve_energy_block_cost` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE8 | `eq_scale_ss_cooldown` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xE9 | `set_skill_level` | immediate | story | World.story.chars[c].skills / skill_levels (+ the live fighter) |
| 0xEA | `get_skill_level` | query | story | World.story.chars[c].skills / skill_levels (+ the live fighter) |
| 0xEB | `set_game_bit_40` | immediate | story | World.story.game_bits |
| 0xEC | `add_map_marker` | immediate | scene | scene.visuals (markers, box, map marker, energy sphere) |
| 0xED | `get_stat_points` | query | story | stat points of the character (fighter or record) |
| 0xEE | `add_stat_points` | immediate | story | stat points of the character (fighter or record) |
| 0xEF | `hide_title_text` | modal | scene | modal wait (Pending::Frames) |
| 0xF0 | `set_actor_field_1a8` | immediate | field | actor+0x1A8 = height << 10, positive up (fieldpatrol::set_z for map enemies) |
| 0xF1 | `unequip_slot` | immediate | story | unequip to the default item, back to the owned list |
| 0xF2 | `boost_stats` | immediate | core | seconds 0: base stats += (bytes); else the timed buff (Player.holy_left / boost, progression::recalc_stats_with) |
| 0xF3 | `level_up_to` | immediate | story | progression::level_up on the fighter / record |
| 0xF4 | `show_energy_sphere` | immediate | scene | scene.visuals (markers, box, map marker, energy sphere) |
| 0xF5 | `eq_add_crit` | immediate | story | HostState.eq_accum (equipment effect accumulator) |
| 0xF6 | `is_tampered` | query | core | always 0 (no tamper check) |
| 0xF7 | `start_timed_special` | immediate | core | the timed buff with no stat bonus; its special flag (record +0x67, effect unknown) is kept in HostState.boost |

Counts: VM 16, core 32, field 22, partial 13, scene 112, story 53

## 9. What is implemented and tested (`crates/wf-script-core`)

* `CoreHost` (borrowed) / `SharedHost` (owned, for the Rhai runtime) implement `wf_script::Host` for all 248
  natives and the event-graph nodes (text, choices, `Host::scene` enter / exit = the scene object's one-frame
  start and the end-of-event release). `HostState` (plain data) holds pending waits, timers, menu results.
* `tests/events.rs`: item use, loot determinism, VM == Rhai on loot, NPC talk, `bindings_are_honest` (every
  non-VM native runs in battle and field mode with the right arity and completes), AI script ids resolve.
* `tests/lockstep.rs`: emulator recordings (`data/script_lockstep/`, recorder `tools/script_lockstep/`) replayed
  frame by frame on the bytecode VM and on Rhai; compared per logic frame: actor positions (24.8), facing,
  animation state + cel, camera, screen black, text box text, sound events, scene end.
  * **72 micro scenarios** (one native or node each) and **6 full cutscenes** (intro, NPC talk with a choice,
    shop, Janemba -> Super Janemba #450, a Buu form change, a boss defeat scene): **78 / 78 exact** on every
    compared item (both tests strict), background objects included, VM == Rhai.
  * Round-2 fixes behind the last 6: a scene load has no player object (2 RNG draws fewer); `play_anim(a, 0)`
    returns a non-idle actor to idle; actors by sprite resolve to the newest object; the player's second idle
    draw on an area load is its idle tick; map NPCs with a behaviour come under script control and get their
    animation back on release; a load inside an event keeps the field held; the real beam object.
* `wf-script` emulator traces and the Rhai differential tests still pass.
* `tests/playthrough.rs` (story playthrough, recorder `tools/story_playthrough/`, data
  `data/story_playthrough/opening.json`: New Game -> the first boss, 19,508 frames played by the real game):
  **frames 0-18,826 continuous exact** (every actor, camera, fade, RNG, sounds, text, zenie; no state copied after
  frame 0), through the first fights, two level-ups and into the first boss's defeat scene. Host order per frame
  (also wf-app `StorySim`): `Session::step_begin` -> run the scripts it queued (drop scripts, kill-XP level-up,
  defeat scenes; a script started there takes its first wait from the next frame) -> `step_update` -> resume the
  blocked script -> start the update's scripts -> `step_post` -> start the trigger check's scripts. Hosts call
  `scene::ai_script_done` (Cause::Ai) and `field::finish_arrival` (Cause::AreaEnter) when those scripts end.
  Round-3 core rules found with it: loot = footprint centre x / bottom y + 1 px per frame along the killing hit,
  taken at the death; a coin keeps a step only when it reaches a new pixel; coins are collectable in the air;
  kill XP / level-up hook #3 and on-defeat scenes run at the next frame's start and hold the rest of that frame
  (the nested loop), the held update completes when the event ends, the death effect's drop countdown pauses
  meanwhile; enemies only animate while an event holds the field and walking ones stop at its start (message
  0xF); a dead enemy frees its grid cells; FacePlayer = 0x0801599C (24.8 difference, ties horizontal; field only);
  the step-away walk overrides any animation; a shake takes the camera slot (follow holds, plus one frame);
  an area load whose on-enter script blocks waits for it (players offstage, screen black, NPCs held).
  Round 4 (recording extended to 26,651 frames: Olibu's defeat scene, a level-65 grind over the respawning fighters
  of 1-13 / 1-14, the Character Gate, the Cave of the Ancients up to 1-17): **all 26,651 frames exact**. Rules
  added: a field death node Idle changes nothing (no hurt anim, no fade / removal); the follow camera keeps easing
  to a removed target's last position (cleared on loads); the field world list updates last to first in a list that
  compacts at once (an object that destroys one before it is updated again; `World::step_entities_list`), with the
  shake task once after the pass; the trigger check runs after the player's update and before the other objects',
  so coins are tested with their rect from before the frame's step (or where they dropped); coins move 60 frames;
  pickups stop while an event holds the field; scripted puffs draw their rate (0x1000 + rand(0x1000)); skill
  edits reach a player offstage in a cutscene scene; L plays sound 40. Route helpers: `tools/story_playthrough/`
  `try_route.py` (goals from a save state), `endstate.py`, `objdump.py`, `lootwatch.py`, `probe.py`.
  Round 5 (recording extended to 41,423 frames: Instant Transmission over the 1-17 wall to the Cotton Gi, Grand
  Kai's tournament 1-20, the final against Pikkon, his defeat scene and the cutscenes to Earth 2-1 = the end of
  Other World): **all 41,423 frames exact**. Rules added:
  - field Instant Transmission (player.rs `ctl_it_field`): the pick also offers triggers whose test is
    `0x0804AA08` (activation 0x0802266F) with rect (x0 + 15, y0 + 7, x1, y1) on camera; the run (vt 0x0804E334)
    turns the trigger list off (+0xD0) while away, lands at the centre of (x0, y0, x1 + 15, y1 + 7) - (8, -4)
    facing down, and arms the trigger (0x080125FA: the next check fires it anywhere, then restores it); the run
    waits while the fired event holds the field, the item-get pose gives the run's state back, sfx 31 at t -23
    always; the selector is a modal loop (no camera, no trigger check);
  - an event that ends in this frame's script phase leaves no trigger check that frame, unless a held update
    resumes (s090 -> s091); a held frame whose next queued event starts at once stays held (s003 -> s103); the
    resumed update gives NPCs no extra "event ended" tick; kill XP (level-up) runs before the defeat callbacks;
  - command tracks: set_pos popped at its start leaves the next command for the next tick, a head the tick starts
    is not updated in it, and tracks tick in world-list order (newest object first, camera first);
    `play_anim(0, 0)` re-idles a player; glide / remove / become_map_object lift an NPC's grid mark;
    `camera_pan_to` starts from the whole-pixel position; `set_form` = form exit, idle (rand), form enter;
  - the back-off node (0x08018E29) restores its reversed walk when a popped hurt state left another animation.
  Next: Earth (2-1 on) is the chapter agents' (wf-import segments); this recording ends at the Earth arrival.

## 10. Field core status (field agent, 2026-10-02; details docs/core.md §10)

Answers to §7: (1) persistent state is `World.story` (flags, l/g words, supplies, equipment, zenie, 9 `CharState`s,
party flags, quests, play time); (2) placed-enemy defeats leave `FieldEvent::Defeated` + `Run { on_defeat, Defeat }`
with the loot position set; (3) `Body::Pickup` (coins, dropped items, map items) with the game's physics and
collection — the drop natives call `fieldobj::set_loot / drop_zenie / drop_item`; (6) `field::warp`, `load_area`,
`spawn_map_object(ObjRef, force)`; trigger / talk / pickup / break / defeat / on-enter / on-leave scripts leave as
`FieldEvent::Run { run: Callback::{Script, Event}, cause, slot }`. A host can run them synchronously through
`Session.hook` (called where the game runs the script; needed for exact RNG, e.g. loot); otherwise after the step.
Blocking events: call `field::attach_host(w, true)`; event triggers / save points / talk then set `Field.busy`
(players get no input, triggers and map NPC behaviours pause, their animations tick) until `field::set_busy(w,
false)`. Spawn conditions are evaluated by the core itself (`story::eval_condition`, all 115 condition scripts).
Note for harnesses that delete the area's objects after `Session::new_field` (e.g. `tests/lockstep.rs`
`build_world`): talk NPCs, breakables and map items mark the walk grid / enemy layer when created — reset
`w.col.as_mut().unwrap().walk = None` and the enemy layer (`enemy = map.walk.clone()`) too, or the removed objects
keep blocking.

**Round 2 (field agent)**: native 0xF0 `set_actor_field_1a8(actor, v)` = obj+0x1A8 = `v << 10`, positive = **up**
(wf-script-core natives.rs stores `ac.z = -arg`: for map enemies use `wf_core::fieldpatrol::set_z(w, i, v << 10)`; the
world map's on-enter event inline_0x08072A48 lowers the 5 ground encounters with it). Map placements (negative
actor ids) resolve with `wf_core::field::actor_by_tag(w, -(id + 1))`. Run the area load's scripts synchronously:
`Session::new_field_hooked` / `Session.hook` (gates and bridges in 48 areas use open_area / block_area on enter).
`open_area` / `block_area` (0x2B/0x2C) = walk grid + enemy layer, `_alt` (0x2D/0x2E) = shot grid only (fixed in
scene.rs `apply_area`).
