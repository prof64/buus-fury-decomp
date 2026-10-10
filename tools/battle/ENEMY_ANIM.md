# Buu's Fury: how enemies and bosses animate and what they play

Tags: **VERIFIED** = reproduced tick for tick from emulator recordings; **INFERRED** = read from code, or seen but
not exercised on purpose.

```
cd re/buu && ../../.venv/bin/python ea_batch.py far,near,mid,punch,kill,move,blast 0-149 900   # record (≈10 min, 3 shards in parallel fine)
.venv/bin/python tools/battle/gen_buu_enemy_anim.py          # -> data/battle/buu_enemy_anim.json + buu_enemy_anim_sim.json
.venv/bin/python tools/battle/gen_buu_enemy_anim_traces.py   # -> data/battle/buu_enemy_anim_traces.json
cd site && npx vitest run tests/battle/buu_enemy_anim.test.ts
```

| file | what |
|---|---|
| `re/buu/ea_rec.py`, `ea_batch.py` | recorder: force-spawn a type (`forcespawn.py`), run a scenario, log every enemy's raw anim / frame / tick / facing / state object / AI state / sprite def per frame, every SetAnim / SetAnimRaw / SetState / SetAnimHold / PlaySfx / StopSfx / object spawn with its caller, and the logic tick counter → `work/buu/eanim/<type>_<scenario>.json` |
| `re/buu/ea_seg.py` | cuts recordings into state segments and checks the shared tick rules against them |
| `re/buu/ea_show.py`, `ea_view.py`, `ea_death.py`, `ea_ctl.py`, `ea_tables.py`, `ea_watchtick.py`, `ea_freeze*.py` | viewers / probes used while decoding |
| `tools/battle/gen_buu_enemy_anim.py` | the data: SetAnim table, tick rules, every state object (controller), every SFX site, per enemy type its states, rules, sequences, sound cues |
| `site/src/battle/rulesets/buu/eanim.ts` | the simulator's model (`EnemyAnim`), wired into `enemy.ts` |
| `site/tests/battle/buu_enemy_anim.test.ts` | replays 95 recordings (19 types) through `eanim.ts`, plus an in-sim punch / sound check |

Scenarios (player HP pinned): `far` (player parked 110 px left), `mid` (64), `near` (36), `punch` (player next to the
enemy pressing A every 24 frames), `kill` (enemy HP 1 first), `move` (player walks a square), `blast` (player at 90 px
pressing B every 48 frames). 150 types × 7 = 1 050 recordings, 900 frames each.

## 1. Mechanism

**Frames are code-driven, as for the player** (`tools/battle/LOCKSTEP.md`, `panim.ts`). An enemy shows the animation
struct at obj+0x50 (= def+0x50 + 4·raw; `raw` = sprites.json `raw_index`) at frame obj+0x11. Its **state object**
(obj+0x14) owns the timing. Per logic tick the actor update `0x0800EF08` runs the obj+0x18 controller, the state's
tick (vtable +4), the state's control (+8); then the enemy update `0x0800FAE6` runs the AI tree (`0x0800AB0C` on
obj+0x238). So **the tick runs before the AI**: a state the AI enters shows its first frame with tick 0 on the entering
tick; one entered from another actor's update (a hit) has already ticked once (tick 1). VERIFIED (45 173 segments).

**Calls** (all VERIFIED by breakpoints):

