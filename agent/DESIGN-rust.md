# Agent interface on the Rust core - design, performance, play results

The agent interface of the TypeScript prototype (`agent/DESIGN.md`, `agent/mcp`) rebuilt on the deterministic Rust
core (`engine/crates/wf-core`): one crate, `engine/crates/wf-mcp`, that is

* a **native MCP server** (`wf-mcp`, stdio JSON-RPC 2.0, no SDK dependency: `initialize`, `tools/list`,
  `tools/call` with text + PNG image content),
* a **transport-free tool API** (`wf_mcp::tools::Server::call(name, args)`) that the server, the C ABI and the tests
  all use,
* a **C ABI** (`libwf_mcp.dylib/.so`) with a gym-style fast path and the whole tool API as JSON, wrapped for Python
  by `python/wf_gym.py` (ctypes; no build tools beyond cargo),

shipped with a Claude Code plugin (`agent/plugin`: manifest, `.mcp.json`, launcher, the `buu-battle` skill, example
Rhai controllers). Everything is headless and muted; only `wf-core`'s public API is used (`Session::new` /
`new_field`, `step`, `snapshot` / `restore`, `hash`, `world` reads, `view::render_view`, `player::fits` /
`can_use_skill`) plus `wf-render` for screenshots.

```
cd engine && cargo build --release -p wf-mcp          # target/release/wf-mcp + libwf_mcp
cargo test --release -p wf-mcp                        # protocol, a won fight + replay hash, controller sandbox
python3 crates/wf-mcp/python/playtest.py              # the validation fights below, through the MCP protocol
python3 crates/wf-mcp/python/bench.py                 # Python fast-path throughput
```

## Modules

| file | what |
|---|---|
| `content.rs` | ROM -> content pack (wf-extract, in-process) cached in `~/Library/Caches/wf-buu/agent` (`WF_CACHE`, `WF_CONTENT`, `WF_ROM`); `GameData` once per process, render packs on first screenshot |
| `env.rs` | `Agent` = wf-core `Session` + per-frame input log of every slot, keyframe snapshots every 120 frames (rewind = restore + <= 120 re-steps), named saves (snapshot + input log), structured events, per-slot autopilots, per-enemy AI-state timers, look-ahead `forecast`, replays |
| `obs.rs` | the observation (below), the on-hit-tree analysis behind `takes`, the AI chain behind `ai` / `attacking`, ASCII map, field info |
| `macros.rs` | policies (one button mask per frame): sequence, BFS walk on the 4 px footprint lattice, attack (punch spots incl. enemies next to walls, re-plan, corner unsticking), face, align, select / use skill, dash, block; `run_policy` with `stop_on` |
| `controller.rs` | Rhai controllers: sandboxed, metered, deterministic |
| `catalog.rs` | fighters / forms / skills, enemies (group, resistances, presets, default arena), arenas, story presets |
| `tools.rs` | the 29 tools; `mcp.rs` the protocol; `capi.rs` the C ABI |

## Tools

Discovery `list_fighters`, `list_enemies`, `list_arenas`, `list_presets`, `list_areas`; sessions `new_battle` (preset
or enemy + count, fighter / form / level / skill tier / stat spend, seed, `players` for co-op), `new_field` (an
overworld area), `list_sessions`, `close_session`, `fork_session`; reading `observe`, `map`, `screenshot` (gba / wide /
arena view, ids drawn, scale, `save_to`); acting `press`, `wait`, `move_to`, `face`, `attack`, `use_skill`, `block`,
`dash` (all with `slot` and `stop_on`: hurt, kill, hit, no_damage, phase, danger); time `save_state`, `load_state`,
`rewind`; replays `get_replay`, `play_replay`; policies `run_controller`, `evaluate_controller`, `set_autopilot`;
mods `modcheck`, `list_mod_content`.

**Mods** (engine/docs/modding.md §7): `new_battle` / `new_field` / `new_game`, the `list_*` discovery tools,
`evaluate_controller`'s `battle` and the C ABI's gym take `mods` (mod folder paths; `[]` = none; default `WF_MODS`,
a `:`-separated path list). The content (base pack + mods, with the mods' scripted hooks) is cached per mod set and
rebuilt when a mod's files change, so an agent can edit a mod and start a new session. A session keeps its content
(forks too); `list_sessions` shows its mods; screenshots draw the mods' art. A replay records the mods' ids, the hash
of their files and their folders: `play_replay` loads those folders (or `mods`) and refuses other content with both
lists named. `modcheck` returns the checker's report as JSON (`ok`, `exit_status`, `errors` [{file, what, hint}],
`lines`); `list_mod_content` lists per mod what it adds / replaces / patches / derives / deletes, grouped by kind
(skills, enemies, items, equipment, areas, events, scripts, art) with the records' ids and names.

Every action returns `{result: {frames, stopped, dealt, hp_lost, hits_dealt, hits_taken, kills, ep_change,
hits_without_damage?, healed_enemy_hp?, ...}, observation}`.

## What changed against the prototype (the DESIGN.md recommendations)

