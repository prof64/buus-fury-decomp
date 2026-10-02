# Battle simulator — engine core and the ruleset API

The battle simulator (`#/battle`) is a game-agnostic engine (`core/`) plus one **ruleset** per game
(`rulesets/<game>/`). LoG II (`rulesets/log2/`), LoG I (`rulesets/log1/`) and Buu's Fury (`rulesets/buu/`) are implemented.

```
site/src/battle/
  core/            game-agnostic engine (no game constants in here)
    geom.ts        Dir (0 down,1 up,2 left,3 right), Rect, strict overlap test, asr
    rng.ts         Rng interface + RecordingRng (call history for the debug panel)
    input.ts       GBA buttons (KEYINPUT bit order), keyboard + Gamepad mapping, per-frame edges, injection
    loop.ts        fixed-timestep loop at the GBA rate (59.7275 Hz), pause / speed / frame advance
    renderer.ts    240x160 (or widescreen) offscreen buffer, integer-scaled blit, camera, debug text
    sprites.ts     SpriteSet/SpriteBank from data/<game>/sprites.json (anchors, flips, per-direction anims)
    pixelfont.ts   3x5 bitmap font for crisp in-game text
    world.ts       World: entity list updated last->first each frame, popups, camera, debug events, stats
    ruleset.ts     the Ruleset / RulesetBattle interfaces (below)
    session.ts     BattleSession: World + ruleset battle + loop + renderer + input; pause, outcome, onEnd
  rulesets/index.ts  registry (enable a game by replacing its stub)
  rulesets/log2/     Legacy of Goku II
  ui/                setup screen, battle view (toolbar + debug panel), sprite previews, CSS
site/tests/battle/   vitest suites (formula, RNG, level-up, AI cadence, move replays vs emulator)
```

## Frame model

`BattleSession.step()` runs once per GBA frame:

1. `InputSource.sample()` → `{held, pressed, released}` (taps shorter than a frame are latched).
2. START toggles pause (core); otherwise `battle.preStep?(world, input)`.
3. `world.step(input)`: `frame++`, `rng.frame = frame`, every entity's `update(world)` **from last to first**
   (Webfoot `World::update` order; entities added during the frame start next frame), removed entities dropped.
4. `battle.postStep?(world)` (camera, phase changes, ...), then `battle.outcome(world)`.

Rendering is independent (`requestAnimationFrame`): `world.draw(renderer)` y-sorts entities by
`layer` then `depth()`, then `battle.drawHud()`; debug labels are drawn at display resolution.
Game logic must never read the renderer; use `world.camera` (the GBA-sized logical camera) for
"off-screen" rules — widescreen only widens what is drawn around it.

## Implementing a ruleset

```ts
interface Ruleset {
  info: RulesetInfo                 // id 'log1' | 'log2' | 'buu', title, enabled
  load(): Promise<void>             // fetch data/<game>/*.json + preload sheets (idempotent)
  fighters(): FighterChoice[]       // playable characters + their forms (setup screen)
  enemies(): EnemyChoice[]          // the full enemy/boss catalog; ids are opaque strings
  arenas(): ArenaChoice[]           // background image + walkable bounds rect (world px)
  presets(): PresetChoice[]         // optional multi-phase story fights
  levelRange: [min, max]
  knobs(): SetupKnob[]              // extra setup controls (stat bonuses -> setup.bonus, options -> setup.options)
  preview(setup): StatLine[]        // stats the fighter will start with
  controls(): ControlHelp[]         // what each GBA button does in this game
  createRng(seed): Rng              // the game's own generator; `seed` is a user number
  arenaRuntime(id): ArenaRuntime
  sprites(): SpriteBank             // for setup previews (Visual.sprite ids)
  createBattle(cfg, world): RulesetBattle   // spawn the player + phase 1 into `world`
}

interface RulesetBattle {
  preStep?(world, input); postStep?(world)
  drawHud(renderer, world)          // draw into renderer.ctx at native resolution
  outcome(world): 'running' | 'won' | 'lost'
  debugLines(world): string[]       // debug panel: player state, each enemy's AI node, RNG state
  summary(world): StatLine[]        // extra rows on the result screen
}
```