| call | address | effect |
|---|---|---|
| SetAnim(id) | `0x0800ECF0` (vfunc +0x28) | raw = `0x08072670[id] + 1`; state = **enemy** table `0x080726B8[id]` via SetStateId `0x0800FAB6` (vfunc +0x2C) |
| SetAnimRaw(n) | `0x0800ECDA` | obj+0x50 = def anim n+1 (and the halo table +0x4C); state, frame and tick untouched |
| SetState(obj) | `0x0800ED18` | old state's exit (vt +0xC), obj+0x14 = new, new state's enter (vt +0) — re-entering the same state re-runs enter |
| SetAnimHold(id, f) | `0x0800ED3E` (vfunc +0x30) | raw of id, state = hold `0x03001B00`, frame = f |
| SetSprite | `0x0800EB60` | (re)loads the def's graphics and calls the **current state's enter** again (INFERRED from a watchpoint: an idle enemy's tick re-randomised with no SetState call; 2 cases in 95 traces) |

**Enemy SetAnim table** (`setanim` in the JSON; ROM tables, IWRAM state singletons read from a save state):

| id | raw | state object | rule | ticks per frame | end |
|---|---|---|---|---|---|
| 0 | 1 idle | `0x03001B0C` (vt `0x0804A77C`) | idle: enter tick = **rand(255)** (game RNG); 2-frame anims blink (frame 1 while (tick & 255) > 240), else loop shift 3 | - | - |
| 1, 2 | 2 walk, 3 | `0x03001B58` | loop | 8 | - |
| 3, 9 | 4, 10 hurt | `0x03001B10` | once | 8 | SetAnim(0) |
| 4, 5, 6 | 5, 6, 7 punches | `0x03001B50` | once; **enter plays 50 + rand(2)** (vol 64) | 4 | SetAnim(0) |
| 7 | 8 guard | `0x03001B04` | hold (frame 0) | - | - |
| 8 | 9 | `0x03001B08` | bored (tick >> 5, then frames 2.. loop) — never seen on an enemy | 32 | - |
| 10, 11 | 11, 12 | `0x03001B30` | once, end callback does nothing (last frame kept) | 8 | none |
| 12, 17..22 | 13, 19..24 | `0x03001B18` | once | 16 | SetAnim(0) |
| 13, 14 | 14, 15 | `0x03001B58` | loop | 8 | - |
| 15 | 16 | `0x03001B48` | loop | 8 | - |
| 16 | 18 knockdown | `0x03001B40` | once | 32 | hold `0x03001B00` (last frame) |

Tick rules (`ticks` in the JSON): once `0x0800DE6C`, loop `0x0800DEB0`, idle `0x0800DFD4`, bored `0x0800DF8A`, hold
`0x0804E4E0`, plus **reverse** `0x08018C24` / `0x0802F224`: tick − 1, wrapping to nframes·8 − 1, frame = tick >> 3 —
the walk cycle played backwards (state `0x03002B9C`, entered with SetAnimRaw(1) by the back-off node
`0x08018E29` "StepTowardPlayer", which moves *away* while facing the player; also `0x03003100`, CirclePlayer).
VERIFIED: 44 937 / 45 173 generic segments follow their rule tick for tick; the 236 others are explained in §4.

**Attack / special controllers** are per-node state objects (71 kinds; `controllers` in the JSON). Their enter sets the
raw (SetAnimRaw) and their tick both steps the frame and does the work (spawns, sounds) on fixed ticks, then calls
SetAnim(0) or lets the AI end it. They are given as **observed sequences** (`[raw, frame, ticks]` run-lengths from the
entering tick), with `fixed` = every run the state ended itself had the same length. 1 244 / 1 293 complete runs are
prefixes of the stored sequence (interrupted by a hit or the AI); the rest are controllers whose length depends on the
game (shield up/down, charge times). Examples decoded from code (VERIFIED against the sequences):

| controller | used by | timing |
|---|---|---|
| KiBlast `0x0804D37C` @`0x0300305C` | Atomizer, Laser Turret… | raw 21; frame = tick >> 3 while tick < 16 (blast spawned at tick 8); frame 1 to tick 64, then 2; SetAnim(0) at tick 72 |
| ki-blast pose `0x0804D358` @`0x03003068` | Majin Buu, Super Buu, Pikkon, Broly | raw 4; frame 0 ×16, frame 1 ×16 (blast + sound 20/21 at 16), SetAnim(0) at 32 |
| gun `0x0804ADF0` @`0x03002FC4` | Gunman, Assassin, Sniper | raw 19 frames 0..3 ×4, sound 28 at tick 4, SetAnim(0) at 16 |
| flame `0x0804C6FC` @`0x03002FA8` | Annihilator, Death Machine, Destroyer | raw 19 frame 0, SetAnim(0) at tick 128; sound 63 from the flame spawner |
| candy / power-up `0x0804C260`, `0x0804D6F0` | Majin Buu forms, Super Buu | raw 21 / 20 loops; sound 11 at 32, 64, 96 / sound 8 at 64 |
| Kid Buu Kamehameha `0x0804C690` (heap) | Kid Buu | raw 4: 0 ×9, then 1/2 alternating ×8 for 91 ticks; beam sounds 3 (charge, tick 0), 32 (fire loop, tick 9), hits 48/49, 33 at the end |
| Final Flash `0x0804D7D0` (heap) | Majin Vegeta | raw 20, 181 ticks; 32 at 24, hits 48/49 |
| Janemba ascend / descend `0x0804CC34`, `0x0804D3C8` | Janemba | raw 20 0 ×9, 1 ×8, 2 ×113; then 2 ×67, 1 ×8, 0 ×23 |

**How AI nodes pick animations** (`set_by` per state; VERIFIED call sites):

| node / code | call |
|---|---|
| path follower `0x0803B934` (ChasePlayer, MoveToPoint) | SetAnim(1) when it starts moving |
| ChasePlayer arrive / stand `0x0802A02A`, node exits `0x0804E50C`, StandStill | SetAnim(0) (re-enters idle each time: new rand(255)) |
| MeleeAttack `0x0802A0C4` | hit test, then **SetAnim(4 + rand(n))**, n = punch animations in the sprite (raw 5/6/7 present; create fn `0x0802A288`); rand is drawn even when n = 1 |
| back-off `0x08018E29` | SetAnimRaw(1) + reverse state |
| AlignWithPlayer / StepAwayFromPlayer / WalkToPlayer / FlyToPlayer / Wander | SetAnim(1) when moving, SetAnim(0) when not |
| Block `0x08018837` | SetAnim(7) (guard, frame 0) |
| shared hit reaction (aux tree of AI state 0, `0x08018A92`) | SetAnim(9) hurt (once, 8 ticks per frame, then idle) unless blocking; Janemba `0x0804E974` likewise |
| launch / death nodes `0x08015944` | SetAnim(16) knockdown (32 per frame, then held) or SetAnim(12) |
| attack / special nodes | their controller (table above); see each type's `states` |

**Movement speed never changes the timing**: the walk is a fixed 8-ticks-per-frame loop at every speed (VERIFIED).
The reverse: WalkToPlayer (Spopovich) takes no step on walk frames 1 and 4. **Idle fidget** = only the 2-frame blink
(15 of every 256 ticks, phase random per idle entry); no enemy reached the bored animation.

**Multi-OBJ bosses**: a frame is one record with up to 64×64 OBJ pieces; timing is the same per actor. Multi-actor
bosses (Janemba's body parts 0x34/0x35/0x37 and hands, Kid Buu copies, mini-Yakons, worm segments) are separate
enemies with their own states (recorded as their own types).

**Graphics swaps**: Mechanoids and shield soldiers change sprite def (`SetSprite`) under a running animation; the
struct pointer stays the old def's, so the recorder finds no raw (−1) until the next SetAnim — the old def's frames keep
showing. A def that shares one animation struct between slots (e.g. Dabura raw 4 = 5) is reported by its lowest slot
(`canon` in the traces).

## 2. Sounds

All PlaySfx calls are `0x0803E378(voices, entry, vol, pan)`, id = (entry − `0x083192A8`) / 12. Per type, `sfx` lists every
site with ids / volumes / scenarios; `sfx_at` gives the tick offset inside the state that was running. Enemy sounds
(VERIFIED unless noted):

| event | ids | site | when |
|---|---|---|---|
| punch swing | 50 / 51 = 50 + rand(2) | punch state enter `0x0800DF1A` | the tick the punch state is entered (after the hit test) |
| melee hit lands on the player | 48 + rand(2); gfx 0x60/0xE5/0xDC → 30, 0xEE/0xF4/0xED → 62 | `0x0802A258` (MeleeAttack) | same tick, before the swing |
| heavy / Bio Mech punch hits | 48 / 49 | `0x0803CD1A`, `0x0803C9F0` | on the hit |
| ki blast launch / hit | 20 / 21, 48 / 49 | `0x0801984A`, `0x080199E0` | blast spawn (tick 16 of the pose), on contact |
| gun / rifle / grenade launch | 28 | `0x08026586` (gun), rifle controller | tick 4 (gun) / 8 (rifle, Hessian, Marauder, Mercenary) |
| shuriken | 61 | ninja throw controller | tick 8 |
| flame | 63 | `0x08023FF8` | while flaming |
| engine / tank loop | 52 | `0x0803BBDE`, AlignWithPlayer `0x0801CF2E` | Annihilator, Death Machine, Destroyer moving |
| Atomizer / Cyborg blast | 35 | builder `0x0802A2EC` | on spawn |
| Majin Buu power-up / candy | 8 / 11 | `0x0802ABE2`, `0x0802B07C` | tick 64 / 32, 64, 96 of the state |
| beams (Kid Buu, Majin Vegeta, Yakon) | 3 charge, 32 fire loop, 33 end, 48/49 hits | `0x08020764`, `0x08020828`, `0x080207A0`, `0x080209D6`, Final Flash `0x080370C8` / `0x0803722C` | |
| Bones / Lich wind-up hit | 62 | `0x08033CEC` | |
| Broly, Kamikaze ghost contact | 49 / 48 | `0x0803618E`, `0x080362FA`, `0x080390F8` | on contact |
| Dabura special | 21 | `0x08027D3A` | INFERRED (one occurrence) |

No sound for: enemy death, enemy hurt (the 48/49 there is the *player's* hit sound `0x08012AA4`), block, walking.
Level-up after a kill (36, script native `0x0800083E`) is the player's.

## 3. Hurt, knockback, death

- Hurt: SetAnim(9) from the shared reaction, once 8 ticks per frame, end → idle; blocking enemies show guard (frame 0)
  instead. VERIFIED (3 546 entries). Knockback motion: `enemy.ts` (separately verified).
- Launch / knockdown: SetAnim(16) (raw 18, 32 ticks per frame) then held on the last frame (Spopovich, Super Buu).
- Death of a regular enemy (`0x08018950`): removed from the world at once, explosion object `0x0801952C` at the body
  centre (rate 0x1000 + rand(0x1000)); **no death animation, no sound**. VERIFIED (kill recordings).
- Boss defeats are scripted cutscenes (death node "Idle", `buu_ai_extra.json`); force-spawned bosses block or do not
  die in the `kill` scenario, so their defeat animation is not recorded (gap).

## 4. What is not the animation model (seen in the recordings)

- **Lag frames**: when a frame's work overruns, the main loop shows a video frame with no game update (logic counter
  `0x03000F94` unchanged; e.g. the frame after the player is hurt). Recordings drop them; a simulator steps logic ticks.
  VERIFIED (watched: no actor update ran).
- **World-loop skips**: the world update `0x08011A1E` walks its list downwards while objects leave it, so an actor can
  miss (or repeat) one tick (4 cases in 95 traces). INFERRED mechanism; the traces mark them (`skips`).
- Script pauses (level-up after a kill) freeze actors for a tick.
- `SetSprite` re-entering the current state (above).

## 5. Data (`data/battle/buu_enemy_anim.json`)

`setanim`, `ticks`, `controllers` (`vtable@object`, heap ones `@heap`: enter / tick / end functions, rule, raws,
users, call sites), `sfx_sites`, `sfx_names`, `verification`, and `entities["0xTT"]` for all 150 types:
`sprite`, `def`, `anims` (raw → name, frames), `states` (per state object and entering raw: `rule`, shift, `sequence`,
`fixed`/`exit`, `set_by` call sites with node names, `ai_states`, `sfx_at` cues, `verified` counts), `sfx`, and `sim`
(the compact rules the simulator loads, also written alone to `buu_enemy_anim_sim.json`: per raw the most used state,
per controller key its rule; sequence rules carry their own sound cues).

## 6. Simulator (`eanim.ts`, `enemy.ts`, `ai.ts`)

`Enemy.setAnim` enters `EnemyAnim` with the type's recorded rule for the raw (else the SetAnim table), `tickAnim` runs
the rule before the AI (a one-shot's end does SetAnim(0)), `frameIdx` = obj+0x11. Idle entries draw rand(255) and punch
entries 50 + rand(2) from the game RNG, MeleeAttack plays its hit sound and picks 4 + rand(punch anims), the back-off
node plays the reversed walk, WalkToPlayer reads the real frame. Test: 95 traces, 78 926 ticks compared, 105 not modelled
(controllers seen only once), all match; plus an in-simulator Majin Fighter check (punch frames 0..3 × 4 ticks, swing and
hit sounds).

Gaps: boss defeat cutscenes; controllers whose length the game decides (shield, charge) are only as long as the AI
keeps them; the 105 unmodelled ticks; sounds made by projectile / beam objects are listed per type but only the punch,
melee-hit and the controllers' own cues are played by `eanim.ts` (ki blasts and beam hits already were).
