# Script host ↔ wf-core: the integration seam

How Buu's Fury scripts (spec `re/buu/SCRIPT.md`, runtime `docs/scripting.md`) drive the deterministic simulation
core `wf-core`. Code: `crates/wf-script-core` (adapter; depends on `wf-core` and `wf-script` — the dependency
points script → core, core never sees scripts).

## 1. Readiness of wf-core (2026-10-02)

`wf-core` is a **battle simulation**: `World` has players (with their character `Record`, form, equipment, skills,
XP/level-up), enemies with AI, shots, effects, per-player input and cameras, the game PRNG, sound/music as
`Event` data, plain-data snapshots (`World::clone`, postcard bytes) and a per-frame `Session::step`. It does **not**
model the field game: story flags, inventory, zenie, the 9-character party, map objects / NPCs / triggers / props,
area loading and warps, cutscene actor command tracks, dialog / menus, screen effects, the HUD/boss bar.

Verdict: ready for the **non-field** part of the script API — stats, forms, XP, equipment checks, RNG, sound,
actor queries — which the adapter implements on the core directly. The field-game state the scripts need is held
by the adapter in `StoryState` (plain data, snapshotted next to the World) until core grows it (§7). Cutscene
choreography, maps, menus and screen effects are **missing** in core; the adapter reports them as explicit
errors naming the missing piece.

Coverage of the 248 natives (single source: `wf_script_core::binding::binding(n)`, table in §8):
**33 core** (existing wf-core APIs), **63 story** (adapter state), **16 VM** (G/P, run_script, run_event),
**136 missing** (proposed APIs below). By *use count in the ROM's scripts* the missing ones dominate cutscenes
(actor commands, camera, fades); everything that item use, equipment effects, loot, flags/quests, shops and
plain NPC dialog need is covered.

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
* Each frame, after `Session::step` (so the World advanced and the input of this frame is visible):
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
12. Optional: `Snapshot.ext: Vec<u8>` for byte snapshots of the layers above core.
13. `docs/core.md` (referenced by `wf-core/src/lib.rs`) does not exist yet.

## 8. Native → system table

Generated by `cargo run -p wf-script-core --example coverage`.