Entities implement `core/world.ts` `Entity` (`update`, `draw`, `depth`, `layer`, optional `drawDebug` /
`debugLabel`). Use `world.stats` (damage dealt/taken, hits, crits, kills, EP spent) so the result screen works,
`world.logEvent('damage', …)` for the damage log, `world.popup()` for floating numbers and
`world.debugRect()` for attack boxes in hitbox mode. Record RNG calls with a label
(`rng.rand(26, 'dmg spread')`) — the debug panel shows them.

Checklist for a new game (log1 / buu):

1. `rulesets/<game>/rng.ts` — port the generator (extend `RecordingRng`, implement `step/getState/setState`).
2. `damage.ts` — integer port of the damage routine; add vectors to `data/battle/<game>_*.json` and a test.
3. `data.ts` — shape `data/<game>/*.json` (characters, moves, enemies, AI, sprites).
4. Player/enemy entities + the enemy AI runtime (interpret the decoded AI data if it exists; mark fallbacks
   with `EnemyChoice.faithfulAi = false`).
5. `index.ts` exporting a `Ruleset`; register it in `rulesets/index.ts` and set `info.enabled = true`.

## LoG II specifics (rulesets/log2)

| file | what | source of truth |
|---|---|---|
| `rng.ts` | 2×u32 xor/rotate generator 0x080213FC, `rand`, `chance`; seed = frames on the title screen | notes Part D §5 |
| `damage.ts` | computeDamage 0x0800F910 with ARM 32-bit wrap; attack table 0x086DB2CC | notes Part A §4 |
| `progression.ts` | level(exp), level-up 0x08003CA0 (exact RNG order), kill exp clamp | notes Part D §4 |
| `player.ts` | state machine 0x083B5278: idle/walk/dash/recoil/hurt/death, punch, special melee, 14 move types, forms + controllers (SSJ drain, Piccolo regen, normal EP regen) | notes Part B, disassembly of 0x08007F34 / 0x08008030 / 0x0800811E / 0x080082C8 / 0x08014DF4 |
| `projectiles.ts` | ki blast, pellets, Masenko, Big Bang, Burning Attack, Sword wave, Spirit Bomb, Kamehameha/SBC/boss beams, enemy shots, lob rock, laser, spray | moves.json, enemies.json projectiles |
| `ai.ts` | interpreter for all 53 AI node types of ai.json (stack of states, behaviour + on-hit trees) | notes Part C §4-7, controller disassembly 0x0801AA46-0x0801AB64 |
| `enemy.ts` | class rows, MeleeStrike geometry 0x0801BD2C, melee-range boxes 0x0801BEB8/0x0801E8B4, death nodes, drain grab, spray | notes Part C |
| `hud.ts` | captured HUD + ROM move icons (tools/battle/extract_icons.py) | ui captures |

Approximations are listed in `NODE_NOTES` (ai.ts) and commented where they occur (e.g. visual frame choices,
arena walls as a rectangle, Patrol without waypoints).

## LoG I specifics (rulesets/log1)

LoG1 runs on Webfoot's older engine: one generic enemy AI driven by template parameters, no attack stats (player
damage comes from max HP or Ki spent), 60 Hz. Fights take place on the real maps with their collision layers.

| file | what | source of truth |
|---|---|---|
| `rng.ts` | main RNG 0x08005CA8 (`randInc(n)` = [0,n] incl. the signed `muls` overflow for n >= 32768), secondary RNG 0x0800A6F4 (`rand2`) | notes §8, `re/log1/model.py` |
| `damage.ts` | apply_damage 0x08007352 (defense roll), punch, ki blast, enemy melee, cap, charge cost, Solar Flare length | notes §2-4 |
| `data.ts` | level table, 34 enemy templates (+ sprite footprints/hurtboxes), maps, items, collision bits | `data/log1/*.json`, `data/battle/log1_collision.json` |
| `game.ts` | char structs, awake/dormant lists, AI tick 0x08008018, decision 0x0800755A, attack 0x0800F774, flee 0x0800F9AC, wander 0x0800F6A0, pathfinder 0x08006F1A + A* 0x08007A4E (heap, quirky closed list, 8-expansion budget), hit() 0x08007416, set_char_state 0x08007F4C, Solar freeze 0x080087A0, camera 0x08002134 | disassembly `work/log1_dis.txt` |
| `player.ts` | Goku states 0x0816696C: 4-way movement 0x08002CE0, punch, Ki pose, flight 0x08002A88, msg 1 handler | notes §2-3 |
| `enemy.ts` | enemy entity 0x081667F0: walking sync, Melee/Ki/Death anims, punch + projectile handler 0x08005604 | notes §3, §5 |
| `projectiles.ts` | Ki Blast (player/enemy), Kamehameha beam, Solar Flare | notes §3.2, §9, `data/log1/moves.json` |
| `hud.ts`, `items.ts` | captured HUD + icons, flight box; map pickups, herbs / Senzu (SELECT) | ui captures, items.json |

