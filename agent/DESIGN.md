# Agent interface for the Buu's Fury battle simulator - design and play-test findings

Prototype: `agent/mcp` (MCP server over the TS simulator) + `agent/skill/SKILL.md`. Goal: find the interface an LLM
agent can actually play with, then carry it over to the Rust core (`engine/`).

## Tools

All tools return JSON text (`screenshot` returns an image). Action tools return
`{result: RunSummary, observation}` with
`RunSummary = {frames, stopped, dealt, hp_lost, hits_dealt, hits_taken, kills, ep_change, ...}` and
`stopped` one of `done | arrived | target_defeated | hits_done | skill_done | max_frames | hurt | kill | hit | phase |
battle_won | battle_lost | controller_stop | controller_error | not_usable (...) | unreachable ...`.
`stop_on` (every action) = subset of `["hurt","kill","hit","phase"]`.

| tool | input | notes |
|---|---|---|
| `list_fighters` | - | id, forms, move list with EP costs |
| `list_enemies` | `search?, group?, limit?` | id (`t18`), group (Boss/Enemy/Story group/Helper), hp, stats, resist tags, presets using it |
| `list_arenas` | - | id, name, walkable bounds |
| `list_presets` | - | story chains (phases) + arena |
| `new_battle` | `fighter?, form?, level?, enemy? (id or name), count?, preset?, arena?, seed?, skill_level?, stat_spend?` | -> `{session, spec, observation(full + map)}`; arena defaults to the enemy's own map |
| `list_sessions` / `close_session` | `session` | many sessions side by side |
| `observe` | `session, map?, full?` | see below; drains events |
| `screenshot` | `session, view? (gba/wide/arena), scale?, labels?` | the game's renderer, ids drawn over enemies |
| `press` | `session, buttons?, frames?, steps?[{buttons,frames}], stop_on?` | low level, buttons like `"RIGHT+A"` |
| `wait` | `session, frames, stop_on?` | |
| `move_to` | `session, x, y, max_frames?, stop_on? (default hurt)` | BFS on the walk grid, re-plans when stuck |
| `face` | `session, target? | direction?` | 1-frame tap (turns without moving) |
| `attack` | `session, target?, hits?, max_frames?, stop_on?` | approach a punching spot, turn, punch on every new press |
| `use_skill` | `session, skill (name/slot), charge_frames?, taps?, gap?, target?, align?, steer?, max_frames?, stop_on?` | selects with L, lines up, holds B, waits until it can act |
| `block` | `session, frames, shield?, stop_on?` | R (+B = energy shield) |
| `dash` | `session, direction, frames?, stop_on?` | waits for idle, double tap, hold |
| `save_state` / `load_state` | `session, name` | |
| `rewind` | `session, frames` | any depth (back to frame 0) |
| `get_replay` | `session` | `{format, spec, inputs: [[mask, frames]...], frames, hash}` |
| `play_replay` | `replay (JSON string), keep?` | new session, re-simulated; `deterministic: hash == expected` |
| `run_controller` | `session, code, frames, every?, stop_on?, timeout_ms?` | JS `(obs, mem) => buttons` per frame in node:vm |

### Observation

```json
{"session":"s3","frame":237,"outcome":"running",
 "player":{"pos":[242,203],"facing":"left","hp":[118,135],"ep":[77,90],"state":"hurt","can_act":false,
           "skill":{"slot":0,"name":"Energy Blast","ep":5,"usable":true},"holding":"UP"},
 "enemies":[{"id":3,"name":"Majin Fighter","pos":[230,200],"hp":[500,500],"dist":12,"dir":"left","facing":"right",
             "ai":"MeleeAttack","melee":"in_range"}, "..."],
 "projectiles":[{"kind":"hand","team":"hostile","pos":[124,126],"size":[23,28],"vel":[0,0],"armed_in":10,"eta":0}],
 "walls":{"down":25,"up":143,"left":160,"right":160},
 "events":["f174 you hit #4 Majin Fighter with punch for 46.5 (its hp 454)","f237 #3 Majin Fighter hit you: -17 HP"],
 "sfx":["Hit impact B","Melee swing A"]}
```

- Positions are the footprint centre (feet) in world px - the frame of reference of collision, movement and
  `move_to`. `dir` / `dist` are relative to the player so the agent rarely needs arithmetic.