* **True snapshots.** `save_state` / `load_state` / `rewind` / `fork_session` restore a `World` clone (~2 µs; 8.9 µs
  through ctypes incl. the input log copy) instead of re-simulating the input prefix. Re-simulation stays the
  verification path: `play_replay` rebuilds from the config and the RLE input log and compares the 64-bit
  `World::state_hash` (every bit of state, not a hand-picked digest).
* **Structured engine events** from `wf_core::world::Event::Damage` + before/after HP: `hit {by, target, effect,
  hp_change, computed, target_hp, crit}` where `effect` is `damage` / `healed it` / `no damage (resisted /
  blocked)` / `no damage (its current state ignores this attack)` - the "hit landed but nothing happened" case
  that confused the Yakon play test is now explicit, and summed in the action result (`hits_without_damage`,
  `healed_enemy_hp`). Also `hurt {by, hp, blocked, note}`, `defeated`, `spawn`, `phase`, `skill`, `grabbed`, `ko`,
  `outcome`, and field events (area enter / leave, transitions, scripts the host would run, pickups). Names are
  resolved before the step, so a target removed by its last hit keeps its name. Own hits collapse per target
  when there are more than 6.
* **`takes` per current AI state.** The on-hit tree of the enemy's *current* top state is run abstractly for a
  melee and an energy hit (IfAttack decided by the kind, other conditions both ways, SubState followed): `damage`,
  `heals it`, `no damage`, `damage (conditional)`, scaled by the resistance bytes. This turns Yakon's post-combo
  window into a field that flips from `energy: heals it` to `energy: damage (conditional)` while `state: "0x323"`,
  and Janemba's `ShockStun` into `no damage`. Cached per state.
* **First-class threat list and a forecast from the engine.** `threats` = hostile shots (team 0/2) and Janemba's
  hands with size, velocity (from the previous frame) and `eta` by extrapolation. `danger` = the engine's own
  look-ahead: the session is snapshotted into a scratch session and stepped 45 frames standing still, and again
  holding R: `{hit_in, by, hp, if_blocking}`. It needs no knowledge of attack kinds, covers melee, beams, hands,
  grabs, and answers "does blocking help" directly. Cost ~0.1 ms per observation.
* **Wind-up / intent.** `ai` (last three meaningful nodes of the running chain, reconstructed from the node
  runtimes), `state` + `in_state` (tracked per enemy from the AI stack entry serial), `attacking {kind, node, t}`
  when the active leaf is an attack node.
* **Deterministic metered sandbox for controllers.** Rhai (the mod language, integer-only build with the feature
  set of `wf-script-rhai`): operation budget per decision (`max_ops`), call depth / string / array / map limits, no
  `eval`, no imports, `print` into the result log (never stdout, which carries the protocol). A runaway loop ends
  with `controller_error: Too many operations` instead of a wall-clock timeout, so controllers are as deterministic
  as the core: same seed -> same hash.
* **Multi-seed evaluation**: `evaluate_controller` runs the controller from frame 0 for N seeds on all cores
  (`std::thread::scope`, one Rhai engine per thread) and returns win rate, mean HP lost, mean frames to win,
  per-seed outcome + hash.
* **Macros as hints.** Controllers get `obs.suggest.attack`: what the `attack` macro (BFS path to a punching spot,
  turning, punch timing) would press now for the nearest enemy, with target hysteresis. Controllers delegate
  navigation to it and add reflexes on top; this removed every stuck-at-a-tree run.
* **Co-op slots.** `players: N` (or a list of fighter specs); every action takes `slot`; slots not acted for run
  their autopilot (`idle`, `attack`, or a Rhai controller); enemies target slots round-robin (`targets_slot`);
  replays carry all slots. Netcode (wf-net) is not wired: two agents on one session take turns through the same
  server.
* **Field sessions** (`new_field`, `list_areas`) on the core's new field mode: an area with its placed enemies,
  NPCs, breakables, pickups and exits; `observation.field` lists exits with destinations and the other triggers.
  Scripts, dialogue and NPC events are not executed (the script host lives outside wf-core).
* **Screenshots** from `wf-render` (the pixel-exact compositor) on a `RenderView` built read-only from the world;
  enemy ids in yellow.

## Performance (Apple M-series, release; ranges = two runs while other builds shared the machine)

| path | throughput |
|---|---|
| core re-simulation (`play_replay`, recording off) | 4 744 frames in 1.6 ms (~3 M frames/s) |
| Python `step_frames` (one C call for an input list) | 0.9-1.3 M frames/s |
| Python gym loop: `step(mask)` + 124-int `obs_vec` + reward, frame_skip 1 / 4 | 34-86 k decisions/s / 230-290 k frames/s |
| Python step + full JSON observation incl. 45-frame forecast | 3-13 k frames/s |
| snapshot + restore through ctypes | 9-17 µs |
| `run_controller` (Rhai every frame, 20-frame forecast, attack hint) | 30-100 k frames/s (one thread) |
| `evaluate_controller`, 32 seeds Janemba -> Super Janemba (153 k frames), 18 threads | 0.9-1.3 s |
| MCP round trip (`wait 1` incl. observation + forecast, Python client over stdio) | ~0.1 ms |