Frame order (as the ROM): `preStep` = frame counter, AI tick (also player Ki regen), then every entity's update
(last to first), then `postStep` = every entity's anim_tick. Quirks kept on purpose are marked `QUIRK` in the
code (enemies keep their old path while the goal tile is unchanged, the attacker's path index reset by hit(),
the separation loop advancing the wrong register, goal.y used as x in one pathfinder fallback, 45-frame
post-attack immunity, negative melee damage healing — toggle "Authentic bugs").

Approximations (`APPROX` in `index.ts`): secondary-RNG advance per frame (punch values differ from a real run),
no story scripts for bosses, Kamehameha beam drawn procedurally, no sound, item use via SELECT, breakable
obstacles not placed.

## Buu's Fury specifics (rulesets/buu)

| file | what | source of truth |
|---|---|---|
| `rng.ts` | the LoG2 generator (identical constants, 0x0803FAAC); `seed` = number of draws from (0,0) | notes §1 |
| `damage.ts` | ComputeDamage 0x0800E9D8: crit (82*STR + crit bonus), STR/20 melee scaling, END >= 200 clamp, attack factor, rand(26) spread, resist byte, frontal block; HP in 1/1024 with the remainder carried; attack-type table 0x086B8088 | notes §4, formulas.json |
| `progression.ts` | level(xp), LevelUp 0x08013678 (4 x rand(256) per level, exact order), RecalcStats 0x08013C66 with the equipment modifiers and effect scripts (crit %, block / EP-cost factors, thorns, regen, Saiyan set, Crisis Ring, Holy Water), fusion 0x0801375E | notes §3, §6 |
| `data.ts` | characters / forms / skills / 162 equipment / 150 enemy types / AI states + on-hit trees / maps; gfx id = sprite index + 9 | data/buu/*.json, data/battle/buu_ai_extra.json |
| `player.ts` | walk 5*SPD+102, dash +320 (double tap 20 f), melee 0x08012A24 (one hit on the press, 17 f), block (R) and energy shield (R+B), hurt 24 f with zero resists, regen timer, SS / SS3 gauges, the 23 skills (costs from the cost table, scaled by rec+0x56), +1 EP per damaging melee hit (script #2) and the hold states enemies use | notes §3-§5, moves.json, attacks_detail.json |
| `ai.ts` | interpreter for all 178 behaviour node types + the extra reaction-state nodes; on-hit (aux) trees with the onHit return protocol (0 / 1 / 2) | notes §8 / §8b, enemy_ai.json, tools/battle/gen_buu_ai_extra.py |
| `enemy.ts` | HP <<10, hit handler 0x0800FAFA (ComputeDamage on every accepted hit, then the aux tree whose ApplyDamage takes the HP), melee box 0x0802A0C4, HeavyStrike, death node (type +0x14 word +8), Kid Buu split / merge, Janemba parts / hands, worm segments, summons | notes §8b, bosses.json |
| `projectiles.ts` | ki blast, growing beams (0x080206B8), instant beam (0x08036FA2), bullets / rifle / shells / shuriken / wraps / spit, energy wave, flames, bombs, Buu stretch arm, candy beam, Super Buu ball, Super Janemba wave, dark sphere, grapple arm, Broly's rocks, Big Bang / Masenko / Kamikaze ghosts | bosses.json projectiles |
| `hud.ts` | captured HUD (hud.png) + skill icons from the ROM (tools/battle/extract_buu_icons.py) | ui captures |

The aux (on-hit) trees and the reaction states 0x00..0x11 are not in enemy_ai.json; `tools/battle/gen_buu_ai_extra.py`
decodes them from the ROM into `data/battle/buu_ai_extra.json`. Bosses whose aux trees have no ApplyDamage take no
damage, exactly as in the game (verified with `tools/battle/buu_boss_hp_check.py`: Majin Buu 0x1A / 0x1B and the
scripted Super Buu 0x77 lose 0 HP in the emulator); their presets are "survive 60 s" / "bat the ball back 3 times".

Fitted / inferred (marked in the code): skill state machines are fitted to the measured runs (first-hit frames,
hit counts, EP), beam hit rate near / far, Big Bang multiplier = 0.284 x charged EP, path-finding replaced by
straight moves, some table-driven offsets (candy beam, formation slots), energy shield effect, thorns, SS cooldown
start value, transform not interruptible, arena walls as rectangles, cutscene scripts not run.

## Tests and tools

* `npm test` (vitest, `site/tests/battle/`):
  * `log2.test.ts` — RNG sequences, 67 damage rolls recorded in the emulator + 300 reference cases (crits,
    scales), the f(S) table, the attack table, kill-exp clamp, the emulator level-up (Piccolo 40→41).
  * `moves.test.ts` — replays 42 of the 45 player-move measurement runs (`re/log2/pl_measurements.json`) and
    requires the same hits, frame by frame: attack id, scale, and the attack rectangle relative to the player.
  * `sim.test.ts` — walk/dash speeds, aggro radii (snake 72 px, wolf 80 px) and every boss measured in
    `bosses.json` (hit cadence + damage per hit vs a passive Lv40 Piccolo).
* `tools/battle/gen_vectors.py` — records damage / level-up vectors from the emulator (`data/battle/log2_vectors.json`).
* `tools/battle/gen_formula_vectors.py` — reference vectors from `re/log2/formula.py`.
* `tools/battle/extract_icons.py` — HUD move icons from the ROM → `site/public/assets/log2/battle/icons/`
  (run once after `tools/extract/extract_all.py`; the HUD shows no icon without them).
* LoG1: `log1.test.ts` — 113 apply_damage calls and 10 punches recorded in the emulator, level table columns, and
  11 frame-by-frame replays of emulator recordings (Goku moves/flight/ki moves, wolf chase, wolf pack, Frieza
  soldiers walking/charging/ki blasts that hit, Raditz, squirrels fleeing, pterodactyl on the flying layer): every
  frame the same positions, states, stun/activity timers, HP and Ki, and the same hit() calls.
  `log1_sim.test.ts` — every enemy / arena / preset runs headlessly with a bot; cap, heal bug, regen, flight, Solar.
* `tools/battle/gen_log1_vectors.py` — records the LoG1 vectors and traces (`data/battle/log1_vectors.json`);
  `tools/battle/log1_collision.py` — collision bits of the arena maps; `tools/battle/log1_icons.py` — HUD icons.
* Buu: `buu.test.ts` — 78 ComputeDamage calls recorded with the RNG state (melee, energy, crits, END >= 200,
  enemy hits, blocks, i-frames), the 3162 enemy -> player hits of all 79 regular enemy types (each value must be a
  formula output), the attack table, LevelUp vectors, RecalcStats / fusion; `buu_moves.test.ts` — 172 skill
  measurement runs (first-hit frame, multiplier, attack type, hit count); `buu_sim.test.ts` — aggro radii (100 /
  88 / 72 px), chase / wander speeds, Majin Fighter melee damage, HeavyStrike reach + timing, vampire drain,
  Blister spawns, invulnerable story Buus, Janemba timeline, Super Janemba wave damage, Kid Buu split,
  Spopovich + Yamu drain; `buu_smoke.test.ts` — all 150 types / presets / fighters' skills run.
* `tools/battle/gen_buu_vectors.py`, `gen_buu_enemy_hits.py`, `gen_buu_ai_extra.py`, `extract_buu_icons.py`,
  `buu_boss_hp_check.py` — Buu vectors, enemy hit table, aux trees, HUD icons, boss damage check (emulator).
* `tools/battle/play.py` — drives the running page through `chrome-devtools-axi` (start a setup, inject
  button sequences frame-exactly, screenshot). `window.__battle` exposes the live `BattleSession`.
  `BT_GAME=log1|buu` picks the ruleset, `BT_URL` / `BT_SESSION` another dev server / browser session.
* `tools/battle/map_preview.py`, `pl_vtables.py` — helpers used to pick arena rects / read player state vtables.