- `melee` is the single most useful field: the punch box of the current facing (and of each other facing) tested
  against the enemy's body box. `in_range` -> press A, `turn_<dir>` -> tap that direction first.
- `ai` = the running behaviour-tree chain without control-flow nodes (`IfPlayerWithin2>GotoState`,
  `ChasePlayer`, `MeleeAttack`, `ShockStun`): names come straight from the decoded AI and work as tells.
- `takes` = damage profile derived from the data (resist bytes + on-hit trees): `melee x0.05`, `energy heals it`,
  `all: no damage`. Added after Yakon healed to full from a Kamehameha in the play test.
- `eta` (frames until a hostile shot overlaps you, linear extrapolation) turned dodging from geometry into a threshold.
- Events: the sim's damage log rewritten into sentences with enemy ids; >6 own hits collapse into one line per
  target. `full: true` adds skills (+ cost at the chosen skill level, `usable`), stats, arena; `map: true` adds a
  16 px ASCII map. A normal observation is 0.4-1.5 KB.

## Implementation notes

- The simulator is imported unmodified (`../../site/src`); esbuild stubs `site/src/data.ts` (Vite `import.meta.glob`)
  and `import.meta.env`. Only the ruleset's public surface is read (positions, hp/ep, `state`, `skills`, `slot`,
  `canUseSkill`, `ai` stack, `fits`, `bodyBox`), defensively, so the ongoing player rewrite should not break it.
- **Save / load / rewind = re-simulation of the input prefix.** The world cannot be deep-cloned (entities, the battle
  object and the context close over their creation scope), but the sim is deterministic and runs ~200k frames/s
  headless (a 4000-frame fight re-simulates in ~5-10 ms), so a snapshot is a copy of the input log. Loading a state
  discards later inputs (a branch).
- Replay = spec + RLE input log + FNV hash of (frame, RNG state/calls, every actor's position/HP/state/AI chain, other
  entities). Verified: replays of fights that used rewinds, load_state branches and screenshots reproduce the hash.
- Screenshots run the game's own `World.draw` + HUD through a ~300-line software Canvas 2D (`canvas.ts`: transforms,
  alpha, `lighter` / `source-atop`, rect clip, nearest-neighbour `drawImage`) with a synchronous fake `Image` that
  decodes `site/public/assets` PNGs with pngjs. No native canvas dependency; rendering has no effect on the sim
  (hash checked).
- Macros are generators yielding one button mask per frame, so they are just input producers: replays, rewinds and
  determinism need nothing special.
- Controller sandbox: fresh vm context, `codeGeneration.strings = false`, observation passed in as a JSON string and
  the result returned as one (no host objects reachable), 50 ms per call. `obs.constructor.constructor('return
  process')()` -> "Code generation from strings disallowed"; `while(true){}` -> timeout. node:vm is not a security
  boundary; fine locally.
- Audio: the ruleset's audio bus is a module-level sink; the session installs its own sink around each step to count
  the game's PlaySfx calls by name (data/buu/audio.json).

## Play tests (all through the real MCP protocol: SDK client <-> stdio server, `scripts/daemon.ts`)

| fight | how | result |
|---|---|---|
| 3x Majin Fighter, Goku Lv80, forest | `attack` x3 | won in 364 frames, lost 33/383 HP |
| 4x Majin Fighter (preset), Goku Lv60 | `attack` macro only, 4 calls | won in 1058 frames, 44/135 HP left |
| same | `run_controller` with `scripts/ctl_group.js` (punch if anything in range, block a neighbour in `MeleeAttack`, turn, approach) | won in 866 frames, 98/135 HP left |
| Dabura (t18, 10 000 HP), Goku Lv80 | `attack` with `stop_on:["hurt"]` x2 | won in 1590 frames without taking a hit: the knockback pinned him against the arena wall |
| PuiPui -> Yakon -> Dabura (preset), Lv80 | step by step | PuiPui 1 punch; Yakon: Kamehameha and blasts did 0 - found that his on-hit tree heals fully on energy hits and melee is x0.05 (led to `takes`); not finished (≈250 punches) |
| Janemba -> Super Janemba (preset, 8880 + 25 840 HP), Goku Lv110 | phase 1: `attack` + `rewind` to inspect the hand that hit, `press RIGHT` to dodge it, `attack`; phase 2: `run_controller` `scripts/ctl_melee.js` | won (frame 4084), 669/993 HP left; `play_replay` reproduced the hash |
| Janemba at Lv60 | `attack` | hopeless (18 dmg/punch vs 8880 HP) - level matters more than tactics |

