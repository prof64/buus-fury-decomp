# Scripting (story, events, items) — the Rhai API

Buu's Fury drives its story with a small bytecode VM and event graphs (spec: `re/buu/SCRIPT.md`). The remake
keeps two layers:

1. **`wf-script`** — the original format: loader for all 522 events and 2540 bytecode units, an **exact,
   resumable interpreter** (`vm::Thread`, host trait `vm::Host`) and a **decompiler** to structured IR and
   readable [Rhai](https://rhai.rs). The interpreter is the verification oracle.
2. **`wf-script-rhai`** — the runtime the engine uses: runs decompiled originals and mods in Rhai, behind the
   same `Host` trait. Differential test: every script, interpreter vs Rhai, identical effect traces
   (`crates/wf-script-rhai/tests/differential.rs`).

Why Rhai (details and numbers: [`script-runtime-eval.md`](script-runtime-eval.md)): pure Rust, builds for
`wasm32-unknown-unknown` without imports, integer-only (`only_i32`, `no_float`), no clock, script functions cannot
keep hidden global state, ~2× slower in wasm than native and fast enough (~1 µs per handler call). Boa (JS) works
but is 4× larger, float-based and keeps state in a heap that cannot be snapshotted; QuickJS/mlua need a C
toolchain for wasm; piccolo cannot clone its state.

## 1. Execution model (rollback-safe)

Online play uses rollback netcode: each peer simulates; on a late input the sim restores a snapshot from a few
frames ago and re-simulates. So **script state must be plain engine data, and scripts deterministic**.

* A handler **runs to completion** within one simulation step. Nothing stays suspended inside Rhai.
* Natives **with an effect** (`walk_to`, `play_sfx`, `give_item`, `set_hp`, …) are **actions**: calling one
  appends it to the thread's **action queue**. The engine runs queued actions in order after the handler
  returns; an action may take many frames (a walk, a fade, a text box); the next starts when it finishes.
  *Instant* actions (no frame span, e.g. `set_flag`, `play_sfx`) execute immediately when the queue is empty.
* Natives **with a result** (`get_flag`, `actor_x`, `rand`, …) are **queries**: they run immediately and are only
  allowed while the queue is empty (they must observe the world *after* earlier actions). Calling one behind
  queued actions is an error that tells you to split.
* `return next(k)` ends the handler: when the queue has drained, the engine calls `step_k(result)`, where
  `result` is the value of the last action (menus and mini-games return one).
* `return v` ends the script (after its queue drains). In an event code node, an int `v` selects the next node.
* Script variables are engine data: `g(i)`/`set_g(i, v)` (4 globals shared by all scripts, persist),
  `p(i)`/`set_p(i, v)` (the run's 4 parameters, shared with nested `run_script`/`run_event`),
  `local(i)`/`set_local(i, v)` (thread locals that survive `next`). Plain `let` variables die with the handler.

The thread (`wf_script_rhai::ThreadState`: frames, step indices, locals, params, queue, pending result) is
`Clone + Serialize`; snapshot = clone, restore = assign. Tested: snapshot at every block point, restore,
re-run → identical traces; native and wasm32 give the same digest over 6.7 M effects (§6).

This is exactly the original's behaviour: on the GBA every effect native ran to completion before returning
(blocking ones in a nested main loop), so "queue, then run in order" performs the same effects in the same
order, and queries only ever see the world after all earlier effects.

## 2. Determinism rules

* Integers only (32-bit, wrapping like the GBA; `/` and `%` truncate; division by zero is an error).
* Randomness only via `rand(n)` / `chance(pct)` — the game's PRNG, part of the snapshot.
* No clock, no I/O, no iteration over hash maps (Rhai object maps are ordered), no local-peer state: a script
  never asks "which player is *me*"; it receives players as arguments.
* Budgets are deterministic (operation counts, not time).

## 3. Writing scripts

A script file defines functions; the engine calls the entry point (`main()` for one-shot handlers, or
`step_0()` with continuations `step_k(result)`).

```rhai
// talk handler for an NPC (event graph code node or inline script)
fn step_0() {
    face_actor(0, 23);                 // queued
    if get_flag(281) != 0 {            // query: legal, the queue is empty
        return 4;                      // event: continue at node 4
    }
    walk_to(23, 120, 80, 200);         // queued
    play_anim(23, 4);
    return next(1);                    // wait for the walk/anim, then step_1
}
fn step_1(result) {
    if actor_x(23) > 100 {             // sees the world after the walk
        give_item(12);
    }
}
```

Cutscene batches: `begin_parallel(); walk_to(a, …); walk_to(b, …); end_parallel();` — commands between the two
run simultaneously on their actors' tracks; `end_parallel()` waits for all (as in the original director,
`0x03002BAC`).

Event graphs (dialog trees) are data: `fn event()` returns the nodes (`code`, `say` with box/portrait,
`choice`, `jump`, `end`). A `choice` stores the selection, read by the next code node with
`get_menu_result()`.

```rhai
fn event() {
    [
        #{ node: 0, say: "You're in no shape for fighting, Vegeta!", box: 4, next: 1 },
        #{ node: 1, choice: "Fight?\n-Yes\n-No", box: 0, next: 2 },
        #{ node: 2, code: "n2_", entry: "main", next: 3 },
        #{ node: 3, end: 0 },
        #{ node: 4, jump: 451 },
    ]
}
fn n2_main() { if get_menu_result() == 0 { return 4; } }
```

## 4. API reference

All 248 original natives are available under their decoded names (`engine/annotations/script_natives.json`,
reference table in `re/buu/SCRIPT.md` §6). Highlights by category:

| category | functions |
|---|---|
| flags & vars | `get_flag(f)`, `not_flag(f)`, `set_flag(f)`, `clear_flag(f)`, `get_flag_bits(first, n)`, `set_flag_bits(first, n, v)`, `g/set_g`, `p/set_p`, `local/set_local`, `get_l0..3`/`set_l0..3` |
| party & stats | `get_character()`, `set_character(c)`, `set_form(f)`, `join_party(c)`, `leave_party(c)`, `in_party(c)`, `get_hp()`/`set_hp(v)`, `get_energy()`/`set_energy(v)`, `give_xp(n)`, `get_level()`, `learn_skill(c, s)`, `set_skill_level(c, s, l)`, `boost_stats(sec, str, pow, end, spd)`, `fuse_characters(...)` |
| items | `item_count(i)`, `give_item(i)`, `add_item(i, n)`, `take_item(i)`, `equipment_count(i)`, `give_equipment(i)`, `is_equipped(i)`, `drop_zenie(size)`, `drop_item(i)`, `drop_equipment(i)`, shop: `shop_add_item(cat, item, price, flag)`, `open_shop_buy()`, `open_shop_sell(pct)` |
| actors (queued) | `spawn_actor(sprite, x, y)`, `walk_to(a, x, y[, speed])`, `glide_to`, `jump_to`, `move_to`, `face(a, dir)`, `face_actor(a, b)`, `play_anim(a, anim)`, `set_pose`, `change_sprite(a, sprite)` (transformations), `emote`, `fire_beam/fire_blast/shoot_at`, `actor_wait(a, frames)`, `remove_actor(a)`, `become_map_object(a, i)` (hand over to the real enemy/NPC) |
| actor queries | `count_actors(a)`, `actor_x(a)`, `actor_y(a)`, `actor_dir(a)` |
| camera & screen | `camera_center_on`, `camera_scroll_to`, `camera_pan_to`, `camera_follow`, `camera_release`, `fade_out/in(frames)`, `fade_out_white/in_white`, `mosaic_out/in`, `flash_white`, `shake_screen(a, i, frames)`, `screen_flash`, `show_hud/hide_hud`, `show_boss_bar(a)` |
| flow | `wait(frames)`, `wait_button()`, `begin_parallel()`, `end_parallel()`, `wait_for_actor(a)`, `run_script(event[, p0[, p1]])` (node 0, now), `run_event(event[, p0..p3])` (whole graph, queued), `next(k)` |
| maps | `warp(world, map, variant, x, y)`, `warp_fade(...)`, `load_scene(world, map, variant, x, y)`, `load_scene_fade`, `return_to_field()`, `spawn_map_object(i)`, `open_area/block_area(x0, y0, x1, y1)` |
| audio | `play_music(song)`, `stop_music()`, `play_sfx(s)`, `stop_sfx(s)` |
| rng | `rand(n)` → [0, n), `chance(pct)` → 0/1 |

Actor ids: 0 = the player (leader), 1–8 party characters' current forms, ≥10 sprite id of a live object, −n =
map placement n−1, 9 = the scene's special object.

### 4.1 Multiplayer API (implemented: `wf_script::vm::EXTENSIONS`, `Host::ext`)

Original scripts address "the player" as actor 0; in multiplayer actor 0 is the **leader** (`player(1)`).
Everything else is a superset the originals never call. Players are 1-based. No function exposes the local
peer — a handler is told *who* triggered it, never "who am I".

* **Triggering player**: the engine starts handlers with `Runtime::start_event_by(event, params, who)` or
  `Runtime::start_handler(file, "on_talk", who, params)`; scripts read it with `who()` (or the entry
  argument: `fn on_talk(who) { … }`). Conventions: `on_talk(who)`, `on_enter(who)`, `on_touch(who)`,
  `on_defeat(by)`, `on_use(who)`. Extension actions carry `who` to the host.
* **Queries**: `player_count()`, `player(n)` (actor id), `player_of_actor(actor)` (0 = none),
  `player_character(n)`, `player_hp(n)`, `player_max_hp(n)`, `player_energy(n)`, `player_x(n)`, `player_y(n)`,
  `player_alive(n)`, `player_button(n, b)` / `player_pressed(n, b)` (b: 0 A, 1 B, 2 L, 3 R, 4 Start; the
  original `a_pressed()` reads the leader).
* **Instant effects**: `set_player_hp(n, v)`, `set_player_energy(n, v)`, `set_player_character(n, c)`.
* **Actions**: `give_item_to(n, item)`, `give_equipment_to(n, item)`, `give_xp_to(n, xp)`, `revive_player(n)`,
  `warp_player(n, world, map, variant, x, y)`, `gather_players(x, y)` (cutscene start), `say_to(n, portrait,
  string_id)` (a text box shown to one player).
* Shared by all players: story flags, G0..G3, the event graph, inventory-less natives. Per-player in the host:
  HP/EP/character/inventory when the game mode says so (the original natives act on the leader).

Test: `crates/wf-script-rhai/tests/multiplayer.rs` (3 players, per-player effects carry `who`, a query behind
queued actions is rejected, snapshot/restore mid-handler identical).

## 5. Decompiler output

`cargo run -p wf-script --example export -- <dir>` writes `scripts/events/sNNN.rhai` (graph + node code) and
`scripts/inline/inline_0xADDR.rhai` (ids as in `engine/annotations/script_catalog.json` and wf-world's
`objects.json`). The decompiler:

* lifts the stack code with exact evaluation order (temps where a statement would reorder effects),
* structures control flow (all 2540 units structure: if/else chains, counted loops),
* keeps the original's non-short-circuit `and`/`or` where the right side has an effect (`both`, `either`),
* inserts `return next(k)` only where a query follows queued actions, after menus/mini-games that return a
  value, after `run_event`, and after `run_script` of a script that itself waits; branches and loops around
  a split become continuation steps; values crossing a split become `local(i)`.

## 6. Verification

| check | what | result |
|---|---|---|
| decompression | 141 compressed units vs the game's own decompressor (emulator) | byte-identical |
| stack effects | native arities vs every unit's stack depth at every join (2458 units) | consistent |
| interpreter vs Rhai | 522 events + 377 node-0 runs + 774 inline/drop scripts = 1673 cases × 4 host seeds = 6692 runs | identical traces (2.35 M effects), 189 runs error identically on both sides (host-invented node indices / 0-count loops) |
| rollback | snapshot at each block point, JSON round-trip, restore, re-run (Rhai and VM) | identical |
| native vs wasm | digest of all events (VM + Rhai + one restore each) | identical (`engine/experiments/wf-script-dev/wasm-check`) |
| emulator vs interpreter | 9 real-game traces replayed in the interpreter (natives, args, offsets, G/P, var writes, text nodes) | 97 roots, 845 host calls, 0 mismatches (`re/buu/SCRIPT.md` §7) |
