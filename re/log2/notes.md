# Dragon Ball Z: The Legacy of Goku II — game mechanics (reverse engineered)

ROM: USA `DBZLGCYGOKU2ALFE` (`roms/log2.gba`). Everything here comes from disassembly of the ROM plus
observation in the headless emulator (`tools/emu`). Tags: **VERIFIED** = disassembly + emulator check;
**DISASM** = read from code, not run; **INFERRED** = reasoning or partial evidence.

Data: `data/log2/{characters,moves,enemies,ai,maps,items,levels,formulas,rng}.json`, all regenerated from the ROM by
`.venv/bin/python re/log2/build_all.py` (runs every `re/log2/extract_*.py`).

## Summary of key results

| topic | result | status |
|---|---|---|
| Damage | `dmg = (((((rand(26)+256)*((power*(1+crit)*(128-END))>>7))>>8)*f(S)>>18)*scale>>10)*MULT>>7`, `f(S)=(((26S-701)S+115618)S)>>8`, S=STR (melee) / POW (energy), crit = chance(164*STR/65536), melee only | VERIFIED (130+ hits incl. charged specials) |
| RNG | 2x32-bit xor/rotate generator at `0x080213FC`, state `0x03003890`; rand(n), chance(p) | VERIFIED |
| Exp | table `0x081D9834` (Lv1..50), kill exp clamped to [span>>7, span>>1] of the current level, active character only | VERIFIED |
| Level-up | stats +[min,max]/256 per level (table `0x081D95F0`), maxHP x1.055..1.075, maxEP +2..4, full heal, cap 100.0 | VERIFIED |
| Stats | STR (melee dmg + crit), POW (energy dmg), END (defense: dmg x (128-END)/128); raised by levels and STR/PWR/END Caps (+1/+3/+5) | VERIFIED |
| Regen | active char +3% max EP per ~596 frames, no HP regen; benched chars +5% HP/EP per tick | VERIFIED |
| Player | walk 1.25 px/f, dash 2.5 px/f (double tap), one-hit A melee, 32-frame hitstun with i-frames, 14 special move types, 22 forms (SSJ etc.) | VERIFIED |
| Enemies | 109 class rows, 152 AI states built from ~50 node types, 666 spawns over 327 maps, 19 drop tables | VERIFIED (core) |
| Debug menu | hidden title option 2 -> Music/Sample/Map Test (warp anywhere, party Lv40) | VERIFIED |


# Part A — Tooling, engine, core formulas

### 0. How to reproduce / tooling

* ROM: `roms/log2.gba` (USA, `DBZLGCYGOKU2ALFE`, 8 MB). All addresses are GBA bus addresses (ROM = `0x08xxxxxx`).
* Python helpers in `re/log2/`: `common.py` (ROM access + capstone Thumb disassembly with literal-pool resolution),
  `d.py ADDR N` (disassemble), `litref.py VAL` (find `ldr rX,=VAL`), `callers.py ADDR` (find `bl ADDR`), `adrref.py`,
  `xref.py`/`xrange.py` (raw pointer search), `vt.py VTABLE` (decode a relative vtable), `vtfind.py FN` (find vtables holding FN),
  `bp.py`/`wtrace.py`/`w1.py` (breakpoint / watchpoint traces while playing a key script), `runto.py` (run until an
  instruction is *about to execute*, by single-stepping the last frame - lets you inspect registers/RAM at any PC),
  `play.py` (scripted input + screenshot grid), `warp.py MAPIDX [VAR] [OUT]` (load any map through the debug menu).
* **Debug menu (VERIFIED)**: the title-screen object (`0x0202B324` in `log2_title.ss`, method `0x08000434`) keeps the cursor in byte
  `+8` and copies it to `+9` on A/START; `+9==2` constructs the unused debug menu (`0x080056B0`, vtable `0x08023EE8`)
  with *Music Test / Sample Test / Map Test (Zone, Area, Variation)*. Writing `2` to `title+8` and pressing A opens it.
  "Map Test" (`0x08005C0A`) loads map record `menu+0x14` (index into the map table, see §2) with variation `menu+0x18`,
  first giving **all six characters exp for level 40 (`EXP[39]`) via the real level-up routine and all move-unlock flags
  (`record+0x14 |= 0x3F`)**. `re/log2/warp.py` automates it (`work/states/log2_debugmenu.ss`).
* Save states made by this agent: `work/states/log2_title.ss` (title), `log2_debugmenu.ss` (debug menu open),
  `log2_prologue_trunks.ss` (new game, Future-Trunks prologue in control), `log2_map12.ss`/`log2_map12b.ss`
  (East District 439, Piccolo Lv40, snakes + wolves - used for all damage verification). Sub-agents add `log2_pl_*`,
  `log2_en_*`, `log2_it_*` states.

### 1. Engine overview

#### Object model (VERIFIED)
The game is C++ (Webfoot engine). Objects start with a vtable pointer; **vtable entries are self-relative**: method =
`vtable_base + entry` (`ldr r1,[obj]; ldr r2,[r1,#slot]; adds r2,r2,r1; bl 0x08021D2C (bx r2)`). `0x08021D28..36`
are `bx r0..r7` trampolines. `re/log2/vt.py` decodes a vtable. Common slots for world objects (from the enemy vtable
`0x080252A4`, base `0x08025564`, player `0x08025604`):

| slot (offset) | role |
|---|---|
| 0 (0x00) | destructor |
| 2 (0x08) | per-frame update (called by `World::update`) |
| 3 (0x0C) | on-screen / visibility test (collect for drawing) |
| 4 (0x10) | draw |
| 9 (0x24) | `onMessage(type, data)` (broadcast handler) |
| 10 (0x28) | set state (player: `setState(id)`) |
| 14 (0x38) | apply hit (enemy `0x0800E484`, player `0x08008806`/`0x08008784`) |

#### Singletons (IWRAM, VERIFIED unless noted)
ROM keeps small tables of singleton pointers that code loads (`0x083B5268`, `0x083B5348`, `0x0840DAF4`, `0x081DA8C8`, ...):

| address | object |
|---|---|
| `0x03001120` | **World**: object list (`+4` count, `+8..` pointers), second list at `+0x40C`, delayed-event queue at `+0x610` (count) / `+0x614..` |
| `0x03000E90` | **Party**: `+0` active character index (0 Gohan, 1 Piccolo, 2 Vegeta, 3 Trunks, 4 Goku, 5 Hercule), 6 records of 28 bytes at `+0x0C` (see §3) |
| `0x03001FA8` | pointer to the player entity |
| `0x0300229C` | input: `+0x12` keys held, `+0x14` pressed this frame, `+0x16` released this frame (GBA key bit order) |
| `0x03002080` | current map: `+0` zone, `+1` area, `+4` pointer to map record (table `0x08409368`) |
| `0x03001090` | camera (`0x0300109C`/`0x030010A4` = left/right edges, from the recomp reference; INFERRED) |
| `0x03003890` | RNG state (2 x u32) |
| `0x030026BC` | text/HUD system (message boxes go through `0x0801FC9C` on `+0x5C4`) (INFERRED) |

#### Frame loop (VERIFIED by breakpoints)
Gameplay scene update `0x08004CAC` -> `World::update 0x080069A4`:
1. Delayed-event queue (`World+0x610`): each event has a countdown at `+4`; at 0 it is removed, then `vt[0x0C]` (fire) and
   `vt[0x08]` (destroy) are called. Used for exp awards (1 frame, vtable `0x080245B0`), game over (200 frames, `0x08023A50`), ...
2. Every object in the World list, last to first: `vt[0x08]` update.
Then `0x08006AD8` draws: objects whose `vt[0x0C]` returns true are collected, quick-sorted by depth
(comparator `0x08027E74`, i.e. y-sorting) and drawn with `vt[0x10]`; `0x08006AA6` flushes the overlay list (`World+0x208`).
Scenes/menus are pushed with `0x0801F2F4` (scene stack); START in the field opens the pause menu (Journal/Status/Items/Sleep/Save/Switch).