| id | native | kind | where | core API / missing piece |
|---|---|---|---|---|
| 0x00 | `G0` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x01 | `G1` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x02 | `G2` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x03 | `G3` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x04 | `P0` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x05 | `P1` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x06 | `P2` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x07 | `P3` | query | VM | VM globals G0..G3 / param block P0..P3 |
| 0x08 | `get_l0` | query | story | StoryState.l[i] (game words 0x03001020) |
| 0x09 | `get_l1` | query | story | StoryState.l[i] (game words 0x03001020) |
| 0x0A | `get_l2` | query | story | StoryState.l[i] (game words 0x03001020) |
| 0x0B | `get_l3` | query | story | StoryState.l[i] (game words 0x03001020) |
| 0x0C | `get_hp` | query | core | leader Player.rec.hp / .max_hp |
| 0x0D | `get_max_hp` | query | core | leader Player.rec.hp / .max_hp |
| 0x0E | `get_energy` | query | core | leader Player: form.ss3 -> ss3_gauge, form.ss -> ss_gauge, else rec.ep |
| 0x0F | `get_max_ep` | query | core | leader Player.rec.max_ep |
| 0x10 | `get_zenie` | query | story | StoryState.zenie |
| 0x11 | `get_variant_byte` | query | story | StoryState.variant_byte |
| 0x12 | `get_world` | query | story | StoryState.area (world, map, variant); core has only World.arena.map |
| 0x13 | `get_map` | query | story | StoryState.area (world, map, variant); core has only World.arena.map |
| 0x14 | `get_map_variant` | query | story | StoryState.area (world, map, variant); core has only World.arena.map |
| 0x15 | `get_character` | query | core | leader Player.form -> GameData.form(id).char_ |
| 0x16 | `get_form_name` | query | **MISSING** | string handles: script values that name UI strings (StringRef(i32) table in core/ui) |
| 0x17 | `get_form_label` | query | **MISSING** | string handles: script values that name UI strings (StringRef(i32) table in core/ui) |
| 0x18 | `get_menu_result` | query | story | StoryState.menu_result (set by choice nodes / menus) |
| 0x19 | `rand` | query | core | World.rng.rand(n) |
| 0x1A | `chance` | query | core | World.rng.chance(pct * 0x28F5D >> 8) |
| 0x1B | `get_flag` | query | story | StoryState.flags |
| 0x1C | `not_flag` | query | story | StoryState.flags |
| 0x1D | `set_flag` | immediate | story | StoryState.flags |
| 0x1E | `clear_flag` | immediate | story | StoryState.flags |
| 0x1F | `play_sfx` | immediate | core | World::sfx(id, vol) -> Event::Sfx |
| 0x20 | `play_sfx_volume` | immediate | core | World::sfx(id, vol) -> Event::Sfx |
| 0x21 | `stop_sfx` | immediate | core | World::sfx_stop(id) -> Event::SfxStop |
| 0x22 | `stop_all_sfx` | immediate | **MISSING** | Event::SfxStopAll |
| 0x23 | `play_music` | immediate | core | Event::Song { id } |
| 0x24 | `stop_music` | immediate | **MISSING** | Event::SongStop |
| 0x25 | `show_bg` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x26 | `hide_bg` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x27 | `set_bg_priority` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x28 | `set_blend_targets` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x29 | `entity_call` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x2A | `set_blend_alpha` | immediate | **MISSING** | presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*) |
| 0x2B | `open_area` | immediate | **MISSING** | Collision overrides: Collision::set_rect(layer, rect, passable) on a per-World copy-on-write grid |
| 0x2C | `block_area` | immediate | **MISSING** | Collision overrides: Collision::set_rect(layer, rect, passable) on a per-World copy-on-write grid |
| 0x2D | `open_area_alt` | immediate | **MISSING** | Collision overrides: Collision::set_rect(layer, rect, passable) on a per-World copy-on-write grid |
| 0x2E | `block_area_alt` | immediate | **MISSING** | Collision overrides: Collision::set_rect(layer, rect, passable) on a per-World copy-on-write grid |
| 0x2F | `show_hud` | immediate | **MISSING** | World.hud_visible: bool |
| 0x30 | `hide_hud` | immediate | **MISSING** | World.hud_visible: bool |
| 0x31 | `camera_center_on` | modal | **MISSING** | camera script mode: Camera { target: CamTarget::{Follow(id), MoveTo{x,y,speed}} } + done() |
| 0x32 | `camera_scroll_to` | modal | **MISSING** | camera script mode: Camera { target: CamTarget::{Follow(id), MoveTo{x,y,speed}} } + done() |
| 0x33 | `camera_follow` | immediate | **MISSING** | Camera.follow: Option<EntityId> (core cameras always follow the player) |
| 0x34 | `camera_release` | immediate | **MISSING** | Camera.follow: Option<EntityId> (core cameras always follow the player) |
| 0x35 | `spawn_actor` | immediate | **MISSING** | scripted actors: Body::Npc(Npc { a: Actor, track: Track }) + World::spawn_npc(sprite, x, y) |
| 0x36 | `spawn_map_object` | immediate | **MISSING** | map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force) |
| 0x37 | `count_actors` | query | core | count live entities whose Actor.sprite == id (players by character) |
| 0x38 | `begin_parallel` | immediate | story | StoryState.director.batch = true |
| 0x39 | `wait_for_actor` | immediate | story | StoryState.director.group.push(actor) |
| 0x3A | `end_parallel` | modal | **MISSING** | commit: block until every Track (or the group) is idle — needs per-actor Tracks |
| 0x3B | `fire_wave_beam` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x3C | `play_anim` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x3D | `set_anim` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x3E | `fire_ball_at` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x3F | `puff` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x40 | `puff_offset` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x41 | `shoot_at_actor` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x42 | `shoot_at` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x43 | `hit_flash` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x44 | `walk_to` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x45 | `jump_to` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x46 | `fire_beam` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x47 | `face_actor` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x48 | `puff_at` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x49 | `glide_to` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4A | `actor_wait` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4B | `queue_sfx` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4C | `screen_flash` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4D | `remove_actor` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4E | `change_sprite` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x4F | `become_map_object` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x50 | `move_to` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x51 | `set_pose` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x52 | `tint` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x53 | `face` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x54 | `set_actor_speed` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x55 | `set_attachment` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x56 | `set_actor_byte8` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x57 | `shake_screen` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x58 | `show_item_get` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x59 | `play_anim_frames` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5A | `emote_wait` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5B | `emote` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5C | `fire_beam_alt` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5D | `set_pos` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5E | `fire_blast` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x5F | `walk_to_actor` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x60 | `walk_to` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x61 | `walk_to_speed` | enqueue | **MISSING** | per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done |
| 0x62 | `prop_poof` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x63 | `prop_hide` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x64 | `prop_show` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x65 | `prop_effect_at` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x66 | `prop_move_to` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x67 | `prop_jump_to` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x68 | `prop_wait` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x69 | `prop_set_pos` | enqueue | **MISSING** | map props (scripted OBJ sprites): World.props + Track |
| 0x6A | `camera_follow_wait` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x6B | `camera_pan_to` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x6C | `camera_release_wait` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x6D | `camera_set_param8` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x6E | `camera_set_param12` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x6F | `camera_set` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x70 | `camera_set3` | enqueue | **MISSING** | camera Track (queued camera commands) |
| 0x71 | `save_field_on_next_scene` | immediate | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x72 | `load_scene_fade` | modal | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x73 | `load_scene` | immediate | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x74 | `load_scene_fade_noobj` | modal | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x75 | `return_to_field` | modal | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x76 | `warp_fade` | modal | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x77 | `warp` | immediate | **MISSING** | area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return) |
| 0x78 | `fade_out` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x79 | `fade_in` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7A | `fade_out_white` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7B | `fade_in_white` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7C | `mosaic_out` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7D | `mosaic_in` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7E | `flash_white` | modal | **MISSING** | screen effect timer: World.screen_fx { kind: Fade\|White\|Mosaic\|Flash, t, len } (modal, done when t == len) |
| 0x7F | `item_count` | query | story | StoryState.supplies (u8 counts) |
| 0x80 | `give_item` | enqueue | story | StoryState inventory + present(ItemGet) (60-frame popup action) |
| 0x81 | `add_item` | immediate | story | StoryState.supplies (u8 counts) |
| 0x82 | `take_item` | immediate | story | StoryState.supplies (u8 counts) |
| 0x83 | `take_items` | immediate | story | StoryState.supplies (u8 counts) |
| 0x84 | `drop_zenie` | immediate | story | StoryState.drops (pickup spawn requests at the loot position) — needs Body::Pickup in core to appear |
| 0x85 | `drop_item` | immediate | story | StoryState.drops (pickup spawn requests at the loot position) — needs Body::Pickup in core to appear |
| 0x86 | `drop_equipment` | immediate | story | StoryState.drops (pickup spawn requests at the loot position) — needs Body::Pickup in core to appear |
| 0x87 | `drop_zenie_at` | immediate | story | StoryState.drops (pickup spawn requests at the loot position) — needs Body::Pickup in core to appear |
| 0x88 | `wait` | modal | story | modal wait: Pending::Frames counted by ScriptLayer::poll |
| 0x89 | `wait_button` | modal | core | modal: Pending::Button, done on World.input[0].pressed & A |
| 0x8A | `spawn_map_object_force` | immediate | **MISSING** | map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force) |
| 0x8B | `refresh_map_objects` | immediate | **MISSING** | out of table |
| 0x8C | `save_menu` | modal | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0x8D | `character_menu` | modal | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0x8E | `return_to_title` | immediate | **MISSING** | session control: return to title / credits / sleep (Session-level, not World) |
| 0x8F | `show_boss_bar` | immediate | **MISSING** | World.boss_bar: Option<(EntityId, a, b)> |
| 0x90 | `show_boss_bar_ex` | immediate | **MISSING** | World.boss_bar: Option<(EntityId, a, b)> |
| 0x91 | `hide_boss_bar` | immediate | **MISSING** | World.boss_bar: Option<(EntityId, a, b)> |
| 0x92 | `equipment_count` | query | story | StoryState.equipment (owned list) |
| 0x93 | `give_equipment` | enqueue | story | StoryState inventory + present(ItemGet) (60-frame popup action) |
| 0x94 | `take_equipment` | immediate | story | StoryState.equipment (owned list) |
| 0x95 | `set_character` | immediate | core | leader Player.apply_form(GameData.chars[c].form) (party roster itself: StoryState) |
| 0x96 | `in_party` | query | story | StoryState.party / char_flags |
| 0x97 | `join_party` | immediate | story | StoryState.party / char_flags |
| 0x98 | `leave_party` | immediate | story | StoryState.party / char_flags |
| 0x99 | `set_char_flag2` | immediate | story | StoryState.party / char_flags |
| 0x9A | `clear_char_flag2` | immediate | story | StoryState.party / char_flags |
| 0x9B | `is_super_saiyan` | query | core | GameData.form(leader.form).ss \|\| .ss3 |
| 0x9C | `char_sprite` | query | core | GameData.form(chars[c].form).gfx - 9 |
| 0x9D | `set_char_form` | immediate | story | StoryState.char_form[c] (per-character current form; party records not in core) |
| 0x9E | `spawn_marker_a` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0x9F | `spawn_marker_b` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xA0 | `spawn_box` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xA1 | `actor_x` | query | core | resolved entity Actor.x>>8 / y>>8 / facing |
| 0xA2 | `actor_y` | query | core | resolved entity Actor.x>>8 / y>>8 / facing |
| 0xA3 | `actor_dir` | query | core | resolved entity Actor.x>>8 / y>>8 / facing |
| 0xA4 | `learn_skill` | immediate | **MISSING** | per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills) |
| 0xA5 | `forget_skill` | immediate | **MISSING** | per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills) |
| 0xA6 | `replace_skill` | immediate | **MISSING** | per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills) |
| 0xA7 | `watch_this_game` | modal | **MISSING** | mini-games (Watch This!, power struggle, punch): modal sub-sessions |
| 0xA8 | `fuse_characters` | immediate | **MISSING** | fusion of party records: progression::fuse exists; needs party records + timed fusion in World |
| 0xA9 | `spawn_map_item_at` | immediate | **MISSING** | map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force) |
| 0xAA | `spawn_map_item` | immediate | **MISSING** | map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force) |
| 0xAB | `show_radar` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xAC | `radar_add_target` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xAD | `respawn_conditional_map_objects` | immediate | **MISSING** | map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force) |
| 0xAE | `start_countdown` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xAF | `stop_countdown` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xB0 | `show_countdown_hud` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xB1 | `hide_timer_hud` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xB2 | `show_title_text` | immediate | **MISSING** | title text overlay: World.title_text |
| 0xB3 | `sin_scale` | query | **MISSING** | the game's sine table (wf-core::fixed::sin_q(angle)) |
| 0xB4 | `cos_scale` | query | **MISSING** | the game's sine table (wf-core::fixed::sin_q(angle)) |
| 0xB5 | `run_event` | modal | VM | run_event / run_script |
| 0xB6 | `run_event` | modal | VM | run_event / run_script |
| 0xB7 | `run_event` | modal | VM | run_event / run_script |
| 0xB8 | `run_event` | modal | VM | run_event / run_script |
| 0xB9 | `run_event` | modal | VM | run_event / run_script |
| 0xBA | `get_flag_bits` | query | story | StoryState.flags (bit range) |
| 0xBB | `set_flag_bits` | immediate | story | StoryState.flags (bit range) |
| 0xBC | `show_stopwatch_hud` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xBD | `hide_stopwatch_hud` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xBE | `reset_stopwatch` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xBF | `start_stopwatch` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xC0 | `stop_stopwatch` | immediate | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xC1 | `get_stopwatch` | query | story | StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation |
| 0xC2 | `give_xp` | immediate | core | player::gain_xp(w, leader, n) (needs World.opts.xp) |
| 0xC3 | `get_xp` | query | core | leader Player.rec.xp / .level |
| 0xC4 | `set_game_bit_20` | immediate | story | StoryState.game_bits |
| 0xC5 | `clear_game_bit_20` | immediate | story | StoryState.game_bits |
| 0xC6 | `run_power_struggle` | modal | **MISSING** | mini-games (Watch This!, power struggle, punch): modal sub-sessions |
| 0xC7 | `run_punch_minigame` | modal | **MISSING** | mini-games (Watch This!, power struggle, punch): modal sub-sessions |
| 0xC8 | `get_level` | query | core | leader Player.rec.xp / .level |
| 0xC9 | `broadcast_1f` | immediate | **MISSING** | engine message broadcast to entities (World::broadcast_msg(kind, value)) |
| 0xCA | `heal_party` | immediate | core | every Player in World.players: rec.hp = max_hp, rec.ep = max_ep (+ StoryState party records) |
| 0xCB | `screen_rect_x_at_actor` | query | **MISSING** | screen-space queries need the presentation camera contract (World::camera(0) + actor box) |
| 0xCC | `screen_rect_y_at_actor` | query | **MISSING** | screen-space queries need the presentation camera contract (World::camera(0) + actor box) |
| 0xCD | `shop_add_item` | immediate | story | StoryState.shop (list for the shop UI) |
| 0xCE | `open_shop_buy` | modal | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0xCF | `open_shop_sell` | modal | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0xD0 | `set_form` | immediate | core | leader Player.apply_form(form) |
| 0xD1 | `run_credits` | modal | **MISSING** | session control: return to title / credits / sleep (Session-level, not World) |
| 0xD2 | `a_pressed` | query | core | World.input[0].pressed & A (leader's input) |
| 0xD3 | `form_name_ptr` | query | **MISSING** | string handles (see 0x16) |
| 0xD4 | `sleep_mode` | modal | **MISSING** | session control: return to title / credits / sleep (Session-level, not World) |
| 0xD5 | `is_equipped` | query | core | leader Player.equip contains item |
| 0xD6 | `run_script` | immediate | VM | run_event / run_script |
| 0xD7 | `run_script` | immediate | VM | run_event / run_script |
| 0xD8 | `run_script` | immediate | VM | run_event / run_script |
| 0xD9 | `show_destination_label` | immediate | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0xDA | `set_destination_label` | immediate | **MISSING** | UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input |
| 0xDB | `set_area_flag_c` | immediate | story | StoryState.game_bits |
| 0xDC | `set_entity_value` | immediate | **MISSING** | engine message broadcast to entities (World::broadcast_msg(kind, value)) |
| 0xDD | `party_xp_total` | query | core | sum of rec.xp of active players (party records: StoryState) |
| 0xDE | `string_ptr` | query | **MISSING** | string handles (see 0x16) |
| 0xDF | `eq_add_hp_regen` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE0 | `eq_add_thorns` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE1 | `eq_add_end` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE2 | `eq_add_pow` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE3 | `eq_add_spd` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE4 | `eq_add_str` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE5 | `eq_halve_block_damage` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE6 | `eq_halve_energy_cost` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE7 | `eq_halve_energy_block_cost` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE8 | `eq_scale_ss_cooldown` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xE9 | `set_skill_level` | immediate | **MISSING** | per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills) |
| 0xEA | `get_skill_level` | query | **MISSING** | per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills) |
| 0xEB | `set_game_bit_40` | immediate | story | StoryState.game_bits |
| 0xEC | `add_map_marker` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xED | `get_stat_points` | query | core | Player.rec.points of the fighter playing that character |
| 0xEE | `add_stat_points` | immediate | core | Player.rec.points of the fighter playing that character |
| 0xEF | `hide_title_text` | modal | **MISSING** | title text overlay: World.title_text |
| 0xF0 | `set_actor_field_1a8` | immediate | **MISSING** | engine message broadcast to entities (World::broadcast_msg(kind, value)) |
| 0xF1 | `unequip_slot` | immediate | core | Player.equip[slot] = 0 + refresh_stats; item back to StoryState.equipment |
| 0xF2 | `boost_stats` | immediate | core | seconds > 0: Player.holy_left = seconds*60 (+ boost values: Missing in EffStats); 0: rec stats += values |
| 0xF3 | `level_up_to` | immediate | core | progression::level_up(rec, growth, old, new, World.rng) |
| 0xF4 | `show_energy_sphere` | immediate | **MISSING** | map markers / radar / energy sphere objects (presentation entities) |
| 0xF5 | `eq_add_crit` | immediate | story | StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats) |
| 0xF6 | `is_tampered` | query | core | always 0 (no tamper check in the remake) |
| 0xF7 | `start_timed_special` | immediate | **MISSING** | timed special buff (record byte +0x67) |

