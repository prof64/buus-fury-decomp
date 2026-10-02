# Battle simulator — engine core and the ruleset API

The battle simulator (`#/battle`) is a game-agnostic engine (`core/`) plus one **ruleset** per game
(`rulesets/<game>/`). LoG II is implemented (`rulesets/log2/`); LoG I and Buu's Fury plug in the same way.

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
* `tools/battle/play.py` — drives the running page through `chrome-devtools-axi` (start a setup, inject
  button sequences frame-exactly, screenshot). `window.__battle` exposes the live `BattleSession`.
* `tools/battle/map_preview.py`, `pl_vtables.py` — helpers used to pick arena rects / read player state vtables.