#### Messages and hit detection (VERIFIED)
`0x08006DA6 broadcast(list, type, data)` calls `onMessage` on every object of both World lists and ORs the results.
Attacks are delivered by broadcasting **type 0** (or 2) with a hit packet:
`data+0` u8 team (0 = hostile to player, 1 = player's attack, 2 = hits everyone), `data+4..+0x13` attack rectangle
(x0,y0,x1,y1, pixels), `data+0x14..` damage message: `+0x14 u8 kind`, `+0x18 scale (0x400=1.0)`, `+0x1C` attack
descriptor pointer, `+0x20` attacker entity, `+0x24` crit flag (out), `+0x28` pre-multiplier damage (out), `+0x2C` damage (out).
Enemies accept team 1/2, the player accepts team 0/2. Receivers test their hurtbox (`0x080079B8`) against the rectangle
and, on overlap, run `computeDamage` (`0x0800F910`, §4) then their hit reaction / `apply hit` (`vt[0x38]`).
Type 0x14 is the scouter scan query; types 7/8/9 are player-internal (form apply / area enter/leave).

#### Entities (shared base, VERIFIED fields only)
| offset | field |
|---|---|
| `+0x000` | vtable |
| `+0x154`,`+0x158`,`+0x15C` | position (`+0x158` x, `+0x15C` y, 24.8 fixed point; `+0x154` layer/z) - setter with map collision `0x08007D24` |
| `+0x160..+0x164` | combat stats bytes: STR, POW, END, melee-taken multiplier (/128), energy-taken multiplier (/128) |
| `+0x168` | "can be hit" flag checked by hit tests |
| enemy `+0x180` | pointer to class row (table `0x086D56AC`) |
| enemy `+0x184` | class row `+0x14` (death/drop behaviour) |
| enemy `+0x188` | current HP (init = class HP) |
| enemy `+0x19C` | AI controller (see enemy section) |
| player `+0x188`/`+0x18C` | 1.25 / 2.5 (floats written by the normal-form apply `0x08016FDA`; movement speeds - see player section) |

### 2. ROM data map (master list)

| address | contents | decoded by |
|---|---|---|
| `0x080E2840`/`0x080E2880` | UI string pointer table (names, items, move names/descriptions, locations) | items/notes |
| `0x081D9548` | initial party records (6 x 28 bytes) - new game | `extract_characters.py` |
| `0x081D95F0` | per-character level-up stat growth (6 x 3 x (min,max) u16, 1/256 units) | `extract_formulas.py`, `extract_levels.py` |
| `0x081D967C` | character forms table (22 x 20 bytes, indexed by record+8) | `extract_characters.py` |
| `0x081D9834` | exp table, 50 x u32 (exp needed for level 1..50) | `extract_levels.py` |
| `0x081D9900` | special-move type table (14 x 16 bytes) | `extract_moves.py` |
| `0x086D56AC` | enemy class table (109 x 24 bytes) | `extract_enemies.py` |
| `0x086D5194` | enemy ai_type table (8-byte entries) | `extract_ai.py` |
| `0x086DB2CC` | attack descriptor table (8 bytes: u8 type, u8 id, u16 0, u32 power) | `extract_moves.py` |
| `0x086F8E40` | scouter database (128 x 20 bytes: name, description, gfx id, portrait id, class row) | `extract_enemies.py` |
| `0x08409368` | map table (327 x 0x38 bytes) | `extract_maps.py` |

### 4. Damage formula (VERIFIED, 32/32 randomized hits)

`EntityBase::computeDamage(this = defender, msg)` at `0x0800F910` (r4 = msg, where msg = hit packet + 0x14):

```
0800f914  ldr r1,[r1,#8]        ; A = attack descriptor
0800f918  ldrb r2,[r1]          ; A.type
0800f91a  ldr r7,[r1,#4]        ; power = A.power
0800f91c  ldr r1,[r4,#0xc]      ; attacker
0800f920  adds r1,#0x160        ; attacker stats
0800f924  adds r6,this,#0x160   ; defender stats
0800f926  cmp r2,#0 ; bne energy
0800f92a  ldrb r5,[r1]          ; S = attacker STR
0800f92c  movs r0,#0xa4 ; muls r0,r5 ; bl chance   ; crit = chance(164*S)
0800f934  str r0,[r4,#0x10]     ; msg.crit
0800f936  adds r0,#1 ; muls r7,r0   ; power *= 1+crit
0800f93a  ldrb r0,[r6,#3] ; str r0,[sp]   ; MULT = defender melee multiplier
   energy: ldrb r5,[r1,#1] (S = POW) ; MULT = ldrb [r6,#4] ; crit = 0
0800f94a  ldrb r0,[r6,#2] ; movs r1,#0x80 ; subs r0,r1,r0 ; muls r0,r7 ; asrs r6,r0,#7   ; b=(power*(128-END))>>7
0800f954  movs r0,#0x1a ; ldr r1,=-701 ; muls r0,r5 ; adds ; ldr r1,=115618 ; muls r0,r5 ; adds ; muls r0,r5 ; asrs r5,r0,#8
          ; f = (((26*S-701)*S+115618)*S)>>8
0800f966  movs r0,#0x1a ; bl rand ; adds r0,#0x100      ; r = 256..281
0800f970  muls r0,r6 ; lsrs r0,#8 ; muls r0,r5 ; lsrs r0,#18
0800f976  ldr r1,[r4,#4] ; muls r1,r0 ; asrs r0,r1,#10 ; str r0,[r4,#0x14]   ; t = scale*t>>10
0800f980  ldr r1,[sp] ; muls r0,r1 ; asrs r0,#7 ; str r0,[r4,#0x18]          ; damage = t*MULT>>7
```

i.e.

```
crit   = melee ? chance(0xA4*STR) : 0          # P = STR*164/65536 = 0.25% per STR point
b      = (power*(1+crit)*(128-DEF)) >> 7
f(S)   = (((26*S - 701)*S + 115618)*S) >> 8      # S = STR (melee) or POW (energy)
r      = rand(26) + 256                         # +0..9.8% random
t      = ((((r*b) >> 8) * f(S)) >> 18) * scale >> 10
damage = (t * MULT) >> 7                        # MULT = defender's melee/energy taken multiplier (0x80 = 1.0)
```

Integer details: the two `lsrs` are unsigned 32-bit shifts of 32-bit products (products fit for all legal inputs).
Reference implementation: `re/log2/formula.py`; machine-readable form + `f(S)` table in `data/log2/formulas.json`.
Rough feel: with power 0x400, DEF 0, scale 1.0 a hit does `f(S)/256` damage = 1.75 (S=1), 17 (S=10), 54 (S=30), 153 (S=60), 466 (S=100).

Application: enemy `0x0800E484`: `HP -= damage`; `HP<=0` -> HP=0, broadcast death (type 0xF) and run the class row's
death behaviour (`row+0x14 -> vt[8]`). Player `0x08008784`: party record HP -= damage, clamped at 0; at 0 the player
enters state 0x21 (death) and a game-over event fires after 200 frames.

Player stat bytes (`entity+0x160..2`) are the integer parts of the party record's 8.8 STR/POW/END, written when the
current form is applied (normal form: `0x08016FDA`), multipliers `0x80`.

### 5. Experience (VERIFIED)
* `level(exp)` (`0x08004214`): highest L (1..50) with `EXP[L-1] <= exp`; EXP table `0x081D9834` (50 entries, `EXP[0]=0`, `EXP[49]=22,965,105`).
* Kill reward (`0x0800E4D2`, on enemy death): `E = class.exp`; if `E>5` and the active character is below Lv50:
  `D = EXP[L] - EXP[L-1]` (the exp span of the current level), `E = clamp(E, D>>7, D>>1)`; at Lv50 no exp. So every
  non-trivial enemy is worth at least 1/128 and at most 1/2 of a level. Granted through a 1-frame delayed event
  (vtable `0x080245B0`) to `0x08003D58(party, char, E)` which adds exp and calls the level-up routine
  `0x08003CA0(party, char, oldLv, newLv)` if the level changed. Observed: Lv40 Piccolo + 13-exp snake -> +4747 = (3,335,510-2,727,775)>>7.


# Part B — Player characters, moves, forms

## LoG2 — player-side mechanics (characters, moves, forms)

Scope: playable characters (Gohan, Piccolo, Vegeta, Trunks, Goku, Hercule), their controller/state
machine, melee, special moves, transformations, hitboxes. Data: `data/log2/characters.json`,
`data/log2/moves.json` (regenerate with `re/log2/extract_characters.py`, `re/log2/extract_moves.py`).

Tags: **VERIFIED** = disassembly + observed in the emulator; **DISASM** = read from code but not
separately measured; **INFERRED** = educated guess (string/order/consistency evidence).

Tools written for this (all in `re/log2/`): `pl_warp.py` (debug Map Test with any character —
patches the party byte right after the debug code forces Piccolo at 0x08005CC4), `pl_trace.py`
(per-frame player state/pos/EP), `pl_battery.py` + `pl_runall.py` (fire a move at a parked enemy,
log every damage calculation, EP changes, spawned projectiles), `pl_hits.py` (pauses at the damage
routine 0x0800F910 by single-stepping and recomputes damage with `formula.py`), `pl_transform.py`,
`pl_hurt.py`, `pl_formshots.py`. Logs: `work/log2/pl_bat_*.txt`, form sheet `work/log2/pl_forms.png`.
Save states: `work/states/log2_pl_{gohan,piccolo,vegeta,trunks,goku,hercule}.ss` (map 12 "East
District 439", Lv40 via debug Map Test, one snake/wolf nearby), `log2_pl_transformed.ss` (Vegeta SSJ).

---------------------------------------------------------------------------------------------------
### 1. Memory map

#### Party records (IWRAM) — VERIFIED
`0x03000E90` u8 active character index (0 Gohan, 1 Piccolo, 2 Vegeta, 3 Trunks, 4 Goku, 5 Hercule).
Record *i* at `0x03000E9C + 28*i` (pointer to 0x03000E90 lives at ROM 0x083B5354 / 0x083B526C):

| off | type | meaning |
|---|---|---|
| +0x00 | u16 | HP |
| +0x02 | u16 | max HP |
| +0x04 | u16 | EP (energy) |
| +0x06 | u16 | max EP |
| +0x08 | u8  | **form id** → form table 0x081D967C (sprite, transform target, special-melee params, form controller) |
| +0x09 | u8  | selected special slot 0..3 (L button cycles to the next *unlocked* slot: 0x08008B88 → 0x08004284) |
| +0x0A | u16 | STR, 8.8 fixed (cap 0x6400 = 100.0) |
| +0x0C | u16 | POW, 8.8 |
| +0x0E | u16 | END, 8.8 |
| +0x10 | u32 | experience |
| +0x14 | u32 | flags: bit0 joined/selectable, bit1 (?), **bit2 special melee learned**, **bits3..6 = slot 0..3 learned** |
| +0x18 | u8[4] | move-type id for each of the 4 slots (table 0x081D9900) |

Initial records: ROM 0x081D9548 (6×28 B, DMA'd by 0x08003C24 on new game). At start only flag
0x40 (slot 3 = basic energy blast) is set; everything else is unlocked by story scripts.
Unlock script builtins (event-script VM 0x080092E4; `02 xx` = call builtin xx from table 0x083B5D28;
args pushed with `1D n v1..vn` zigzag varints): 0x4F set bit0, 0x5B clear bit0, 0x50 set bit1, 0x51 clear
bit1, 0x52 unlockTransform (bit5 = slot 2), **0x53 `learn(char, n)` sets bit (4<<n)** (n=0 special melee,
n=1..4 slots 0..3) — handlers 0x0800A9B6..0x0800AA7C (DISASM). No level-based unlocks exist.
Learn calls found in the script data (`pl_unlocks.py`; `characters.json → move_unlocks`; located by
nearby dialogue, DISASM):

| script addr | unlock | context |
|---|---|---|
| 0x083B8637 / 0x083B868F | Gohan Masenko Ball, Piccolo Special Beam Cannon | "Three years later..." (start of Gohan's chapter) |
| 0x083CE4CB / 60B / 75B (dup 0x083E34B3..) | Gohan Super Kick, Piccolo Spin Punch, Vegeta Two-Handed Smash | "X now has the ... Technique!" |
| 0x083D1FD7 / FDD | Vegeta transform + Big Bang | "You are now playing as Vegeta." |
| 0x083DB64D / 653 | Gohan transform + Kamehameha | "GOKU: Hey, look who I brought home for dinner!" |
| 0x083DBC46..C5E | Goku transform, Flurry Punch, Kamehameha, Spirit Bomb | "Okay, here goes... / That should do it!" |
| 0x083DDF1F / F25 | Trunks Sword Blast, Vegeta Energy Punch | journal update / "Vegeta has achieved Level %d!" |
| 0x083DF2DB..2E7 | Trunks transform, Cross Slash, Burning Attack | Trunks joins |
| 0x083E0D6D / D75 | Piccolo Scatter Shot + power-up (slot 2) | "I must go." / journal update |
The debug Map Test (0x08005C0A) sets bits 0,1,2,3,4,5 for chars 0-4 and sets every char's exp to Lv40.

#### Player entity (EWRAM object, pointer at `0x03001FA8`) — VERIFIED unless noted
Player class vtable `0x08025604` (relative vtable: target = vtable_base + entry).

| off | meaning |
|---|---|
| +0x00 | vtable |
| +0x08 | anim speed byte (0x28 written by most anim states) |
| +0x10 | facing: 0 down, 1 up, 2 left, 3 right |
| +0x12 | u16 state/anim counter (most states count frames here) |
| +0x14 | u16 animation frame index |
| +0x18 | current state object (IWRAM singleton); `setState(n)` = vtable+0x28 → 0x080088BA → table **0x083B5278**[n] → 0x0800763C (old.exit, new.enter) |
| +0x1C | secondary/overlay state (aura, hit-flash) set by 0x08007662 |
| +0x3C | current animation frame box (s8 x, s8 y, u8 w, u8 h) |
| +0x48 | actor descriptor (from form.actor_id via 0x08009324 / table 0x083B4E74) |
| +0x4C | current animation pointer |
| +0x158 / +0x15C | x / y position, 24.8 fixed (pixels*256) |
| +0x160..+0x164 | STR, POW, END, melee-damage-taken mult, energy-damage-taken mult (u8; mult/128). Written by the form controller (record>>8 + bonus) |
| +0x168 | vulnerable flag (0 during special-melee release) (DISASM) |
| +0x184 | regen/drain accumulator for the form controller |
| +0x188 | walk speed (24.8) |
| +0x18C | run/dash speed (24.8) |

Globals: input object `0x0300229C` (+0x12 held, +0x14 pressed this frame, +0x16 released);
frame counter `[0x03000E1C+4]`; double-tap memory `0x03001F40`; RNG state `0x03003890`.

---------------------------------------------------------------------------------------------------
### 2. State machine (VERIFIED by per-frame tracing unless marked)

State objects are IWRAM singletons whose relative vtable has: [0x00] enter(stateobj, ent),
[0x04]/[0x08] per-frame update(s), [0x0C] exit, [0x14] "animation finished" (most states call it
when their counter reaches the length). Table 0x083B5278 (indices 0x00-0x33; entries 0x02-0x11
are generic actor states shared with NPCs/enemies).

| id | name | notes |
|---|---|---|
| 0x00 | idle | input handler 0x08007F34: d-pad → walk (sets facing), double-tap same direction within 20 frames → dash 0x0D, A → melee (0x08007DE8), B → selected special (0x08007EDA → table 0x081D9900). After 1024 idle frames → 0x0E idle animation |
| 0x01 | walk | 1.25 px/f per axis (entity+0x188); diagonals are **not** normalised (1.25 on both axes) |
| 0x0D | dash | run speed entity+0x18C (2.5 px/f normal form) along facing while held, perpendicular steering at walk speed (0x0800811E); blocked → 0x1F |
| 0x0E | idle anim | — |
| 0x1F | recoil (dash bump) | 32 f, pushed back 0.75 px/f |
| 0x20 | hurt | 32 f, knockback 0.75 px/f opposite to the player's **own facing** (table 0x083B51C8 = ±0xC0), damage mults = 0 (invulnerable) while in it; no i-frames after it ends (was hit again 33 f later). Entered from 0x08008806 only if damage > 0 |
| 0x21 | dying | HP 0 (0x08008784) → game-over object fires after 200 frames |
| 0x22 | energy blast | see §4 |
| 0x23 / 0x25 | transform / power-up | 0x23 ≈ 160-175 f (Goku, Vegeta measured), 0x25 = 64 f (Piccolo) |
| 0x24 | revert | 40-80 f, damage mults 0 |
| 0x27 | special melee | §3 |
| 0x28/0x29/0x2A | punch anims | random pick (rand(3)) after every A press, 16 f each |
| 0x2B | energy punch anim | 16 f |
| 0x2C..0x33 | Scatter Shot, Masenko, Big Bang, Burning Attack, Kamehameha, SBC, Sword Blast, Spirit Bomb | §4 |
| 0x05 | Hercule special | 32 f |

---------------------------------------------------------------------------------------------------
### 3. Melee

**Basic punch (A)** — VERIFIED. 0x08007DE8 builds an attack message on the **press frame** (no
startup): type-0 (melee) attack table id 2 (power 0x400), scale 0x400, rect = integer position +
`0x083B5144[facing]` (x0,y0,x1,y1):

```
down (0,0,16,16)   up (0,-24,16,0)   left (-8,-24,0,8)   right (16,-24,24,8)
```
Then a random punch animation state 0x28/0x29/0x2A runs 16 frames (0x0800E19C: counter+1, end at 0x10).
Maximum punch rate = one hit per 17 frames (idle reads A again the next frame). No combo chain:
every punch is identical (power 0x400). If nothing was hit the same press can "check/talk"
(rect table 0x083B5104, msg type 3).

**Special melee (charged A)** — VERIFIED (Goku/Gohan/Piccolo/Vegeta/Trunks runs).
Requires record flag bit2. Keep holding A through a punch: when the punch anim ends with A held
(0x080085AC) → state 0x27 (vt 0x08025E20, update 0x080083C2):
* charge: animation advances to `form.charge_anim_quads*4` frames, then a 16-frame countdown
  (state+8 = 0x10) runs; a charge-complete effect appears when it reaches 0.
* release before the countdown finishes = cancel (back to idle, nothing happens).
* release after: entity+0x168 = 0 (DISASM: not targetable), lunge `form.lunge` px/f along facing
  (unit dir table 0x083B51E8 × form+0x0C), every 4th frame k where bit k of `form.hit_quads_mask` is
  set → melee hit, **attack id 4 (power 0x600), scale 0x400**, rect 0x083B5208[facing] (same
  geometry as the punch). Ends after `form.anim_len_quads*4` frames.
* Goku (form 19: len 6, mask 0x18): two hits at +11/+15 f after release, 24 f total (measured).
  Trunks forms 12-17: len 7, charge 2 quads, mask 0x60. Piccolo: mask 0x1E (4 hits), no lunge.
  Hercule (form 21) has no special melee (mask 0).
Names (string table 0x080E2840 idx 0x9D+char): Super Kick, Whirlspin, Two-Handed Smash, Cross
Slash, Flurry Punch.

**Energy Punch (Vegeta, move type 7)** — VERIFIED: B → EP-4, immediate melee hit attack id 5
(power 0x600, scale 0x400) with the punch rects, 16-frame anim 0x2B.

---------------------------------------------------------------------------------------------------
### 4. Special (B) moves — move-type table 0x081D9900

Entry (16 B): `icon_gfx_a, icon_gfx_b, canUse(&rec.HP, player), execute(&rec.HP, player)`.
canUse always also requires the global "attacks allowed" flag `[[0x086DB27C+8]+0xC] != 0`.
`execute` subtracts the start cost and calls `player.setState(...)`.

Character slot assignment (record+0x18, VERIFIED) and names (UI strings 0xA7+char for slot 0,
0xB1+char for slot 1; consistent with every move's measured behaviour and description):

| char | slot0 | slot1 | slot2 | slot3 |
|---|---|---|---|---|
| Gohan | 4 Masenko Ball | 3 Kamehameha | 12 Super Saiyan | 2 energy blast |
| Piccolo | 9 Special Beam Cannon | 8 Scatter Shot | 11 power-up | 2 |
| Vegeta | 0 Big Bang | 7 Energy Punch | 12 | 2 |
| Trunks | 1 Burning Attack | 13 Sword Blast | 12 | 2 |
| Goku | 3 Kamehameha | 10 Spirit Bomb | 12 | 2 |
| Hercule | 6 | 6 | 6 | 6 |

| type | move | min EP | EP on start | ongoing cost | state | attack id (power) | scale |
|---|---|---|---|---|---|---|---|
| 2 | energy blast | 2 | 2 | 2 per extra shot | 0x22 | 3 (0x400) | 1024 |
| 4 | Masenko Ball | 4 | 4 | — | 0x2D | 7 (0x800) | min(2C+256, 2048) |
| 3 | Kamehameha | 12 | 2 | 86/256 EP per frame | 0x30 | 10 (0x40) per frame | 1024 |
| 9 | Special Beam Cannon | 8 | 2 | 34/256 EP per frame | 0x31 | 11 (0x66) per frame | 1024 |
| 8 | Scatter Shot | 12 | 12 | — | 0x2C | 6 (0x500) ×3 | 1024 |
| 0 | Big Bang | 20 | 10 | 1 EP / 4 f while held | 0x2E | 8 (0xC000) | 32 + frames held |
| 7 | Energy Punch | 4 | 4 | — | 0x2B | 5 (0x600) melee | 1024 |
| 1 | Burning Attack | 6 | 6 | — | 0x2F | 9 (0x40) | 1024 |
| 13 | Sword Blast | 5 | 5 | — | 0x32 | 12 (0x400 melee swing) + 13 (0xC00) wave | 1024 |
| 10 | Spirit Bomb | 20 | 10 | 1 EP / 4 f while held | 0x33 | 14 (0xA000) + 15 (0, stun) | 5 + frames until throw |
| 6 | Hercule special | 6 | 6 | — | 0x05 | 15 (power 0) to all | 0 |
| 11/12 | transform | 10 | 10 | form drain | 0x25/0x23 (0x24 revert) | — | — |
| 5 | unused | — | — | — | — | — | — |

Per move (all VERIFIED with `pl_battery.py`; constants DISASM with addresses):

* **Energy blast (0x22)**: shot leaves 8 frames after the press (counter starts at 8, fires when
  counter%16==0), projectile class vt 0x08024CBC (ctor 0x0800EAD8), 3.0 px/f, spawns ~18 px in front,
  flies until it leaves the camera rect, single hit. Tapping B again inside the 32-frame window
  fires another shot every 16 frames for 2 EP each (0x08014E1E).
* **Masenko Ball (0x2D)**: lob. charge C starts at −180 (0x08014F4A) and gains +30 per frame B is
  held (0x08014F80); on release (0x08013B12) C = clamp(C, 255, 1024); vx/vy = dir[facing]·C
  (dir table 0x086DE8D0, 24.8) with vy −= 0x200 and +0x20/frame "gravity"; a height value starts at
  0x800 with +0x200 upward speed, −0x20/frame; when it goes below 0 (≈36 f) it explodes: attack
  id 7 over the ball rect with scale `min(2·C+256, 2048)` (0x08013B88). Measured: tap → scale 766,
  range 34 px; hold 60 f → scale 2048, range 136 px.
* **Kamehameha (0x30)** / **Special Beam Cannon (0x31)**: 24-frame charge; at frame 24 the beam fires
  only if B is still held (else cancelled). While held the beam head grows 3 px/f (0x0801475E)
  until it touches a target/wall/screen edge; every frame a 16×16 box at the head is hit with
  attack id 10 (0x40) / 11 (0x66). EP drains through a fractional accumulator: Kamehameha +0x56/256 per
  frame (≈1 EP/3 f), SBC +0x22/256 (≈1 EP/7.5 f). Ends on release or EP 0. Measured ~10 and ~12
  damage per frame-hit at POW 65/55 vs END 3. Kamehameha only damages through its 16×16 head and stops at the
  first enemy; SBC is one 12-px-thick rect from the muzzle to the tip that keeps growing and hits **every**
  enemy inside it every frame (pierce VERIFIED with two enemies, `sbc_r_2`).
* **Scatter Shot (0x2C)**: at counter 16 spawns 3 pellets (class vt 0x08024D14, ctor 0x080137C4)
  from table 0x086E362C (per facing 3×24 B: dx, dy, vx, vy (24.8), param, anim): centre 3.0 px/f
  straight, sides (2.6, ±1.5) px/f. Each pellet attack id 6 (0x500). State lasts 48 f.
* **Big Bang (0x2E)**: ball (vt 0x080241B0, ctor 0x08013E48) held in front while B is held; ball
  scale starts at 32 and +1 per frame (stops when EP hits 0; EP −1 every 4 f). On release
  (0x08014160) it jumps 16 px then flies 2 px/f (table 0x086DE904); each frame a 24×24 probe (pos±12);
  on contact/wall it explodes: attack id 8 (power 0xC000) to everything in a 64×64 box (pos±32)
  with scale = 32+held frames. Measured scale 33 (tap, dmg 246) and 122 (90 f hold, dmg 931).
* **Burning Attack (0x2F)**: bolt (vt 0x0802507C, ctor 0x080141B4) hovers while held; release
  (0x08014350) → 2.5 px/f (+10 px jump); the first target touched takes attack id 9 (power 0x40,
  tiny damage) — the "stun" comes from the receiver's reaction to that attack (enemy side).
* **Sword Blast (0x32)**: immediate sword-swing melee hit (attack id 12, 0x400) on press, then an
  energy wave (vt 0x08024B24) 8 frames later, ≈3.4 px/f, attack id 13 (0xC00). 33-frame state.
* **Spirit Bomb (0x33)**: bomb (vt 0x08024AF8, ctor 0x080165EC) 24 px above the head; size/scale
  starts at 5, +1 per frame while held (EP −1 / 4 f, growth stops at EP 0); release → 8 frames
  later thrown (0x080169B8) at 1 px/f (table 0x086F78F0), msg 0x13 broadcast; on impact attack
  id 14 (power 0xA000) with scale = size, plus attack id 15 (power 0) "stun" broadcast. Measured
  hold 40 → scale 53, hold 200 → scale 213 (dmg 362 / 1483).
* **Hercule special (type 6)**: EP −6, state 5 (32 f), sends attack id 15 (power 0) to every
  entity in the world list and spawns a 32-frame effect (vt 0x08025808).

#### Effective damage
All moves go through the shared formula (0x0800F910, `formula.py`, verified 100+ hits):
```
crit  = melee only: chance((0xA4*STR) & 0xFFFF)  → power *= 2
b2    = power*(128-DEF)>>7
t     = ((rand(26)+256)*b2 >> 8) * f(S) >> 18,   f(S) = ((26S-701)S+115618)S >> 8
t     = scale*t >> 10
dmg   = t*mult >> 7          (S = STR for melee / POW for energy; mult = defender +0x163/+0x164)
```
So scale acts as a multiplier on power: Big Bang effective power = 0xC000·(32+t)/1024 = 48·(32+t).

---------------------------------------------------------------------------------------------------
### 5. Forms / transformations — table 0x081D967C (22 × 20 B, index = record+8) — VERIFIED

```
+0x00 u8  flags: bit0 = transformed (using the transform slot reverts), bit1 = transform slot unusable
+0x01 u8  actor id (sprite set/hurtbox descriptor, table 0x083B4E74)
+0x02 u8, +0x03 u8   other graphics ids (portrait/HUD; not decoded)
+0x04 u8  form after transform (used when state 0x23/0x25 completes: 0x08014D58(ent,1))
+0x05 u8  form after revert   (state 0x24: 0x08014D58(ent,0))
+0x06 u8  special-melee attack length in 4-frame units
+0x07 u8  special-melee charge animation length in 4-frame units
+0x08 u8  special-melee hit mask (bit k = hit on attack anim frame k)
+0x0C u32 special-melee lunge speed (24.8 px/f; 0x100 = 1 px/f)
+0x10 ptr form controller singleton (IWRAM 0x030021E0..F0, initialised at 0x080232E4)
```
Form controllers (relative vtables; slot0 applies stats+speeds, slot1 = per-frame tick):

| ctl | vtable | stats | walk / run px/f | per-frame tick |
|---|---|---|---|---|
| normal 0x030021E0 | 0x08024004 | record>>8 | 1.25 / 2.5 | regen (below) |
| 0x030021E4 (Gohan form 4 SSJ2) | 0x08024E4C | STR=POW=END=100 | 2.0 / 3.25 | HP +1% max every 20 f |
| 0x030021E8 (forms 10, 14 bulky SSJ) | 0x0802526C | +15/+10/+15 | 0.75 / 1.75 | EP −1 every 40 f, revert at 0 |
| 0x030021EC (forms 5,11,16,17,20 SSJ) | 0x08024C58 | +10 each | 1.75 / 3.0 | EP −1 every 20 f, revert at 0 |
| 0x030021F0 (Piccolo form 7) | 0x08024ABC | +6 each | 1.75 / 3.0 | HP +1% & EP −1 every 20 f |

Measured (pl_transform.py): Goku 19→20 stats 62/65/47 → 72/75/57, walk 320→448, EP −5 per 100 f;
Piccolo 6→7 +6, HP +50 per 100 f at max 1055; Vegeta 8→11 (+10). Speeds are floats converted with
0x080265C4 (×256). Damage multipliers are reset to 0x80 by every controller apply.

Form labels (screenshots `work/log2/pl_forms.png`): 0-5 Gohan (dark gi / white armor / blue armor /
starting outfit / SSJ2 / SSJ), 6-7 Piccolo (caped / cape off), 8-11 Vegeta (normal, alt armor,
bulky Super Vegeta, SSJ), 12-18 Trunks (jacket, long-hair armor, bulky SSJ, long-hair alt, SSJ armor,
SSJ jacket, prologue Future Trunks), 19-20 Goku / SSJ Goku, 21 Hercule. Transform chains
(form→transform→revert): 2→5, 5↔2 (5's transform target 4 = scripted SSJ2), 8→11, 9→10,
12→17, 13→14, 15→16, 19→20, 6→7.

#### Regeneration (normal controller 0x08017028) — VERIFIED (+29 EP at max 999)
`acc += 110` per frame; on 16-bit overflow (every ~595.8 frames ≈ 9.9 s): active character EP +=
maxEP·0x7AE>>16 (3.0%); every other party member HP and EP += max·0xCCD>>16 (5.0%) (DISASM).
No HP regen for the active character in normal form.

---------------------------------------------------------------------------------------------------
### 6. Hitboxes / hurtboxes
Hurtbox = integer position + actor-descriptor rect `desc+0x0C+facing*16` (x0,y0,x1,y1) (0x080079B8).
Typical player: (−2,−26)–(18,10) (20×36 px); Piccolo left/right are wider (−3..25 / −9..19).
Per form in characters.json. Attack rects are tested with strict overlap
(x0 < hx1 && hx0 < x1 && y0 < hy1 && hy0 < y1). Projectiles keep their own rect at +0x28..+0x34
built from a per-frame box (s8 dx, s8 dy, u8 w, u8 h) at projectile+0x18 (0x08013BF4).

---------------------------------------------------------------------------------------------------
### 7. move_format — how to author a new move

1. **Attack table entry** (0x086DB2CC, 8 bytes each, 16 used):
   `u8 type` (0 = melee: uses attacker STR, can crit, defender mult +0x163; 1 = energy: POW, mult
   +0x164), `u8 id` (tag; some receivers react to specific ids — e.g. 9 Burning Attack stun, 15
   zero-damage stun), `u16 pad`, `u32 power` (1024 ≈ a basic punch/blast).
2. **Hit message** (built on the stack, sent with 0x08006DA6(list, msgType=0, msg) to every entity
   whose hurtbox overlaps): `+0 u8 team (1 = player side)`, `+4..+0x10 rect x0,y0,x1,y1 (pixels)`,
   `+0x14 u8 facing`, `+0x18 u32 scale (1024 = 1.0)`, `+0x1C ptr attack entry`, `+0x20 ptr attacker
   entity` (STR/POW read from attacker+0x160/+0x161), `+0x30 ptr source object`. msgType 2 is a
   "is anything here?" probe (used by Big Bang / Burning Attack before detonating).
3. **Move type** (0x081D9900): icon gfx ×2, `canUse` (check EP ≥ min), `execute` (EP −= cost,
   `player.setState(state)`), plus a slot in the character's record (+0x18) and an unlock bit.
4. **State** (IWRAM singleton in table 0x083B5278): enter / per-frame / exit methods; typical
   pattern = count frames in entity+0x12, spawn a projectile at pos + per-facing offset table,
   return to state 0 when done.
5. **Projectile object** (base ctor 0x08013BC0, 0x50-0x5C bytes): +0x18 box ptr (s8,s8,u8,u8),
   +0x1C owner, +0x20/+0x24 pos 24.8, +0x28..+0x34 rect, +0x40.. class-specific (velocity 24.8,
   timers, scale). Speed examples: 0x300 = 3 px/f. Removed when outside the camera rect (0x08013C8E).

A "new move" = attack entry (power/type) + a projectile with velocity/box + a state that spawns
it + a move-type row with EP cost.

---------------------------------------------------------------------------------------------------
### 8. Characters (base data; full numbers in characters.json)

Initial (Lv1, record 0x081D9548): HP/EP/STR/POW/END —
Gohan 85/20/3/5/3 (exp 40), Piccolo 95/20/4/4/5, Vegeta 105/20/6/4/6, Trunks 110/20/6/3/4,
Goku 100/20/5/7/6, Hercule 50/20/8/1/6.
Growth per level (0x081D95F0, min..max in stat points): Gohan STR 1.24-2.02, POW 1.18-1.96, END
1.00-1.78; Piccolo 0.88-1.66 / 0.96-1.74 / 0.81-1.59; Vegeta 0.91-1.70 / 0.86-1.64 / 0.96-1.74;
Trunks 1.12-1.90 / 0.67-1.45 / 0.71-1.49; Goku 1.00-1.78 / 1.00-1.78 / 0.77-1.55; Hercule 0.25-0.75.
Level-up: 0x08003CA0 (stat += min·n + rand((max−min+1)·n), cap 100; maxHP ×(1.055..1.075) per level;
maxEP +2..4 per level; full heal). Max level 50; exp table 0x081D9834.

---------------------------------------------------------------------------------------------------
### 9. Per-attack simulator data (VERIFIED unless noted) — `moves.json → player_attacks`
Measured with `pl_measure.py` / `pl_measure_all.py` (45 runs; raw → `re/log2/pl_measurements.json` via
`pl_measure_compact.py`): Lv40 characters on map 12 against class-53 snakes pinned at chosen offsets; every
damage call is captured by single-stepping into 0x0800F910 (hit-message rect = msg base+4, base = r1−0x14).
Coordinates relative to the player's integer position, facing right unless noted; f0 = press frame.

| attack | startup → first hit/spawn | hitbox / projectile | speed | hits | pierce | EP | vulnerable |
|---|---|---|---|---|---|---|---|
| Punch | 0 (press frame); 16 f anim | rect per facing (R: 16,−24,24,8; D: 0,0,16,16; U: 0,−24,16,0; L: −8,−24,0,8) | — | 1 | all in rect | — | yes |
| Special melee | release ≥32 f after press; hits at release+~11/+15 (Goku), 4 hits every 4 f (Piccolo) | rect table 0x083B5208 (same geometry as punch) moving with a 1 px/f lunge | lunge 1 px/f | 2 (4 Piccolo) | all in rect | — | charging yes, release **no** (+0x168=0) |
| Energy blast | shot at +8 f, refire every 16 f | 16×16, spawns (18,−8) R / (8,0) D | 3.0 | 1 | no (stops, 15-f impact) | 2 per shot | yes |
| Masenko | flies on release, explodes 36 f later | 8×8 ball, 16×16 blast | C/256 px/f horizontal, arc | 1 per enemy in blast | blast hits all overlapping | 4 | yes |
| Kamehameha | 24-f charge, first hit +26 f | 16×16 head, grows 3 px/f until contact | — | every frame | no | 2 + 86/256 per f | yes |
| SBC | 24-f charge, first hit +25 f | 12-px-thick rect muzzle→tip (starts (26,−14)), tip +3 px/f | — | every frame | **yes** | 2 + 34/256 per f | yes |
| Scatter Shot | pellets at +16 f, state 48 f | 3×16×16 from table 0x086E362C | 3.0 centre, (2.6,±1.5) sides | up to 3 | no | 12 | yes |
| Big Bang | on release | ball 8→16→32 px while charging; probe 24×24; blast 64×64 (ball ±32), 16 f | 2.0 (+16 px jump) | 1 per enemy in blast | blast | 10 + 1/4 f held | yes |
| Energy Punch | 0 | punch rects | — | 1 | — | 4 | yes |
| Burning Attack | on release | 8×8 held → 32×32 flying | 2.5 (+10 px jump) | 1 | no | 6 | yes |
| Sword Blast | swing 0 (16×24 rect (16,−16,32,8)), wave +8 f | wave 8×16 (16×8 vertical) | 3.5 | 1+1 | no | 5 | yes |
| Spirit Bomb | throw 8 f after release | bomb 8→16→32 px; blast = bomb box ×3 (24×24 small, 48×48 at size≥64) | 1.0 | 1 per enemy in blast | blast | 10 + 1/4 f held | yes |
| Hercule special | 0 | whole map (0,0,2048,2048), power 0 | — | all enemies | — | 6 | yes |

* **Stuns sent**: attack id 15 (power 0) — Hercule special always, Spirit Bomb only when size ≥ 128
  (0x08016726) — puts enemies into stun state @0x03002230 for **324 frames** (snakes; VERIFIED
  `spirit_stun_long`). Attack id 9 (Burning Attack) produced only the normal hurt reaction on the snake
  (a special stun, if any, is enemy-class specific — Part C). msg 0x13 is broadcast at the Spirit Bomb throw.
* **Knockback given** (measured on a free snake, enemy-side reaction): every damaging hit puts the enemy in
  the generic hurt state (@0x03002128) for ~31 f and pushes it away from the player: punch 16.5 px,
  blast 7.6 px, Burning Attack 14.5 px; Kamehameha keeps it pinned/pushed (20 px over 95 f).
* **Recovery**: projectile moves keep the player locked in the move state until it ends (blast 24 f,
  Scatter 48 f, Masenko/Big Bang/Burning 32 f after release, Sword Blast 33 f); beams end the frame B is
  released.

### Open gaps
* Exact AI reaction to attack id 9 and msg 0x13 per enemy class (enemy side, Part C).
* Beam maximum length is "until screen edge/wall" (no constant cap found).
* Form aux graphics ids (+0x02/+0x03) and the meaning of record flag bit1 not decoded.
* Map/event that owns each learn script is identified only by nearby dialogue.
* Idle state 0x0E (after 1024 frames) contents not examined.


# Part C — Enemies, AI, bosses, maps

## LoG2 — Enemies, AI and bosses

Scope: enemy class table, enemy entity layout, the AI engine (state machine of behaviour-tree nodes), every node
type, per-enemy/boss behaviour, hit reactions / stun / knockback / i-frames, death, exp, drops, and where every
enemy spawns. Generated data: `data/log2/enemies.json`, `data/log2/ai.json`, `data/log2/maps.json`
(scripts: `re/log2/extract_enemies.py`, `extract_ai.py`, `extract_maps.py`; helpers `en_lib.py`, `en_script.py`,
`en_tree.py` (pretty printer: `.venv/bin/python re/log2/en_tree.py 74 75 76`)).

Evidence tags: **VERIFIED** = disassembly + observed in the emulator; **VERIFIED(disasm)** = read in code, not
separately measured; **INFERRED** = plausible reading, not proven.

Units: positions are 24.8 fixed point (px<<8); distances in px measured centre-to-centre on collision boxes;
speeds in 1/256 px per frame; probabilities per frame out of 65536 (`rand16 < p`, `0x0802142e`);
1 frame = 1/59.73 s. Facing byte (entity+0x10): 0=down, 1=up, 2=left, 3=right (VERIFIED by the blind-spot test).

---------------------------------------------------------------------------------------------------------------

### 1. Data tables

| Table | ROM | Size | Notes |
|---|---|---|---|
| Enemy class table | `0x086d56ac` | 109 × 24 B | stats/exp/AI per enemy kind (§2) |
| AI state table | `0x086d5194` | 152 × 8 B | `[behaviour root node*, on-hit root node*]` (§4) |
| AI nodes | `0x086d1bxx–0x086d5650`, `0x086ff3c4–0x086ffc30`, `0x08701e1c–0x087047d4` | — | node structs (§5) |
| Attack table | `0x086db2cc` | 16 × 8 B | `[u8 type(0 melee/1 energy), u8 id, u16 pad, u32 power]`; enemies use ids 1, 2, 3 (all power 1024) |
| Scouter database | `0x086f8e40` | 128 × 20 B | `[name*, desc*, gfx id, portrait id, class row]` (VERIFIED: scouter page 0x0801790e prints class row HP/STR/POW/END) |
| Actor graphics ptrs | `0x083b4e74` | 4 B × id (7..156) | gfx id → actor descriptor (descriptor+3/+4 collision box w,h; +0xc+16*facing hurt rect) |
| Map table | `0x08409368` | 327 × 0x38 B | spawn lists (§10) |
| Item table | `0x086ade24` | 56 B/item | used to name drops (item 11..16 = Health/Energy bonus orbs) |
| Event script VM | interpreter `0x080092e4`, opcodes `0x083b5c00`, builtins `0x083b5d28` | — | on-death (drop) scripts (§8) |

### 2. Class row (24 bytes) — VERIFIED

```
+00 u32 exp          base exp (clamped at award time, §8)
+04 u32 hp           max HP -> entity+0x188
+08 u32 ai_state     initial AI state id (pushed by 0x0800e280)
+0c u8  STR          attacker stat for melee hits
+0d u8  POW          attacker stat for energy hits
+0e u8  END          defence: incoming *= (128-END)/128
+0f u8  melee_mult   incoming melee *= m/128   (38, 64, 128 seen)
+10 u8  energy_mult  incoming energy *= m/128  (64, 96, 128 seen)
+11 u8[3] pad
+14 u32 death node   fn2 called on death (0x086d2b6c explode, 0x086ff3c4 quiet, 0x086d5654/5664/5694 variants)
```
Copy into entity: `0x0800f7cc` stores bytes +0xc..+0x10 at entity+0x160..+0x164 (`ldrb/strb` sequence).
Damage formula (shared with players; VERIFIED 14/14 hits by `verify_dmg.py`, see main notes): enemy hits use
attack id 2 (melee, S = enemy STR) or ids 1/3 (energy, S = enemy POW), against the player's END/multipliers.
VERIFIED example: snake class 53 (STR 7) vs Piccolo (END 53) → 7 damage per hit, matching the formula.

Full table (generated, `re/log2/en_mdtable.py`): aggro = radius of the first range check of the class's initial
state ("seen" = must not be behind the enemy); chase = ChasePlayer speeds reachable from that state.

| id | name(s) | HP | exp | STR | POW | END | melee/energy taken | ai | aggro px (facing?) | chase px/f | attacks | spawns | boss/scripted |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | (player placeholder row) | 100 | 100 | 1 | 1 | 1 | 128/128 | 0 | - | - | - | 0 |  |
| 1 | Alligator | 600 | 5400 | 29 | 1 | 20 | 128/128 | 149 | 72.0 (seen) | 0.6992 | melee strike | 38 |  |
| 2 | Android 16 | 4480 | 0 | 42 | 50 | 39 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 3 | Android 17 | 3550 | 0 | 32 | 39 | 34 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 4 | yes |
| 4 | Android 18 | 2910 | 0 | 29 | 34 | 27 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 5 | Android 19 | 2970 | 0 | 21 | 14 | 11 | 128/128 | 29 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 6 | Android 20 | 2810 | 0 | 24 | 19 | 14 | 128/128 | 29 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 7 | Android 20 | 1900 | 0 | 25 | 21 | 17 | 128/128 | 29 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 3 | yes |
| 8 | Tiger Bandit | 38 | 14 | 6 | 3 | 4 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 23 |  |
| 9 | Tiger Bandit | 325 | 650 | 10 | 12 | 8 | 128/128 | 45 | 72.0 (seen) | 0.3984,0.5,0.7969 | lobbed arcing projectile; melee strike | 49 |  |
| 10 | Tiger Bandit | 1120 | 25250 | 34 | 42 | 25 | 128/128 | 45 | 72.0 (seen) | 0.3984,0.5,0.7969 | lobbed arcing projectile; melee strike | 8 |  |
| 11 | T-Rex | 6700 | 42350 | 55 | 1 | 60 | 128/128 | 59 | 72.0 | 0.6992,0.7969 | melee strike; pounce | 1 |  |
| 12 | Warlord's Henchman | 900 | 3200 | 10 | 30 | 26 | 128/128 | 11 | 72.0 (seen) | - | energy ball projectile | 13 |  |
| 13 | (unnamed) | 400 | 1200 | 1 | 14 | 9 | 128/96 | 0 | - | - | - | 0 |  |
| 14 | Snake | 530 | 3290 | 32 | 1 | 28 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 3 |  |
| 15 | (unnamed) | 525 | 2830 | 35 | 1 | 15 | 38/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 16 | T-Rex | 5120 | 36900 | 49 | 1 | 55 | 128/128 | 59 | 72.0 | 0.6992,0.7969 | melee strike; pounce | 4 |  |
| 17 | Wolf | 65 | 45 | 8 | 1 | 4 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 21 |  |
| 18 | Bulma | 22 | 0 | 2 | 1 | 2 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 19 | Butterfly, Squirrel | 1 | 0 | 1 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 20 | Cat | 6 | 0 | 1 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 21 | Cell, "Perfect" | 6500 | 0 | 50 | 55 | 60 | 128/128 | 138 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 22 | Cell, "Perfect" | 6500 | 0 | 50 | 55 | 60 | 128/128 | 138 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 23 | Cell, "Perfect" | 6500 | 0 | 50 | 55 | 60 | 128/128 | 138 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 24 | Cell, "Perfect" | 6500 | 0 | 50 | 55 | 60 | 128/128 | 138 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 25 | Cell, "Perfect" | 8230 | 0 | 60 | 59 | 65 | 128/128 | 138 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 26 | Cell Junior | 4990 | 0 | 46 | 50 | 46 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 3 | yes |
| 27 | Chiaotzu | 142 | 0 | 9 | 9 | 7 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 28 | Chi-Chi | 29 | 0 | 5 | 1 | 4 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 29 | Cooler | 10200 | 0 | 75 | 85 | 79 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 30 | Dende | 172 | 0 | 22 | 32 | 11 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 31 | Destroyer | 1463 | 4170 | 32 | 39 | 29 | 128/128 | 91 | 88.0 | 0.3984,0.5,0.7969 | boss special; melee strike | 4 |  |
| 32 | Destroyer | 4200 | 58900 | 49 | 44 | 43 | 128/128 | 91 | 88.0 | 0.3984,0.5,0.7969 | boss special; melee strike | 4 |  |
| 33 | Destroyer | 3120 | 43180 | 46 | 42 | 39 | 128/128 | 91 | 88.0 | 0.3984,0.5,0.7969 | boss special; melee strike | 2 |  |
| 34 | Dog | 9 | 0 | 2 | 1 | 2 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 35 | Dr. Briefs | 36 | 0 | 3 | 1 | 2 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 36 | (unnamed) | 1 | 1 | 1 | 1 | 1 | 128/128 | 30 | - | - | - | 1 |  |
| 37 | Eggbot | 250 | 875 | 19 | 25 | 18 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 47 |  |
| 38 | Eggbot | 525 | 2670 | 35 | 41 | 22 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 9 |  |
| 39 | Eggbot | 1349 | 46200 | 47 | 53 | 52 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 6 |  |
| 40 | Eggbot | 1130 | 33170 | 38 | 44 | 34 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 8 |  |
| 41 | (unnamed) | 130 | 20 | 3 | 2 | 3 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 42 | Frieza | 1974 | 0 | 30 | 37 | 20 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 43 | (unnamed) | 1321 | 0 | 19 | 28 | 21 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 44 | General Tao | 6400 | 0 | 50 | 55 | 60 | 128/128 | 129 | 88.0 | 0.5,1.0,1.1992 | boss special projectile; ki blast projectile; melee strike | 1 | yes |
| 45 | Ninja | 1600 | 53000 | 47 | 64 | 52 | 128/128 | 52 | 60.0 | 0.8984,1.1992 | melee strike; thrown projectile | 0 |  |
| 46 | (unnamed) | 3400 | 21800 | 1 | 60 | 49 | 128/96 | 0 | - | - | - | 0 |  |
| 47 | Scorpion | 2000 | 50000 | 70 | 1 | 56 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 3 |  |
| 48 | Snake | 3530 | 50000 | 49 | 1 | 41 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 49 | (unnamed) | 4290 | 58900 | 60 | 1 | 64 | 128/128 | 59 | 72.0 | 0.6992,0.7969 | melee strike; pounce | 0 |  |
| 50 | Wolf | 3980 | 60000 | 54 | 1 | 30 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 0 |  |
| 51 | Warlord's Henchman | 1100 | 16200 | 15 | 42 | 36 | 128/128 | 11 | 72.0 (seen) | - | energy ball projectile | 8 |  |
| 52 | (unnamed) | 650 | 7000 | 1 | 25 | 25 | 128/96 | 0 | - | - | - | 0 |  |
| 53 | Snake | 35 | 13 | 7 | 1 | 3 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 17 |  |
| 54 | Triceratops | 870 | 35000 | 40 | 1 | 40 | 128/64 | 66 | 72.0 (seen) | 0.6992,0.7969 | melee strike; pounce | 13 |  |
| 55 | Saber-Toothed Tiger | 1500 | 20000 | 25 | 1 | 21 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 2 |  |
| 56 | Warthog | 900 | 25000 | 55 | 1 | 30 | 128/128 | 66 | 72.0 (seen) | 0.6992,0.7969 | melee strike; pounce | 0 |  |
| 57 | Wolf | 675 | 1680 | 29 | 1 | 24 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 6 |  |
| 58 | Hawk | 110 | 300 | 10 | 1 | 5 | 128/128 | 32 | - | 1.0 | charge/dash | 6 |  |
| 59 | Human, Maron, Mayor, Mrs. Briefs, Turtle | 32 | 0 | 2 | 1 | 2 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 60 | Cell | 5210 | 0 | 32 | 32 | 37 | 128/128 | 80 | 88.0 | 0.5,1.0,1.1992 | boss special melee; ki blast projectile; melee strike | 1 | yes |
| 61 | Kami | 230 | 0 | 23 | 30 | 13 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 62 | King Cold | 1180 | 0 | 24 | 28 | 10 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 63 | Korin | 3470 | 0 | 25 | 59 | 43 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 64 | Krillin | 1800 | 1200 | 23 | 24 | 17 | 128/128 | 98 | 88.0 | 0.5,1.0 | ki blast projectile; melee strike | 1 | yes |
| 65 | Ladybug | 29 | 16 | 1 | 7 | 4 | 128/128 | 106 | 60.0 (seen) | 0.6992 | projectile volley | 24 |  |
| 66 | Ladybug | 175 | 520 | 1 | 16 | 11 | 128/128 | 106 | 60.0 (seen) | 0.6992 | projectile volley | 21 |  |
| 67 | Ladybug | 700 | 11200 | 1 | 32 | 29 | 128/128 | 106 | 60.0 (seen) | 0.6992 | projectile volley | 14 |  |
| 68 | Master Roshi | 87 | 0 | 9 | 15 | 4 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 69 | Kuma Mercenary | 125 | 250 | 17 | 1 | 15 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 17 |  |
| 70 | Kuma Mercenary | 940 | 9800 | 35 | 1 | 32 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 16 |  |
| 71 | Kuma Mercenary | 596 | 3610 | 25 | 1 | 20 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 16 |  |
| 72 | Mr. Popo | 830 | 0 | 14 | 40 | 25 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 73 | Namek | 390 | 0 | 12 | 20 | 26 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 74 | Wolf | 400 | 500 | 11 | 1 | 5 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 1 | yes |
| 75 | Ninja | 900 | 5200 | 34 | 25 | 27 | 128/128 | 52 | 60.0 | 0.8984,1.1992 | melee strike; thrown projectile | 16 |  |
| 76 | Cell, "Nonperfect" | 4890 | 0 | 39 | 41 | 46 | 128/128 | 116 | 88.0 | 0.6992,1.1992 | boss beam/projectile; boss special melee; ki blast projectile; melee strike | 1 | yes |
| 77 | Oolong | 56 | 0 | 3 | 2 | 5 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 78 | Cell, "Perfect" | 5160 | 0 | 44 | 46 | 50 | 128/128 | 148 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 79 | Cell, "Perfect" | 5500 | 0 | 44 | 46 | 50 | 128/128 | 148 | 88.0 | 0.6992,1.1992 | boss beam/projectile; ki blast projectile; melee strike | 1 | yes |
| 80 | Pterodactyl | 900 | 9578 | 28 | 1 | 16 | 128/128 | 38 | - | 1.0 | charge/dash | 4 |  |
| 81 | Puar | 24 | 0 | 2 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 82 | (unnamed) | 900 | 14700 | 1 | 43 | 36 | 128/96 | 0 | - | - | - | 0 |  |
| 83 | Snake | 275 | 600 | 15 | 1 | 14 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 17 |  |
| 84 | Hawk | 1000 | 21000 | 35 | 1 | 30 | 128/128 | 32 | - | 1.0 | charge/dash | 0 |  |
| 85 | Warlord's Henchman | 90 | 325 | 2 | 8 | 6 | 128/96 | 11 | 72.0 (seen) | - | energy ball projectile | 41 |  |
| 86 | Ninja | 1400 | 36800 | 37 | 31 | 31 | 128/128 | 52 | 60.0 | 0.8984,1.1992 | melee strike; thrown projectile | 38 |  |
| 87 | (unnamed) | 110 | 250 | 1 | 6 | 6 | 128/96 | 0 | - | - | - | 0 |  |
| 88 | Pterodactyl | 1200 | 25000 | 40 | 1 | 30 | 128/128 | 38 | - | 1.0 | charge/dash | 3 |  |
| 89 | Saber-Toothed Tiger | 1750 | 30000 | 30 | 1 | 24 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 0 |  |
| 90 | Scorpion | 400 | 750 | 18 | 1 | 11 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 6 |  |
| 91 | Snake | 125 | 350 | 10 | 1 | 5 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 6 |  |
| 92 | Saber-Toothed Tiger | 750 | 6160 | 32 | 1 | 14 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 25 |  |
| 93 | (unnamed) | 70 | 225 | 9 | 1 | 15 | 38/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 94 | Triceratops | 275 | 225 | 20 | 1 | 20 | 64/64 | 15 | - | - | touch damage | 34 |  |
| 95 | (unnamed) | 5 | 0 | 1 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 96 | T-Rex | 5 | 0 | 1 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 97 | Triceratops | 5 | 0 | 1 | 1 | 1 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 98 | Tien | 2110 | 2500 | 35 | 29 | 27 | 128/128 | 98 | 88.0 | 0.5,1.0 | ki blast projectile; melee strike | 1 | yes |
| 99 | T-Rex | 1750 | 3750 | 40 | 1 | 30 | 128/128 | 59 | 72.0 | 0.6992,0.7969 | melee strike; pounce | 2 |  |
| 100 | Triceratops | 80 | 175 | 15 | 1 | 14 | 128/64 | 66 | 72.0 (seen) | 0.6992,0.7969 | melee strike; pounce | 0 |  |
| 101 | Triceratops King | 850 | 1500 | 21 | 1 | 25 | 64/64 | 59 | 72.0 | 0.6992,0.7969 | melee strike; pounce | 1 |  |
| 102 | Warthog | 300 | 800 | 20 | 1 | 14 | 128/128 | 66 | 72.0 (seen) | 0.6992,0.7969 | melee strike; pounce | 27 |  |
| 103 | Scorpion | 110 | 375 | 11 | 1 | 5 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 10 |  |
| 104 | Wolf | 1000 | 1300 | 65 | 1 | 35 | 128/128 | 74 | 80.0 (seen) | 1.0,1.1992 | melee strike | 3 |  |
| 105 | Yajirobe | 110 | 0 | 12 | 1 | 15 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |
| 106 | Yamcha | 800 | 600 | 18 | 12 | 14 | 128/128 | 21 | 88.0 | 0.5,1.0,1.1992 | ki blast projectile; melee strike | 1 | yes |
| 107 | Hawk | 700 | 3000 | 25 | 1 | 30 | 128/128 | 32 | - | 1.0 | charge/dash | 0 |  |
| 108 | Scorpion | 30 | 10 | 6 | 1 | 2 | 128/128 | 8 | 72.0 (seen) | 0.6992 | melee strike | 0 |  |

Notes on the table:
* Rows with exp 0 and/or many names (Bulma, Humans, Kami, ...) are scouter/NPC stat rows: those characters are
  spawned by the NPC factories `0x0800b711` / `0x0800d6df` (no AI); the class row is only shown on the scouter.
* Rows with 0 spawns and unnamed (13, 15, 43, 46, 49, 52, 82, 87, 93, 95–97) are not placed by any map list
  (unused or created by code paths not found; INFERRED unused).
* Five rows for Perfect Cell (21–25) + rows 78/79 are different fight variants (map 304/324/325, §9).
* Class 94 Triceratops (34 spawns, Triceratops Jungle) uses ai 15 = Patrol + ContactDamage and its on-hit tree
  only flashes (no ApplyDamage): it cannot be damaged. VERIFIED: 16 point-blank melee/ki attacks did 0 damage
  while its touch damage hit the player (`en_verify_trike.py`).

### 3. Enemy entity (0x1f0 bytes, vtable 0x080252a4; dying: 0x08025fec)

| off | type | meaning | evidence |
|---|---|---|---|
| +0x000 | ptr | vtable (relative-offset vtables: target = vtable_base + entry) | VERIFIED |
| +0x010 | u8 | facing 0=down 1=up 2=left 3=right | VERIFIED |
| +0x018 | ptr | overlay/anim controller object | VERIFIED(disasm) |
| +0x048 | ptr | actor descriptor (from gfx id via 0x08009324) | VERIFIED |
| +0x154/155 | u8,u8 | collision box w,h (from descriptor+3/+4, 0x080078e4); e.g. wolf/snake 16×8 | VERIFIED |
| +0x158/15c | s32 | x, y (px<<8; top-left of collision box) | VERIFIED |
| +0x160..164 | u8×5 | STR, POW, END, melee_mult, energy_mult | VERIFIED |
| +0x168 | u32 | hittable flag (1 = can be hit; cleared during KnockbackInvulnerable) | VERIFIED(disasm) |
| +0x180 | ptr | class row | VERIFIED |
| +0x184 | ptr | on-death event node (spawn def +0x14) | VERIFIED(disasm) |
| +0x188 | s32 | HP | VERIFIED |
| +0x18c | u8 | direction of last hit (hit packet byte 0) | VERIFIED(disasm) |
| +0x18d | u8 | death-started flag | VERIFIED(disasm) |
| +0x190 | 12 B | patrol controller {cur waypoint obj, s8 index, list*} (0x0800d138/0x0800d156) | VERIFIED |
| +0x19c | ~0x58 B | AI controller (§4) | VERIFIED |

Hurt rect: descriptor+0x0c+16*facing = `[x0,y0,x1,y1]` s32 offsets added to the integer position
(0x080079b8). Wolf: down/up `[-2,-26,18,10]`, left/right `[-10,-23,26,10]` (read from emulator, matches ROM).
Per-gfx collision boxes and hurt rects are exported in `enemies.json` → `graphics`.

Enemy update (vtable slot 2, `0x0800e5a4`): base movement/animation (`0x0800772a`) then AI tick (`0x0801ab08`).
Enemy onMessage (slot 9, `0x0800e5b8`): message 0 = attack query from the player side: if hit packet team byte
is 1 (player) or 2 (everyone), entity+0x168 != 0 and the packet rect overlaps the enemy hurt rect →
damage calc `0x0800f910` → AI on-hit dispatch `0x0801ab2a`. Message 0x14 = scouter scan. (team byte 0 =
hostile-to-player attacks: enemy MeleeStrike/ContactDamage and enemy projectiles use team 0 (projectiles copy
owner+0x16c, which is 0 for enemies); the Eggbot SelfDestructBurst and the lobbed rock explicitly use team 2 and so
also hurt other enemies — VERIFIED(disasm).)
takeHit (slot 14, `0x0800e485`): `HP -= hit.damage`; if HP <= 0 → HP = 0, call class death node fn2.

### 4. AI engine — VERIFIED(disasm), behaviour VERIFIED in emulator for wolf/snake

* Controller at entity+0x19c: `+0 owner`, `+4 depth` (max 5), `+8..` stack of `(runtime node object*, state
  entry*)`, `+0x30` count + list of runtime objects to destroy at the next tick.
* `push(id)` 0x0801aa98, `replace_top(id)` 0x0801aa46 (old top's exit is called, object queued for destroy),
  `pop` 0x0801aad4, `tick` 0x0801ab08 = flush destroy list, then `top.update(owner)`.
* On hit: `0x0801ab2a` = flush, then `states[top].on_hit_root.fn1(node, entity, hit)`.
* Every node is a ROM struct: `word0 = create(node, entity)` → runtime object with vtable
  `[enter, update→done?, exit, destroy]`; `word1 = onHit(node, entity, hit)` → handled?; `word2` helper;
  then parameters. Static parameter-less nodes return a shared runtime object in IWRAM (0x0300220c..0x03002290).
* The class row's ai_state is pushed at spawn. Most initial states are "idle until the player is noticed" and
  then `GotoState` into the active machine. There is no "lose interest" transition in most machines (once aggroed
  an enemy stays aggressive).

Observed (`en_verify_aggro.py`, map 13): player placed on each axis and stepped 1 px/frame closer:
wolf (class 17, ai 74) leaves state 74 → 75 at exactly 80.0 px, snake (class 53, ai 8) 8 → 9 at 72.0 px; from
behind (player on the side the enemy faces away from) no transition. Chase speed measured 1.000 px/f (wolf state 75,
node speed 256) and 0.699 px/f (snake state 9, node speed 179); patrol speed 0.547 px/f = waypoint param 140/256.

### 5. Node catalog (word0 / word1 → meaning). Full list with evidence in `ai.json → node_types`.

Composites: **Sequence** `0x0801b63f` [n, children] (run in order); **RaceFirst** `0x0801b887` (all entered, done
when first child (in order) reports done); **RaceAll** `0x0801b861` (all ticked each frame, done if any).
Conditions: **IfPlayerWithin** `0x0801aef3` [r², near, far] — re-evaluated every frame, switching exits/enters
children; **IfPlayerSeen** `0x0801d749` [r², seen, notSeen] — within r and not behind (test `0x0801d5d0`);
**Chance** `0x0801ba47` [p, child] — ticks the child with probability p each frame;
**WhileInMeleeRange** `0x0801c05b` / `B 0x0801ea59` [child] — player hurt rect overlaps enemy box ± a few px.
Transitions: **GotoState** `0x0801ab65` [id], **PushState** `0x0801a71d` [id], **PopState** `0x0801a9a5`,
**SubState** `0x0801b511` [id] (inline another state's trees), **ClassDefaultState** `0x0801c451`.
Timers: **Wait** `0x0801bbad` [frames], **WaitPose** `0x0801c95f` [frames].
Movement: **ChasePlayer** `0x0801b333` [spd] (velocity = d·spd/max(|dx|,|dy|), wall sliding), **FleePlayer**
`0x0801b4f3`, **AlignWithPlayer** `0x0801b15f` (get within 3 px of player row/column then face him),
**BackAway** `0x0801c2ad`, **BackAwayUntilBlocked** `0x0801c431`, **Patrol** `0x0801a98d` (spawn waypoints).
Attacks: **MeleeStrike** `0x0801be8b` [anim variants] (instant hit on enter: box = own box extended 3–4 px toward
player, attack id 2, sfx rand(3)+0x22, anim 6+rand(n), done when anim ends), **ContactDamage** `0x0801b991`
(touch, 48-frame cooldown, id 2), **Charge** `0x0801c7f3` [spd, overshoot px] (dash, contact id 2),
**Pounce** `0x0801d255` [spd, range], **ShootEnergyBall** `0x0801bb6d` (32f anim, projectile at frame 8, id 1),
**ShootKiBlast** `0x0801bc6f` (32f anim, projectile at frame 16, id 3), **SelfDestructBurst** `0x0801ca43`,
**LobProjectile** `0x0801cca9` (arcing, gravity 0x20/frame², id 1), **ThrowProjectile** `0x0801cf73` (ninja, id 1),
**LaserBeam** `0x0801e145` (ladybug robots), boss specials **DrainGrab** `0x0801dbb3`, **SprayAttack** `0x0801dcdb`,
**BeamAttackTao** `0x0801e5c1`, **BeamAttack** `0x0801e77b` (all decoded in §13–§14).
Reactions / visuals: **Knockback** `0x0801a947` [p], **KnockbackInvulnerable** `0x0801acb7` [p], **FacePlayer**
`0x0801c0d7`, **SetIdleAnim** `0x0801c085`, **HitFlash** `0x0801bca9`, **StunAnim** `0x0801bcf1`,
**ClearAnim** `0x0801bd17`, **GlowFlash** `0x0801c5a3`, **Succeed** `0x0801bb89`, **Nop** `0x0801a825`.
On-hit-only (word0 = 0): **ApplyDamage** (word1 `0x0801a6e1`), **IfAttackId** `0x0801a805` [id-or-type, child],
**IfKnockbackBlocked** `0x0801acd5` [blocked, free], **HealFractionOfDamage** `0x0801c489` [frac/65536].

Knockback math (`0x0801a947`, VERIFIED(disasm) + one emulator observation): direction = hit direction;
`k = (rand(64)+96)·p >> 7`; velocity along the hit direction = 8·k (1/256 px/f) i.e. 0.375–0.62 px/f for p=16,
plus perpendicular jitter `(rand(32768)-16384)·k >> 12`; slides while the hurt animation (anim state 2) plays.
Observed: snake hit by Piccolo's melee in the open → state 2, slid 16 px over 32 frames, stayed hittable.

### 6. Generic hit handling (states 1–7, 16–20, 111–115) — VERIFIED(disasm), parts VERIFIED in emulator

Ordinary enemies (on-hit chain of idle states → state 3; active states delegate to it via ClassDefaultState):
1. ApplyDamage (HP -= damage).
2. SubState 7: if the attack id/type byte is **9** → push state 6 (stun 128 f with dizzy anim, flashing for the
   last 32 f) + push state 4 (Knockback 16); if **15** → push state 5 (stun 256 f, flashing last 64 f).
   (Attack ids 9 and 15 are player special attacks — see moves.json; 9 = power 0x40 energy, 15 = power 0.)
3. IfKnockbackBlocked: wall behind the enemy → push state 1 (KnockbackInvulnerable 40: entity unhittable until
   the hurt anim ends); free ground → push state 2 (Knockback 16; any further hit restarts the reaction).
While stunned (state 5/6) further hits apply damage, refresh/extend the stun and push a knockback (state 4).

"Fighter" bosses (Androids, Cell Jr, Cooler, Yamcha, Krillin/Tien, General Tao, Cell) use state 18 instead:
same structure with Knockback 32 / KnockbackInvulnerable 48 (states 16/17) and stun states 19/20 (20 adds an
initial knockback). Cell Nonperfect/Perfect (ai 116/148) use the twin set 111–115 (Knockback 24).

Android 19 / Android 20 (ai 29) on-hit: **IfAttackId(type 1 = any energy attack)** → GlowFlash + heal
`raw_damage·0x3333>>16` (20 %) + Succeed (damage NOT applied); otherwise the fighter handler. VERIFIED: with the
class-5 handler patched onto a snake (`en_verify_absorb.py`), ki blasts healed +26/27 (raw 133 → 26.6) and did no
damage, melee did normal damage (−82) and caused knockback.

### 7. Behaviour archetypes (decoded trees: `ai.json → states[i].text`)

* **ai 8 "brawler"** (snakes, scorpions, Kuma, Eggbots, Tiger Bandit 8, Android 16/Frieza rows): state 8 idle
  (Patrol) until seen within **72 px** → state 9: chase **0.70 px/f**; while in melee range, 10 %/frame → state 10:
  MeleeStrike, Wait 30 f, back to 9. Observed snake hit cadence on a standing player: every 64–92 frames.
* **ai 74 "wolf"** (wolves, saber-tooth tigers): idle until seen within **80 px** → 75: chase 1.0 px/f until within
  48 px → 76: face player, wait (3 %/f), then 20 % → 77 (hover: if <32 px flee 1.0 px/f, else 2 %/f back to 75)
  or → 78: rush 1.2 px/f, in melee range 50 %/f → 79: MeleeStrike then back off 1.0 px/f (10 f) → 76.
* **ai 66 "charger"** (triceratops 54/100, warthogs): seen 72 → chase 0.7; at <48 px 3 %/f → pose 60 f → Pounce
  1.2 px/f, range 56 px → rest 32 f; melee in range.
* **ai 59 "T-Rex / Triceratops King"**: within 72 → chase 0.7; at <48 px 3 %/f → pose 30 f → Pounce 2.0 px/f
  range 72 px; melee strike; rest.
* **ai 11 "henchman shooter"** (Warlord's Henchman): seen 72 → keep 30–60 px distance (flee 0.7 if closer than 30),
  AlignWithPlayer 0.5 px/f, 5 %/f → ShootEnergyBall (id 1), wait 16 f.
* **ai 45 "bandit"** (Tiger Bandit 9/10): seen 72 → at mid range align 0.5 px/f + 5 %/f LobProjectile (arcing id 1);
  close (<48, or 50 %/f) → approach 0.4–0.8 px/f, melee strike (anim variants 2), wait 30 f.
* **ai 52 "ninja"**: within 60 → like 45 but align 0.9 px/f, special = ThrowProjectile, rush 1.2 px/f.
* **ai 32/38 "flyers"** (hawks / pterodactyls): hop/fly anim, seen 48/72 → circle (face, 3 %/f) and Charge
  1.5 px/f (56 px overshoot) / 2.0 px/f (64 px); flee at < 40/48 px; chase 1.0 when > 88 px.
* **ai 106 "ladybug robot"**: seen 60 → align 0.7, LaserBeam (max 64 px), flee/hover 0.7.
* **ai 149 "alligator"**: seen 72 → chase 0.7, melee (box B) 10 %/f.
* **ai 91 "Destroyer robot"**: within 88 → SprayAttack (0x08701e4c, 128-frame particle spray) / mid-range align &
  5 %/f special, close melee (0.4–0.8 px/f approach).
* **ai 21 "fighter"** (Android 17/18, Cell Jr, Cooler, Yamcha) and **ai 29** (Android 19/20 = ai 21 + absorb):
  within 88 → melee mode (24/25: approach 0.5→1.0 px/f, 4 %/f or 50 %/f in range → state 26: MeleeStrike (3 anim
  variants) then 25 % repeat combo / 25 % → 28 / else 27 back-off); beyond 72 px → ranged mode (22: align 0.5,
  5 %/f → 23 ShootKiBlast (id 3), then 25 % stay ranged / 75 % close in at 1.2 px/f).
* **ai 98 "sparring"** (Krillin, Tien): like 21 with chase 0.5/1.0.
* **ai 80 "Cell (imperfect)"**: fighter + state 85 (player not within 24 px: 35 % DrainGrab (0x08701e1c) / 65 % strike).
* **ai 116 "Cell Nonperfect"**: fighter with 50 % choice between ShootKiBlast and BeamAttack (0x08704788), plus
  DrainGrab in close range; uses hit handler 113.
* **ai 129 "General Tao"**: fighter; ranged choice 40 % BeamAttackTao (0x08703914) / 60 % ShootKiBlast.
* **ai 138 / 148 "Perfect Cell"**: fighter; ranged 50 % ShootKiBlast / 50 % BeamAttack (0x087047d4); melee combos.
* **ai 15** (class 94 triceratops): Patrol + ContactDamage, invulnerable (above).
* **ai 30/31** (class 36, 1 HP, 1 exp, map 119): on hit takes damage and Knockback(0) (INFERRED: breakable/punchable object).
* **ai 0**: Nop (classes 13, 46, 52, 82, 87; none spawned).

### 8. Death, exp and drops

* Death (takeHit → class death node fn2). Default node `0x086d2b6c` → `0x0801a74d`: exp award (`0x0800e4d2`),
  on-death event (`0x0800e39c` queues world event vtable `0x080238fc` whose run copies the death position to
  `0x03001fd0` and calls the spawn's on-death node), explosion effect (size 0x28) and random death sfx.
  `0x086ff3c4` (bosses) does the same without the explosion (VERIFIED(disasm)).
* **Exp award** (`0x0800e4d2`, VERIFIED): `E = class.exp; if E > 5: L = active character level; if L == 50 → no exp;
  span = expTable[L] − expTable[L−1]; E = max(E, span>>7) then min(E, span>>1)`. Rows with exp ≤ 5 are not clamped.
  Observed: snake (exp 13) killed by Lv40 Piccolo awarded 4747 = 607735>>7.
* **Drops** are per-spawn on-death scripts (spawn def +0x14 = `[0x0801069b, script*]`). Decoded VM: `00 xx` push u8,
  `01` push zig-zag varint, `02 v`/`0x4a+v` call builtin v, `03 v` store local, `04..10` && || neg + - * / == != > >= < <=,
  `12 n` jump, `13 n` jump-if-zero, `14` pop, `11` end; builtins 0x0d–0x10 player HP/maxHP/EP/maxEP, 0x11 rand(n),
  0x12 chance(n %), 0x13/0x14 has/!has item, 0x15 item count (owned + on ground), 0x57 spawn pickup(item) at the death position.
  Generic drop scripts (one d100 roll `r`, first matching rule wins):

  | script | uses | rules |
  |---|---|---|
  | `0x083ee120` | 418 spawns | r<2 & HP<max → HealthBonusLarge; r<7 & HP<max → Medium; r<20 & HP<max → Small; r<23 & EP<max → EnergyBonusLarge; r<28 & EP<max → Medium; r<50 & EP<max → Small |
  | `0x083f4ee8` | 104 | as above but Medium health r<15; plus r<10 & flag/item 128 & Fish count<3 → special object 0 (INFERRED: Fish) |
  | `0x083eede4` | 58 | r<2 HBL; r<4 → **Fish** (item 43, unconditional); r<9 HBM; r<20 HBS; energy as 0x083ee120 |
  | `0x083f7b88` | 10 | same as 0x083f4ee8 |
  | `0x083ffaf4` | 7 | r<2 HBL, r<7 HBM, r<20 HBS (HP<max); r<40 EBL, r<60 EBM, r<80 EBS (EP<max) |
  | `0x083feb28` | 7 | unconditional: r<5 HBS, r<20 HBM, r<35 EBS, r<50 EBM |
  | `0x083fe538` | 6 | unconditional: r<3 HBL, r<6 EBL, r<15 HBM, r<25 EBM, r<40 HBS, r<60 EBS |

  Boss/story spawns run cutscene sequences instead (engine 0x0800b4ae) — decoded in §15.
  Effects of the orbs (amounts) are in the items notes.

### 9. Bosses and scripted fights

Boss classes are placed in the normal map spawn lists; almost all boss spawns use **spawn condition 1** (never
spawned at map load; activated by event scripts — condition evaluator `0x08003f06` called from the map loader at
`0x0800c526`, VERIFIED(disasm)). Exceptions: Triceratops King and Cell (imperfect) are gated on story flags
(hi16=2 "flag clear"), one Android 17 spawn (map 297) and Perfect Cell row 22 (map 325) have condition 0.
HP/stats from the class table:

| boss | class | map(s) (zone/area, name) | HP | STR/POW/END | ai |
|---|---|---|---|---|---|
| General Tao | 44 | 46 (2/50 East District 439) | 6400 | 50/55/60 | 129 |
| Triceratops King | 101 | 105 (4/58 Triceratops Jungle, cond flag) | 850 | 21/1/25 (64/64 mult) | 59 |
| Cooler | 29 | 172 (6/61 New Namek) | 10200 | 75/85/79 | 21 |
| Yamcha / Krillin / Tien (sparring) | 106/64/98 | 175 (6/70 Sparring Arena) | 800/1800/2110 | 18/12/14, 23/24/17, 35/29/27 | 21/98/98 |
| Android 19 | 5 | 191 (7/15 Southern Continent) | 2970 | 21/14/11 | 29 (absorbs energy) |
| Android 20 (Dr. Gero) | 6, 7 | 195 (7/19), 215/229/242 (Northern Mountains) | 2810 / 1900 | 24/19/14, 25/21/17 | 29 (absorbs energy) |
| Android 18 | 4 | 251 (8/49 Northern Mountains) | 2910 | 29/34/27 | 21 |
| Cell (imperfect) | 60 | 287 (10/20 Gingertown, flag cond) | 5210 | 32/32/37 | 80 |
| Android 17 | 3 | 296–298 (11/4–6 Master Roshi's Island) | 3550 | 32/39/34 | 21 |
| Cell Nonperfect | 76 | 302 (12/4 Tropical Islands) | 4890 | 39/41/46 | 116 |
| Perfect Cell | 78, 79 | 304 (12/6 Tropical Islands) | 5160 / 5500 | 44/46/50 | 148 |
| Perfect Cell (Cell Games) | 21 | 324 (14/2) | 6500 | 50/55/60 | 138 |
| Perfect Cell (Cell Games, Gohan) | 22–25 | 325 (14/3) | 6500 ×4, 8230 | 50/55/60, 60/59/65 | 138 |
| Cell Junior ×3 | 26 | 325 (14/3) | 4990 | 46/50/46 | 21 |

Exp 0 for all of these except the sparring partners (600/1200/2500) and Triceratops King (1500); the real
rewards come from the death cutscenes (`giveExp`, see §15). Fight phases are decoded in §15 / `bosses.json`.
Android 16, Frieza, King Cold and other named rows have no spawns (story NPCs). Class 41 (gfx 48 = Frieza, map 14)
is the "Frieza" of Gohan's opening nightmare (HP 130).

### 10. Where enemies appear (maps.json) — VERIFIED

Map record (0x38 B): `+0 zone, +1 area` (key for warps / debug Map Test lookup `0x0800eb7c`), `+2` entry/variation
count, `+3..+7` counts of the lists at `+0x10` (conditional objects `[cond, objdef*]`), `+0x14` (object defs),
`+0x18`, `+0x1c`, `+0x20`; `+0xc` = location-name string id into `0x080e2840` (VERIFIED: map 0 → 227 "Outside
Pepper Town", map 12 → 203 "East District 439", matching the in-game banners).
Enemy object def (factory `0x0800e77f`): `+4 spawn condition` (0 always, 1 script-only, hi16=1/2/3 flag
set/clear/test-and-clear, else script), `+8 class`, `+0xc s16 x, s16 y`, `+0x10 gfx id`, `+0x14 on-death node`,
`+0x18 waypoint count`, `+0x1c waypoint ptrs`. Waypoint `[factory, s16 x, s16 y, param]`; factory `0x0800d625`
walks to (x,y) at param/256 px/f (common params 102–384 → 0.4–1.5 px/f). 666 enemy spawns over 167 maps.
Other object factories: `0x0800b711` NPC (245), `0x0800d6df` walking NPC (197).

### 11. Save states made
* `work/states/log2_en_map13.ss` — debug-warped map 13 (zone 2 area 7), idle wolves (class 17, state 74) and snakes
  (class 53, state 8), Lv40 party.
* `work/states/log2_en_android19.ss` — map 191 with Android 19's spawn condition patched to 0 (cutscene running;
  input locked).

### 12. Open gaps
* Cutscene actor/camera commands (cutscene cmd ≥ 2) and about half of the ~160 script builtins are only
  catalogued, not named; they don't affect combat. The pre-fight cutscenes that first activate condition-1 bosses
  were located by scanning (`bs_scan_spawn.py`) but are not tied to their maps in `bosses.json`.
* Which player character fights phases that are not preceded by an `setActiveCharacter` in a death cutscene
  (e.g. Cell Games class 21/22, Cell Nonperfect) comes from pre-fight cutscenes — not traced.
* Boss beam head growth/hitbox use the player Kamehameha class (see Part B); its geometry was not re-measured for
  the boss variants (only damage cadence/amount).
* Strike-animation lengths per enemy gfx (recovery after MeleeStrike) are only measured for the snake (17 f);
  others need the graphics agent's animation tables.
* Waypoint kinds other than walk-to; ai 15 / class 94 intent (indestructible obstacle: behaviour verified).

### 13. Enemy projectiles — `enemies.json → projectiles` (extractor `en_attacks.py`)

All derive from the projectile base `0x08013bc0` (fields: +0x14 direction = owner facing, +0x15 team = owner+0x16c
(0 for enemies), +0x18 frame data `[s8 dx, s8 dy, u8 w, u8 h, ...]`, +0x1c owner, +0x20/+0x24 position 24.8,
+0x28..+0x34 hit rect = position − (dx,dy) .. + (w,h), `0x08013bf4`). Vtable layout: +0x24 onMessage, +0x28 update.
Off-screen test `0x08013c22` (camera view 240×160), wall test `0x08005d6c`. Hits are broadcast with `0x08006e0a`
(message 0 → player/enemies check the rect against their hurt rect). Measured with the arena harness
(`bs_arena.py`/`bs_watch.py`: force-spawn any class on quiet map 13 by patching objdef `0x083ee440` in the emulated
cartridge, Lv40 Piccolo STR 40 / POW 55 / END 53 standing still).

| key | ctor / vtable | spawned by (frame) | speed | hitbox | attack | team | ends | evidence |
|---|---|---|---|---|---|---|---|---|
| ki_blast | `0x0800e8b0` / `0x08024cbc` | ShootKiBlast anim frame 16 | 3.0 px/f (table `0x0840e554`) | 16×16 centred | id 3 (1024) | 0 | camera exit, wall or hit → 16 f explosion; cancelled by overlapping player energy attack | VERIFIED |
| energy_ball | `0x080160c0` / `0x08024258` | ShootEnergyBall anim frame 8 | 1.75 (`0x086e5ba8`) | 16×16 with facing offset (`0x086e5dc8`) | id 1 | 0 | camera exit / wall / hit; cancelled by player energy or attack ids 8/14 | VERIFIED |
| thrown_star | `0x0801a0b0` / `0x08024f0c` | ThrowProjectile anim frame 8 | 1.75 (`0x086fe06c`) | 8×8 | id 1 | 0 | camera exit / wall / hit | VERIFIED |
| lob_rock | `0x0801ccc8` / `0x08024204` | LobProjectile anim frame 16 | ground vel = Δ(player−self) along facing axis × 0x71d/256 per frame; z0 8 px, vz 2 px/f, g 0x20/256 | 16×16 at landing | id 1 | **2** | lands after ~35 f (measured), one hit + explosion/debris | VERIFIED |
| laser | `0x0801e224` / `0x08024e5c` | LaserBeam node | grows 8 px/f to param (64) | segment ±4 px | id 3 | 0 | 60 f then retract 8 px/f (≈68 f); one hit max | VERIFIED |
| boss_beam | `0x080143a8` / `0x08024acc` | BeamAttack / BeamAttackTao | player Kamehameha beam (head 3 px/f) | 16×16 head box per frame | id 10 (power 0x40) × scale 0x3000 | 0 | 90-frame anim | VERIFIED: hits every 32 f (player i-frames), Cell NP 24–36, Tao 48–60 |
| spray_particle | `0x0801dcfc` / `0x080250a8` | SprayAttack frames 4–127, 1/frame | 2 px/f ± rand jitter 0.375 | 16×16 centred | id 3 × scale 51 (≈1/20) | direct | 20 f; one hit per particle, no hitstun / no i-frames | VERIFIED: 2–4 dmg every 1–2 f |

No enemy projectile homes; all fly along the shooter's facing (the AI aligns/faces first). None pierce except
the laser/beam (persistent). Fire cadence comes from the AI chances (e.g. ai 11: aligned → 5 %/f → shot, then
16 f wait; fighters: 5 %/f when aligned, ~58–67 f mean interval at 100 px measured).

### 14. Boss special attacks — VERIFIED (disasm + forced in the arena with `bs_special.py`, which patches the
AI chance/radius params in the emulated ROM to force the branch)

* **DrainGrab** `0x0801dbb3` (Cell imperfect state 89, Cell Nonperfect state 127): the player is put in anim
  `0x0300227c` (held, `0x080251e0`: 240 frames, each newly pressed A removes 16 extra frames) and Cell in
  `0x03002280` (`0x08024bb4`). At hold frame 8: hit with attack id 2 (melee, Cell STR) at scale 256 (¼), then every
  16 frames a hit at scale 128 (⅛), applied directly; Cell heals by each hit's damage. The node has no range check
  (in ai 80 it is only chosen from state 85, i.e. after the melee-range approach, when the player is not within
  24 px: 35 % grab / 65 % strike). Measured: 8 then 4 damage per tick (first tick crit 17), released after 240 f.
* **SprayAttack** `0x0801dcdb` (Destroyer state 94, chosen whenever the player is within 88 px): 128-frame spray,
  one particle per frame from frame 4 (see `spray_particle`), mouth offsets `0x08701e28` by facing; each particle
  2 damage on END 53, applied without hitstun or invulnerability, i.e. continuous damage while in the stream.
* **BeamAttack** `0x0801e77b` (Cell Nonperfect state 120 / Perfect states 142) and **BeamAttackTao**
  `0x0801e5c1` (Tao state 131): 90-frame beam using the player Kamehameha object with scale 0x3000; measured
  one hit per 32 frames while the player stands in it.
* **LaserBeam** `0x0801e145` (ladybug state 109) and **SelfDestructBurst** `0x0801ca43` (Eggbot death: flash, 40 f
  pose, 32×32 team-2 burst, explode) — see §13 / `ai.json`.

### 15. Boss fights and phases — `bosses.json` (extractor `extract_bosses.py`, chain decoder `bs_chain.py`)

Phase model (VERIFIED by disassembly of all node types): no AI node reads the boss's HP, so a class row never
changes behaviour at an HP threshold. Multi-phase fights are chains of spawn defs: when a phase reaches 0 HP its
death cutscene (spawn def +0x14 = `[0x080106b3, sequence*]`, cutscene engine `0x0800b4ae`; steps `[u16 next, u8 cmd]`,
cmd 0 = VM script, cmd 1 = dialogue) runs, then spawns the next spawn def with builtin `0x54
spawnMapObject(idx)` (`0x0800c73a`) or converts a cutscene actor into it with `0x68
replaceActorWithMapObject(actor, idx)` (`0x0800c75c`). Other decoded builtins: `0x1b/0x1c` set/clear story flag,
`0x45`/`0x28` wait n frames, `0x46` set active character (`0x0800a680`), `0x8b` giveExp, `0x26/0x27` change map,
`0x21` camera follow, `0x56` player control on/off, setters `0xd/0xf` = player HP/EP (HP/EP refill).
Script opcode `0x1d n v1..vn` pushes n zig-zag varints (decoder `bs_vm.py`).

| fight | phases (class @ map/object) → next | player char (from cutscenes) | notes |
|---|---|---|---|
| Cell Games | 21 @324 → (map change) 22 @325/0 → 23 @325/5 → Cell Jr 26 @8 → 26 @9 → 26 @10 → 24 @11 → 25 @12 | 23: Gohan; Jr #1 Piccolo, #2 Vegeta, #3 Trunks; 24: Gohan | each Cell Jr death: +50000 exp and character switch; 25 has HP 8230, STR/POW/END 60/59/65 |
| Semi-perfect → Perfect Cell | 76 @302 → (map 303→304, HP/EP refill) 78 @304/4 → 79 @304/1 | 78: Trunks (`setActiveCharacter(3)`) | 76 death +30000 exp; 79 death +40000 exp |
| Android 17 | 3 @296 → (map) 3 @297 (cond 0) … 3 @298/0 → actor → 3 @298/4 | — | final death +15000 exp |
| Android 18 | 4 @251 | then Piccolo | +8000 exp |
| Android 19 / 20 | 5 @191; 6 @195; 7 @215/229/242 | — | +3500 / +5000 / +7000 exp; energy absorb |
| General Tao | 44 @46 | — | +50000 exp, Dragon Ball |
| Cooler, sparring, Triceratops King, Newman, nightmare Frieza | single phase | — | rewards in `bosses.json` |

Boss invulnerability: only the generic wall-knockback window (KnockbackInvulnerable 48/40) and while not spawned.
Heals: Android 19/20 absorb energy hits (+20 % of raw damage, no damage); Cell drain grab heals Cell. No teleport
node exists. Cell Juniors are not AI summons but three sequential spawn defs. Measured cadence/damage per boss
(30 px and 100 px start, 900 frames) is in `bosses.json → phases[].measured` (`bs_measure.py`), e.g. Perfect Cell
class 25 at 100 px: 14 hits, mean interval 57.8 f, 60–94 damage to Lv40 Piccolo; Cooler 186–203 per ki blast.

### 16. Contact and melee rules (simulator) — VERIFIED

* Body contact does **not** damage the player, except class 94 triceratops (ContactDamage: overlap → hit, then
  48-frame cooldown on the frame counter `0x03000e20`) and enemies in a Charge/Pounce dash (single hit).
  Snake at 14 px only damaged the player on strike-state entry frames.
* MeleeStrike: zero wind-up — the hit check happens on the first frame of the strike state (AI chance rolls act as
  the wind-up); hit box = own hurt rect half toward the player extended 3–4 px (horizontal: y from hurt centre to
  bottom+3, x from centre to right+3 or left−4 to centre; vertical: x = collision box, y = hurt top .. bottom−6
  (down) or .. collision-box top (up)) (`0x0801bd2c`). Recovery = strike anim (anim 6+rand(variants); snake 17 f)
  + the AI's following Wait (ai 8: 30 f; fighters: combo/back-off states).
* Hurtboxes per class/gfx: `enemies.json → classes[].graphics[].hurtbox_by_facing` (descriptor+0xc+16·facing,
  checked against emulator for wolf/snake) and `collision_box_wh`.


# Part D — Items, regen, progression, RNG

## LoG2 — Items, Regen, Progression, RNG

Scope of this section: the item table, inventory, item effects, enemy drops and field pickups, HP/EP regen,
exp/level-up, character availability/switching, death, saving, and the RNG.
Each claim is tagged **VERIFIED** (disassembly plus a matching emulator run) or **INFERRED** (disassembly only, or a reasonable reading of it).
Data files: `data/log2/items.json`, `data/log2/levels.json`, `data/log2/rng.json`. You can regenerate them with
`re/log2/extract_items.py`, `extract_levels.py` and `extract_rng.py`. These test scripts reproduce the results: `it_inv.py` (item use), `it_lvl.py` (level-up),
`it_regen.py` (regen), `it_drop.py`/`it_drop2.py`/`it_drops_all.py` (drops), `it_rngboot.py` (RNG at boot).
Save states: `work/states/log2_it_menu.ss` (pause menu), `log2_it_items.ss` (Items page), `log2_it_drop.ss`
(two dropped pickups on the ground), `log2_it_levelup.ss` (just after a level-up).

### 1. Memory map (party block, IWRAM 0x03000e90; ROM pointer at 0x083b5354 / 0x086ade1c)

| offset | type | meaning |
|---|---|---|
| +0x000 | u8 | active character index (0 Gohan, 1 Piccolo, 2 Vegeta, 3 Trunks, 4 Goku, 5 Hercule) **VERIFIED** |
| +0x001 | u8 | bit0 is set by the save routine (INFERRED: "save exists") |
| +0x002..4 | u8×3 | map zone/area/variation, written at save time (0x08004030) **VERIFIED from code** |
| +0x00c + 28·i | record | character record i (layout in levels.json `record_layout`) **VERIFIED** |
| +0x0b4..0x117 | bits | story/event flag bitfield (tested by 0x08003e44: `party[0xb4+4+(n>>3)] & (1<<(n&7))`) INFERRED |
| +0x12b + id | u8 | **inventory**: count of item `id` (0..49) **VERIFIED** (Items menu shows it; using an item decrements it) |
| +0x160 | s16×2 | saved player x,y (0x08004030) INFERRED |

`0x08004030` saves this whole 0x164-byte block (plus a checksum from 0x0802153e) to EEPROM (`EEPROM_V122`).
The slot is selected by 0x03000e8c. The block is copied to a stack buffer first and 0x08003c24 re-initialises the live block.
This is INFERRED from the code path; the save itself was not exercised in the emulator.

New-game init (`0x08003c24`): active=3 (Trunks, for the prologue). It DMA-copies the 6 initial records from ROM 0x081d9548 and zero-fills the flags and inventory.

Character record (28 bytes): HP, maxHP, EP and maxEP are s16. Then come the form id, the selected slot, STR/POW/END as u16 8.8 fixed point, exp (u32), flags (u32) and the move slots (u8[4]).
Flags bit0 marks the character as unlocked. Every list that lets you pick a character tests it:
the status-screen character list `0x080034e2` and the Switch screen `0x08010c1e` / count `0x08003e00`. **VERIFIED from code.**
All six initial records have flags = 0x40, so story scripts set bit0 when a character joins.
bit1 is counted by `0x08003e1e`, which has no direct BL callers. Its meaning is INFERRED only ("in party").
Bits 3..6 are the special-slot unlocks (player agent).

### 2. Items

The item table is at **0x086ade24**: 50 entries × 0x38 bytes. `items.json` has every row.

- `+0` stored_in_inventory: 0 means the item is used immediately on pickup (the bonus orbs).
- `+1` flag: set for Golden Cap. #17, Missing Nameks and the Dragon Balls. Meaning INFERRED: quest marker.
- `+2` show an "obtained" popup.
- `+3` max count.
- `+4/+6` u16 name and description string ids (string table **0x080e2840**).
- Sprite descriptors.
- `+0x30` use function.
- `+0x34` pickup function.

Pickup function 0x0800f3f1 (**VERIFIED from disasm**):
- If count ≥ max, the pickup is refused.
- If popup is set, it shows the popup.
- If stored, it increments the count; otherwise it calls the use function right away.

The ids 46–49 "***" use 0x0800f441, which shows the popup only.

| id | name | effect (use fn) | status |
|---|---|---|---|
| 0 | Senzu Bean (max 3) | HP=maxHP and EP=maxEP for the active character. Refused if both are already full (0x0800f6e9) | **VERIFIED** (HP 100→1055, EP 5→137, count 2→1) |
| 1 | Golden Cap. | runs script 0x083b6c50 (World Map) | INFERRED |
| 2/3/4 | STR Cap. +1/+3/+5 (max 99) | record.STR += n<<8 (n = whole points), clamped to 0x6400 = 100.0. Refused if STR ≥ 0x6400 | **VERIFIED** (+3: 10354→11122 = +768) |
| 5/6/7 | PWR Cap. +1/+3/+5 | the same on POW (+0xc). Refused only if POW == 0x6400 | **VERIFIED** (+5: +1280) |
| 8/9/10 | END Cap. +1/+3/+5 | the same on END (+0xe) | **VERIFIED** (+5: +1280) |
| 11/12/13 | HealthBonus Small/Med/Large | HP += maxHP·k>>16 with k = 0x199a/0x3334/0x4ccd (10%/20%/30%), clamped. Not stored; applied on touch | formula from disasm 0x0800f593 |
| 14/15/16 | EnergyBonus Small/Med/Large | EP += maxEP·k>>16 with k = 10%/20%/30% (0x0800f639 → clamp helper 0x080265ac) | disasm |
| 21 | Cookie | HP += 5 | **VERIFIED** (100→105) |
| 43 | Fish | the heal routine has a 10% case for id 0x2b, but the Fish's use fn is NULL, so it cannot be used from the menu | **VERIFIED** (count unchanged) |
| 18 | Album | opens the Album screen | INFERRED |
| others | quest/key items, Dragon Balls 1–7, Dragon Radar | no use fn | table |

After a Cap is applied, 0x080086a2 refreshes the player entity's stat bytes (entity+0x160 = record>>8).
Note the off-by-design check: the STR cap refuses when STR ≥ 100, while POW/END refuse only when the value is exactly 100.

#### Enemy drops (VERIFIED)
Every enemy spawn definition has a pointer at +0x14 to `[on_death_fn, script]`.
`0x0801069b` runs the script in the script VM (interpreter 0x080092e4, opcode table 0x083b5c00).
The VM is stack-based: `00 nn` pushes a byte, `02 xx` is a sub-op (table 0x083b5d28 = main-table entry 0x4a+xx), `03 nn` stores a local, `0b..10` are comparisons, `04/05` are and/or, `12/13` are jmp/jz, and `11` ends the script.

The standard drop script (418 of the 666 spawns, e.g. 0x083ee120) is:
```
L0 = rand(100)                         ; sub-op 0x11 -> 0x08009569 -> rand()
if L0 <  2 && HP != maxHP: drop 13 (HealthBonusLarge)
elif L0 < 7 && HP != maxHP: drop 12 (Medium)
elif L0 < 20 && HP != maxHP: drop 11 (Small)
elif L0 < 23 && EP != maxEP: drop 16 (EnergyLarge)
elif L0 < 28 && EP != maxEP: drop 15
elif L0 < 50 && EP != maxEP: drop 14
```
Each test is "L0 < T && condition". When the condition fails, control falls to the next row.
For example, a roll of 1 with full HP still drops EnergyBonusLarge if EP is not full. With full HP and full EP, nothing drops.
Sub-op 0x57 (0x0800aba5) spawns the pickup at the dying entity (0x03001fd0) via the ctor 0x08011802.
The emulator matched this: killing snakes with Piccolo's HP below max dropped ids 12 and 11, and the call stack showed script pc 0x083ee14a.

There are 19 distinct tables in total; all of them are in `items.json/drop_tables` with each spawn's `drop_table_id`. Variants:
- Medium threshold 9 (59 spawns) or 15 (114 spawns).
- An unconditional table, 6 spawns, with cumulative thresholds 3/6/15/25/40/60. That gives HP-L 3%, EP-L 3%, HP-M 9%, EP-M 10%, HP-S 15%, EP-S 20%, and nothing 40%.
- A 5/20/35/50 table, 7 spawns.
- A "boss-ish" table with EP thresholds at 40/60/80, 7 spawns.
- 12 single-use copies of the standard table that first set a story flag (`push n; call 0x1b`).
- 14 spawns that run event scripts with no drop.

0x08010699 means no drop; 0x080106b3 means a scripted event.

#### Pickup objects
The ctor is 0x08011802 and the vtable 0x08024c90.
- Height is in 1/1024 px and starts at 0x2000; vz starts at 0x800.
- Gravity subtracts 0x80 from vz each frame.
- When the pickup reaches the ground, the bounce halves both height and velocity. It settles once vz < 0x80 and becomes collectible after that.
- On touch it calls the item's pickup function.
- Temporary pickups (not stored and no popup) are registered with a watcher (0x0801177c / 0x0800d8b8). The watcher removes them when the area changes (INFERRED).

On screen they persisted for at least 3000 frames (**VERIFIED**), so there is no timeout.
Map-placed items use object 0x0801a2f8 (vtable 0x080252e8) and chests/touch items use 0x0800fa6c (vtable 0x08024230).
Script op sub-0x57 and main op 0x64 (0x080096ab: give item by calling its pickup fn) are how scripts give items.

### 3. Regen (VERIFIED for the normal mode)
The player entity holds a "power mode" object at player+0x180. There are 5 mode handlers, and their pointer table is filled at 0x080232e4 into 0x030021e0.
The form table 0x081d967c (last field) selects the mode.

In mode 0, the normal mode, handler `0x08017029`:
```
acc(player+0x184) += 0x6e  every frame (any input, moving or not)
if acc >= 0x10000:  (every 65536/110 = 595.8 frames ≈ 9.9 s)
   active.EP += active.maxEP*0x7ae >> 16          ; +3.0%, clamped
   for each other character c (all 6 records, active skipped):
       c.EP += c.maxEP*0xccd >> 16 ; c.HP += c.maxHP*0xccd >> 16   ; +5%, clamped
   acc &= 0xffff
```
- Measured: Piccolo, maxEP 137, went EP 10 → 14 → 18 at frames 2739 and 3335 (596 frames apart). The result was the same while holding a direction or mashing A.
- Benched Gohan (859/133) went HP 100 → 142 → 184 and EP 10 → 16 → 22.
- **The active character has no natural HP regen.**
- acc resets to 0 when the mode is entered.

The other modes (transformation states; their semantics belong to the player notes) are INFERRED from disasm:
- mode1 0x0801710f: HP +1% every 20 frames; stats fixed at 100.
- mode2 0x080171e5: EP −1 every ~40 frames, reverts at EP 0; STR+15/POW+10/END+15.
- mode3 0x08017307: EP −1 every 20 frames; +10 to all stats.
- mode4 0x08017401: HP +1% and EP −1 every 20 frames.

Healing by "Sleep" and story events goes through script ops 0xb7/0xb8 (set any char HP/EP, clamped) and 0xe9..0xec (set active HP/maxHP/EP/maxEP). This is INFERRED; no bed was exercised.

### 4. Progression
**Exp award** (`0x0800e4d2`, called from the enemy death handler 0x0801a74c) is **VERIFIED**:
```
E = classRow.exp
if E == 0: return
if E > 5:
   L = level(active); if L == 50: return
   span = expTable[L] - expTable[L-1]
   if (span>>7) > E: E = span>>7       ; floor: 1/128 of the current level's span
   elif (span>>1) < E: E = span>>1     ; ceiling: half a level per kill
queue msg(vtable 0x080245b0, E) -> 0x08026404 -> addExp(party, activeIdx, E) 0x08003d58
```
Only the active character gets exp. Piccolo at level 40 killing a snake (exp 13) got +4747 = 607735>>7.

**Level** = 1 + max{i : exp ≥ table[i]}. The table is at 0x081d9834 with 50 u32 entries, and the cap is 50 (0x08004214).

**Level-up** (`0x08003ca0(party, idx, old, new)`) is **VERIFIED**: it predicted all 7 fields exactly for Piccolo 40 → 41 with `it_lvl.py`.
```
for stat in STR,POW,END (8.8 fixed):  (min,max) = growth[idx][stat]   ; table 0x081d95f0, 12 bytes/char
    stat = min(0x6400, stat + min*n + rand((max-min+1)*n))             ; n = new-old, one roll per stat
for each level gained:
    maxHP += maxHP*(rand(0x51f)+0xe14) >> 16    ; +5.50%..+7.50%
    maxEP += rand(3)+2                          ; +2..+4
    HP = maxHP; EP = maxEP
```
Then script 0x083b6524 shows "%s has achieved Level %d!" and "Attribute Bonus" with the integer deltas (0x03001fc1..3).

Growth per level in stat points (min–max):

| char | STR | POW | END |
|---|---|---|---|
| Gohan | 1.24–2.02 | 1.18–1.96 | 1.00–1.78 |
| Piccolo | 0.88–1.66 | 0.96–1.74 | 0.81–1.59 |
| Vegeta | 0.91–1.70 | 0.86–1.64 | 0.96–1.74 |
| Trunks | 1.12–1.90 | 0.67–1.45 | 0.71–1.49 |
| Goku | 1.00–1.78 | 1.00–1.78 | 0.77–1.55 |
| Hercule | 0.25–0.75 each | | |

Initial records are in levels.json. Gohan starts with 40 exp, which is still level 1.
Story script ops can force a character up to level N: the handler near 0x0800ac2e runs the level-up and then sets exp = table[N-1].

**Switching**: the save/switch point object 0x080109ec offers a menu if more than one character has flags bit0. Choice 2 opens the character select (0x08010b08, which lists the bit0 characters); choice 1 saves (0x08004030, then "Game Saved!").

**Death**: the player takeHit 0x08008784 clamps HP at 0. At 0 it sets the player state to 0x21 and queues an object (vtable 0x08023a50) with a 200-frame timer. Its handler 0x080085fc runs dialog 0x083b63b4 ("You have died!"). The aftermath (reload) is INFERRED.

### 5. RNG (`data/log2/rng.json`)
State is two u32 at 0x03003890/94. The generator `0x080213fc`:
```
r1 = (a + 0x02cbbc71) ^ b
r0 = (rol(b,3) + 0x0632d80f) ^ r1
a = r1; b = r0; return r1 - r0
rand(n)   = ((rng() & 0xffff) * n) >> 16        ; 0x0802141c
chance(p) = (rng() & 0xffff) < p                ; 0x0802142e
```
- **Seeding (VERIFIED)**: the state is 0,0 after crt0's IWRAM clear, and nothing writes it during the boot logos.
- The only per-frame advance is the title/intro screen manager (0x08000c1c). The seed therefore depends on how many frames you spend on the title.
- In gameplay it is called only on demand: 120 idle frames in a map made 0 calls, and during combat it is called about every 16 frames.
- All 88 BL call sites are listed with their purpose.
- Gameplay-relevant callers:
  - melee crit `chance(0xa4*STR)` at 0x0800f930;
  - damage variance `rand(26)` at 0x0800f968;
  - level-up rolls;
  - script `rand` (enemy drops) and `chance%` (0x28f5d·p>>8);
  - enemy AI nodes (0x0801a1c8..0x0801eaf8).
- Many other call sites only pick a random sound-effect variant (`rand(k)+base`, indexing the sfx table 0x080e15b0).

### Open gaps
- Flags bit1 and the item +1 flag: meaning not confirmed.
- Golden Capsule / Album: not exercised in the emulator.
- Sleep/bed heal: not located concretely (expected in a map script).
- Despawn of temporary pickups on area change: inferred, not observed.
- Quest/journal data: the strings are at ~0x086d6ca9, but the table was not decoded.
- Transformation modes 1–4: regen/drain is from disasm only.

# Consolidated open gaps

* Boss fight chains decoded (Part C §15); remaining: pre-fight activation cutscenes not tied to maps, cutscene actor commands (cmd ≥ 2) and ~half of the script builtins unnamed.
* Boss beams reuse the player Kamehameha object: geometry not re-measured for bosses; per-gfx strike-animation lengths (enemy melee recovery) only measured for the snake.
* Which story scripts unlock which moves (flags set only by script opcodes 0x4F–0x53, 0x5B).
* Not run in emulator: Health/Energy Bonus pickups (10/20/30%), Golden Capsule, Album, Sleep heal, transformation-mode regen/drain, wall-knockback i-frames, SBC piercing two targets.
* Undecoded: record flag bit1, form table bytes +2/+3, item table +1 flag, quest/journal table, idle state 0x0E.