`npm run selftest` replays the Majin Fighter and Janemba wins and the determinism checks.

### What was awkward (and what changed because of it)

1. Ids in events: the sim's log names enemies by label only; with 4 identical "Majin Fighter"s that was useless.
   Events now carry `#id` (matched by label + logged HP, else nearest).
2. Event spam: a 600-frame `attack` produced 23 hit lines. Own hits now collapse per target; the result summary has
   the totals.
3. Hazards that are not projectiles (Janemba's hands are actors without `rect()`) were invisible in the JSON while the
   screenshot showed them. Hazards now include them with `armed_in` / `gone_in`. Lesson: a generic "things that can
   hurt you" list must come from the engine, not from class checks.
4. `preset: "dabura"` silently matched the 3-phase "Babidi's ship ... -> Dabura" chain (substring). Exact id first.
5. `dash` from the walk state did nothing: the double tap is only read by the idle state. The macro waits for idle.
   Also: dashing into a wall bumps you back - `walls` in every observation tells which way is free.
6. Energy Blast taps closer than ~25 frames are dropped (refire window). Default `gap` is 26 now; the result's
   `ep_change` exposes how many shots really fired.
7. `skill_level` 2 (the default) makes Kamehameha cost 150 EP, more than a Lv60 Goku has: `usable:false` in the
   observation explained it, but the agent has to know to lower `skill_level`.
8. The "ai" field first showed the leaf only ("GotoState"), which says nothing; the last three non-control-flow nodes
   (`IfPlayerWithin2>GotoState`, `IfPlayerAttackingMe>Chance>GotoState`) read like intent.
9. Damage popups in screenshots look like id labels; ids are yellow and above the head, popups white.
10. Macros vs controllers: macros are the right granularity for planning (one call = one intention, 50-1500 frames)
    but cannot react inside the call; `stop_on:["hurt"]` makes them interruptible. Controllers react every frame
    and were clearly better in melee brawls (2.2x less HP lost vs 4 fighters), cost one call per thousands of frames,
    and are easy to iterate with `save_state`/`load_state`. The best loop was: macros + rewind to learn the fight,
    then encode the learned rule in a controller.

## Recommendations for the Rust-core version

Keep:
- The pause-between-calls model, frames as the time unit, `{result summary, observation}` from every action.
- Deterministic replays (spec + input log + state hash) and branchable time travel. In Rust, make the world
  `Clone` (or snapshot-serializable) so `save_state` is O(state) instead of re-simulating; keep re-simulation as the
  verification path.
- Macros as pure input generators (same log, same replay), plus `stop_on` interrupts.
- Agent-oriented derived fields: `melee` in-range/turn hint, `dir`/`dist`, `eta` for hostile shots, `walls`,
  `takes` damage profile, `can_act`, `usable` per skill with the cost at the current skill level.
- The controller mode; in Rust run it in a real sandbox (QuickJS/Boa or WASM with fuel metering instead of
  wall-clock timeouts, so controllers are deterministic too) and consider letting the agent register named
  controllers that macros can call.
- Screenshots from the real renderer (the Rust core already has the extractor; render to an RGBA buffer -> PNG).

Change / add:
- Emit structured events from the engine (hit {attacker_id, target_id, attack, dmg, crit, blocked, iframes},
  spawn, defeat, phase, state change, absorbed/healed hit) instead of parsing log text; include "hit accepted but no
  damage" - the most confusing moment of the Yakon test was hits that made sounds and did nothing.
- A first-class "hazards" query (anything with an active hit box that can damage the player, with activation
  windows) instead of inferring it from entity classes.
- Expose the enemy's next-attack tell explicitly when the AI enters a wind-up node (a `telegraph: {kind, in_frames}`
  field) - the AI node names were good enough here but required knowing the vocabulary.
- Batch / headless evaluation tool: run a controller over N seeds and return win rate + mean HP lost (the sim is fast
  enough to do this in one call), which would let agents optimise policies properly.
- Return `observation` deltas (only what changed) for long sessions, and make `full` / `map` cacheable static parts.
- Keep ids stable across rewinds (they are, because of determinism) and document it - agents reuse ids from
  earlier observations after a rewind.