Field sessions: `new_field {area: "8-3"}` -> fought the placed Majin Fighters, `move_to` an exit from
`observation.field.exits`, arrived in 8-4 (`area_leave` / `area_enter` events); the replay reproduced the hash.

## Play results (all through the MCP protocol: `python/mcp_client.py` <-> `wf-mcp` over stdio; `playtest.py`)

Step by step = the calls an LLM agent makes (observe -> `attack {stop_on: [hurt, phase, danger]}` / `block` when
`danger.if_blocking` says it helps / step off a hand / `use_skill`), controller = one `run_controller` call.
Every session's replay was re-simulated with `play_replay`: all hashes reproduced.

| fight | how | result | frames | HP left | tool calls | replay hash ok |
|---|---|---|---|---|---|---|
| 4x Majin Fighter (Goku Lv60) | step-by-step | won | 1866 | 118/135 | 16 | True |
| 4x Majin Fighter (Goku Lv60) | controller melee.rhai | won | 1814 | 135/135 | 2 | True |
| 6x Majin Fighter (Goku Lv70) | step-by-step | won | 1880 | 247/247 | 8 | True |
| 6x Majin Fighter (Goku Lv70) | controller melee.rhai | won | 1359 | 247/247 | 2 | True |
| Dabura (Goku Lv80) | step-by-step | won | 1578 | 383/383 | 4 | True |
| Dabura (Goku Lv80) | controller melee.rhai | won | 3817 | 379/383 | 2 | True |
| Janemba -> Super Janemba (Goku Lv110) | step-by-step | won | 4274 | 645/993 | 32 | True |
| Janemba -> Super Janemba (Goku Lv110) | controller melee.rhai | won | 4253 | 898/993 | 2 | True |
| Yakon (Goku Lv100, skill lv 2) | step-by-step | won | 844 | 741/748 | 120 | True |
| Yakon (Goku Lv100, skill lv 2) | controller yakon.rhai | won | 609 | 723/748 | 2 | True |
| Yakon (Goku Lv80, skill lv 1) | controller yakon.rhai | won | 3815 | 307/383 | 2 | True |

Controllers over 32 seeds (`evaluate_controller`):

| fight | controller | seeds | win rate | mean HP lost | mean frames to win | wall ms |
|---|---|---|---|---|---|---|
| 4x Majin Fighter (Goku Lv60) | melee.rhai | 32 | 1.0 | 2.8 | 1693.6 | 390.0 |
| 6x Majin Fighter (Goku Lv70) | melee.rhai | 32 | 1.0 | 3.2 | 1284.3 | 355.0 |
| Dabura (Goku Lv80) | melee.rhai | 32 | 1.0 | 4.1 | 2789.9 | 501.0 |
| Janemba -> Super Janemba (Goku Lv110) | melee.rhai | 32 | 1.0 | 142.6 | 4778.8 | 885.0 |
| Yakon (Goku Lv100, skill lv 2) | yakon.rhai | 32 | 0.938 | 22.6 | 696.3 | 2815.0 |
| Yakon (Goku Lv80, skill lv 1) | yakon.rhai | 32 | 0.281 | 47.3 | 3790.7 | 11955.0 |

Notes from the fights:
- **Yakon**: the observation made the strategy discoverable without prior knowledge: `takes.energy` reads `heals it`
  in every state except `0x323` (`in_state` counts the ~60-frame window), the events of a beam outside the window
  say `effect: "healed it"`. Step by step: block while `danger` says so, wait in 6-frame steps with `stop_on:
  ["danger"]`, `use_skill Kamehameha {align: true, charge_frames: 60}` when the window opens - one window kills him
  at Lv100 / tier 2. At Lv80 he summons Mini-Yakons (0x325 ignores all hits) and EP becomes the limit: the
  controller punches for EP between windows (won 9/32).
- **Janemba -> Super Janemba**: the attack macro alone wins at Lv110 (phase 1 has `ShockStun` states that ignore
  hits - visible as `hits_without_damage`); `attack` without a target waits through the 120-frame phase gap.
- **Dabura**: pinned against the wall by knockback, never hits back (prototype finding reproduced).
- **Groups**: the forecast-based block (`if_blocking: "no HP lost"`) and hysteresis on the nearest target took the
  controller from 56 % to 100 % wins on 4 Majin Fighters at Lv60.

## Gaps

- Netcode: co-op/PvP is local slots only; the core has no player-vs-player damage, and wf-net / remote agents are
  not wired.
- Field sessions do not run scripts / dialogue / cutscenes (needs the script host); KO in the field does not end
  the session; `evaluate_controller` is battle-only.
- `takes` is a static reading of the on-hit tree: random / counter conditions show as `(conditional)`.
- The forecast assumes you stand still (and other slots idle); it does not predict reactions to your input.
- Observation deltas (only what changed) are not implemented; observations are 0.5-2 KB.
- No pyo3 module: Python goes through ctypes (no compiler needed; numpy-free lists).
- Screenshot ids can overlap the sprite's head; beam piece chains and a few effects are not drawn by the core's view
  adapter yet (core.md §8).