Counts: **MISSING** 136, VM 16, core 33, story 63

## 9. What is implemented and tested (`crates/wf-script-core`)

* `CoreHost` (borrowed) / `SharedHost` (owned, for the Rhai runtime) implement `wf_script::Host` for every
  **core** and **story** binding; every **missing** native returns
  `Err("native 0xNN name: not available in wf-core yet (<missing piece>)")`.
* `poll()` drives `Pending` (frames, A button, choice menus) and the script timers; `leader`, `player(n)`,
  `resolve(actor id)` implement the actor/multiplayer mapping.
* Tests (`tests/events.rs`, headless; content from the ROM, reusing wf-core's test extraction when present):
  * Miso Soup use script (`inline_0x083BBBB0`) on a real core player: HP up, supply consumed, `Event::Sfx`;
  * loot `s497` on `World.rng` over 20 seeds: deterministic, drops at the loot position;
  * the same loot run by the **Rhai runtime** on `SharedHost` equals the VM run (drops and RNG state), 10 seeds;
  * NPC talk event `s043` (Gregory): text node presented, blocks until A;
  * `bindings_are_honest`: every core/story binding runs on a real World with the right result arity, every
    missing one errors.
* Builds and passes in the engine workspace (`cargo test -p wf-script-core`). While wf-core is mid-edit,
  `engine/experiments/wf-script-core-dev` builds the adapter against the last committed wf-core (copied, gitignored).
