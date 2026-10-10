# Dragon Ball Z: The Legacy of Goku (GBA, USA) — game mechanics

ROM: `roms/log1.gba`, game code `ALGE`, SHA-1 `a3c8c76742f11fc6a3f6e3fcca20992458cac0b5`.
All addresses are GBA bus addresses (ROM file offset = address − 0x08000000). Code is Thumb
unless noted. "rand(n)" = main RNG, inclusive range [0, n]; "rand2(n)" = secondary RNG, [0, n−1].

Status tags: **VERIFIED** = read in the disassembly *and* confirmed in the emulator (script
named); **STATIC** = read in the disassembly, not exercised in the emulator; **INFERRED** =
deduced from data/labels or partial evidence.

The game logic runs at 60 Hz: one logic tick per VBlank, frame counter `0x03003B6C`.

Everything here can be regenerated from the ROM with the scripts in `re/log1/` (see the end).

---------------------------------------------------------------------------------------------

## 1. Engine overview

### 1.1 Code layout
* All game code is Thumb in ROM `0x08000000–0x08010400` (~64 KB). Two ARM blobs are copied to
  IWRAM: the IRQ dispatcher (`0x08000138` → `0x03003BC0`) and the sound mixer (`0x08008814` →
  `0x03003D44`). Signed division is the ARM routine at `0x0800E568`
  (`div(d, n)` = n / d, truncating toward zero).
* Everything else (maps, scripts, characters, sprites, text) is data in `0x08010400+`.
  Text is plain ASCII (NUL terminated).

### 1.2 Frame loop (gameplay scene)
`0x08001212` (gameplay scene callback, once per frame):
1. `0x080022BA`: `0x08008018` **AI/"world" tick** (ki regen, enemy wake/sleep, all enemy
   decisions), `0x08002FF6` HUD/controller sync (copies player char HP/Ki/exp into the
   controller struct), `0x08004FD0` map animation.
2. `0x080026F8`: calls `state.update` (callback #1) of every live entity, last to first.
3. START pressed and HP ≠ 0 → pause menu.
Rendering (`0x08002556`, per frame) calls `state.anim_tick` (#2) and `state.visible` (#3) of every
entity, then sorts/draws (#4). Animation counters therefore advance once per frame.

### 1.3 Entities (0x40 bytes) — STATIC/VERIFIED
Pool of 0x60 entities at `0x02003840`, allocation bitmap `0x030007A0`, live list
`0x03000610` (u32 count, then pointers). Allocator `0x08002716`.

| off | meaning |
|---|---|
| +0x00/+0x02 | screen position (written by the visibility callback) |
| +0x04/+0x06 | size w,h (projectiles: hitbox size; Kamehameha grows these) |
| +0x08 | pointer to the animation row of the current state (`spritedef.anim + state*16`) |
| +0x0C | OAM flags (bits 4–5 = priority) |
| +0x0D | palette slot |
| +0x10 | animation tick counter (u8, +1 per frame) |
| +0x12 | animation frame index |
| +0x13 | facing: 0 down, 1 up, 2 left, 3 right |
| +0x15 | actor id (map actor index / script id) |
| +0x16 | projectile: 1 = enemy-owned |
| +0x18 | state table (array of 0x14-byte rows = 5 callbacks) |
| +0x1C | sprite definition (see §9) |
| +0x20/+0x22 | position, pixels (top-left of the 16×8 foot box) |
| +0x24 | generic counter (projectile charge / lifetime, player Ki-pose phase) |
| +0x28/+0x2A | projectile velocity (px/frame) / Kamehameha table pointer |
| +0x2C | projectile damage |
| +0x34 | current state row pointer |
| +0x38 | message handler `fn(entity, msg, arg)` |
| +0x3C | data: character struct (fighters), owner entity (projectiles), item id (pickups) |

`set_state(entity, n)` = `0x08002466`: `+0x34 = statetable + n*0x14`, `+0x08 = spritedef.anim +
n*16`, then calls the row's *enter* callback. State number == animation number. Animation names
(strings at `0x08010F08`): 0 Standing, 1 Walking, 2 Death, 3 Melee1, 4 Melee2, 5 Ki, 6 Excited,
7 Sad, 8 Transform, 9 SSaiyaJin, 10 Monkey, 11 Monster, 12 Raditz, 13 Bubbles, 14 Flying,
15 Bored, 16 Mallet.

**Messages** (broadcast to every entity's `+0x38` handler):
`0x0800283C(msg,arg)` stops at the first handler returning non-zero (entity list scanned from the
end); `0x0800286A` sends to all.
* msg 0 = "A pressed": arg unused; the handler tests the point in front of the player
  (`0x08003446`). Used for punching, talking, picking up items.
* msg 1 = projectile hit test; arg → hit record
  `{s16 x0,y0,x1,y1; s32 damage; s32 enemy_owned; u8 once_per_invuln; s32 owner_char}`.
* msg 2 = script event (arg = actor id): NPC actors turn into enemies (`0x0800573A`).
* msg 6 = an obstacle was destroyed (refresh collision).

### 1.4 Characters ("chars", 0x278 bytes) — HP/Ki/def/exp/team/alive/stun/invuln/aggro/position/speed VERIFIED (watched or poked in the emulator), the rest STATIC
Array starting at `0x02013224` (slot 0 = Goku), bump-allocated (`0x030046B4`) when a map spawns
fighters. `init_char(char, team, template)` = `0x08006D90`.

| off | type | meaning |
|---|---|---|
| +0x00 | u16 | template id (enemy) / 0 (player) |
| +0x02 | u16 | template +0x08 (2 or 3; purpose unknown) |
| +0x04 | ptr | name string (template +0x04) |
| +0x08 | u8 | AI facing |
| +0x09 | u8 | **char state**: 0 idle, 1 walk, 3 move on flying layer, 4 ki attack, 5 melee attack, 6 hurt, 7 dead (state→anim table `0x086B6068`: 0,1,6,14,5,3,0,2) |
| +0x0A | u8 | **stun / cooldown timer** (frames). AI skipped and damage ignored while ≠ 0. Set to 45 after an attack/ki animation ends, 20 after a hurt animation (`0x08005DE0`), 20 by `set_state(6)`, 8 after bumping another enemy |
| +0x0B | u8 | AI activity: 0 awake/deciding, 1 walking a path, 2 dormant, 3 roam-woken |
| +0x0C | u16 | move speed, 1/16 px per frame (`SPEED_TABLE[+0x34]`) |
| +0x0E | u16 | wake radius² in px² = `(96·aggro_tiles²) & 0xFFFF` |
| +0x10 | u16 | collision layer (0 ground, 1 flying) |
| +0x12 | u16 | roams (1 if move_type ∈ 2..5) |
| +0x14 | u16 | **invulnerability timer** (80 after being damaged; enemies only) |
| +0x16 | u16 | decision counter |
| +0x18 | u32 | frame until which the "clink" sound is muted |
| +0x1C | u16 | flee/stuck counter |
| +0x1E | u16 | **aggro timer** (900+rand(900) frames when hit by the player) |
| +0x20/+0x22 | s16 | position px (player: tile snapped) |
| +0x24/+0x26 | s16 | position in 1/16 px (enemy movement integrates here) |
| +0x28..+0x2B | u8 | bbox offset x,y and size w=16,h=8 |
| +0x2C..+0x33 | s16 | bbox rect (enemy–enemy separation) |
| +0x34 | s32 | speed class (template +0x28) |
| +0x3C | u16 | move type (template +0x10) |
| +0x3E | u16 | target type (template +0x12) |
| +0x40 | u16 | fight style (template +0x14) |
| +0x42/+0x44/+0x46/+0x48 | u16 | template +0x16 / group flag / flying / aggro tiles |
| +0x4C | s32 | **HP** |
| +0x50 | s32 | **max HP** |
| +0x54 | s32 | **Ki** |
| +0x58 | s32 | **max Ki** |
| +0x5C | s32 | Ki regen per tick = maxKi >> 6 |
| +0x60 | s32 | **defense** (player: constant 250) |
| +0x64 | s32 | leash radius; +0x68/+0x6C leash home tile; +0x70/+0x74 guard tile |
| +0x78/+0x7C | s32 | current tile x/y (px >> 3) |
| +0x84 | s32 | player: **experience**; enemy: **exp reward** |
| +0x90/+0x98 | | group leader pointer / flag (set when template +0x18 ≥ 2) |
| +0x9A | u16 | team: 1 player, 2 enemy |
| +0x9C | u16 | alive |
| +0xA0/+0xA4 | ptr | list links |
| +0xA8/+0xAC/+0xB0.. | | path index / path length / path waypoints (tile x,y) |
| +0x240..+0x253 | s16[5][2] | patrol waypoints (init −1,1) ; +0x254 patrol index |
| +0x258 | ptr | current target char ; +0x25C last attacker char |
| +0x260..+0x270 | ptr[5] | "assist" list (target type 2) |
| +0x274 | ptr | owning entity |

Lists (`{count, head, tail}`): `0x030047F0` awake list (player first), `0x03003B54` dormant list,
`0x03003B48` stationary list (move type 0). Moving between them: `0x08006294` (add), `0x080062FC` (remove).

### 1.5 Player controller `0x030007AC` — VERIFIED
| off | meaning |
|---|---|
| +0x00 | u8 max flight energy (level table column 3) |
| +0x01 | u8 flight energy |
| +0x04 | u8 Goku sprite variant (scripts) |
| +0x05 | u8 selected ki move 0..2 |
| +0x06 | u16 walk speed = 16 (1/16 px per frame) |
| +0x0C | player entity |
| +0x14/+0x16 | s16 position in 1/16 px |
| +0x18/+0x1C/+0x20/+0x24 | maxKi / Ki / maxHP / HP mirrors (HUD) |
| +0x28 | take-off position (flight) |
| +0x2C | experience mirror |
| +0x30 | **flying flag** (`0x030007DC`) |
| +0x38 | Solar Flare ready frame (`0x030007E4`) |

Save/progress block `0x03000028`: `+0x10` (= `0x03000038`) story-flag bitfield; `0x03000048`
respawn x,y; `0x0300004C` current map; `0x0300004D` learned-moves bitmask (bit1 Kamehameha, bit2
Solar Flare); `0x0300004E + item_id` inventory counts. Main RNG state `0x0300098C/0x03000990`;
`0x03000984` exp mirror; `0x03000988` level index; `0x03000C18` god-mode flag (see §2.4); `0x03000C1C` per-hit damage cap
for enemy attacks.

### 1.6 Maps — VERIFIED (warped into all 42 with `warp.py`/`survey.py`)
`load_map(map, entrance_xy*)` = `0x08001FA8`; map table `0x081CA09C` (42 records of 0x34 bytes).
Content record: music, actors (12 bytes: x, y, sprite id, spawn condition + story flag, kind
0/1 NPC 2 enemy, script id), pickups (8 bytes: x, y, item id), breakable obstacles (8 bytes),
script regions, default entrance. Full decode: `data/log1/maps.json`, `extract_maps.py`.
Enemies are fixed placements (no random encounters). `load_map` resets the char allocator
(`0x08001FF2`: `0x030046B4 = 0x02013224`) and re-spawns every actor, so **all enemies respawn each
time a map is (re)entered** (STATIC; consistent with every warp in the emulator). Bosses are
placed as NPCs and turned into enemies by their dialogue scripts (msg 2 → `0x0800573A`) or by
the "change form" script command `0x0800DB00` (same struct re-initialised with the template of the new
sprite; this is how Frieza's forms and Vegeta→Oozaru map onto templates 26–29 / 6 — INFERRED from
the sprite→template table, the scripts themselves were not traced).
The ROM also contains a developer map-select handler at `0x08000C3E` (up/down chooses 0..41, A
loads) — STATIC, not reached in normal play.

---------------------------------------------------------------------------------------------

## 2. Player: Goku

### 2.1 Stats and growth — VERIFIED
Table `0x080114A8`, 16 bytes per level `{exp, maxHP, maxKi, maxFlight}` (25 levels):

| Lv | EXP | max HP | max Ki | flight | Ki regen/16f | ki blast dmg | punch dmg |
|---|---|---|---|---|---|---|---|
| 1 | 0 | 105 | 100 | 6 | 1 | 35 | 78-104 |
| 2 | 350 | 200 | 150 | 7 | 2 | 66 | 150-199 |
| 3 | 500 | 325 | 200 | 8 | 3 | 108 | 243-324 |
| 4 | 900 | 435 | 300 | 9 | 4 | 145 | 326-434 |
| 5 | 1600 | 600 | 350 | 10 | 5 | 200 | 450-598 |
| 6 | 3200 | 900 | 1000 | 11 | 15 | 300 | 675-898 |
| 7 | 6400 | 1100 | 1000 | 12 | 15 | 366 | 825-1097 |
| 8 | 12800 | 1300 | 1000 | 13 | 15 | 433 | 975-1297 |
| 9 | 25600 | 1500 | 2000 | 14 | 31 | 500 | 1125-1497 |
| 10 | 30400 | 1700 | 5000 | 15 | 78 | 566 | 1275-1696 |
| 11 | 44000 | 2000 | 6000 | 18 | 93 | 666 | 1500-1996 |
| 12 | 100000 | 3000 | 7000 | 21 | 109 | 1000 | 2250-2994 |
| 13 | 140200 | 4000 | 8000 | 25 | 125 | 1333 | 3000-3992 |
| 14 | 180800 | 5000 | 9000 | 30 | 140 | 1666 | 3750-4990 |
| 15 | 257680 | 6000 | 10000 | 33 | 156 | 2000 | 4500-5988 |
| 16 | 355360 | 7000 | 18000 | 35 | 281 | 2333 | 5250-6986 |
| 17 | 410720 | 8000 | 25000 | 40 | 390 | 2666 | 6000-7984 |
| 18 | 500000 | 9000 | 45000 | 45 | 703 | 3000 | 6750-8982 |
| 19 | 630000 | 10000 | 60000 | 50 | 937 | 3333 | 7500-9980 |
| 20 | 800000 | 20000 | 65000 | 55 | 1015 | 6666 | 15000-19960 |
| 21 | 900000 | 40000 | 75000 | 60 | 1171 | 13333 | 30000-39921 |
| 22 | 1000000 | 50000 | 95000 | 70 | 1484 | 16666 | 37500-49902 |
| 23 | 2200000 | 80000 | 110000 | 80 | 1718 | 26666 | 60000-79843 |
| 24 | 3100000 | 100000 | 120000 | 90 | 1875 | 33333 | 75000-99804 |
| 25 | 3600000 | 150000 | 150000 | 99 | 2343 | 50000 | 112500-149707 |

* Level from exp (`0x08005C32`): `i = 0; while (T[i].exp <= exp && T[i].exp != 0) i++; level_idx = i−1`
  (row 0 has exp −1). Max level 25.
* On every kill by the player (`0x08007518`): `exp += victim.exp_reward`; then `0x08005BFE`:
  `maxKi = T.maxKi; maxHP = T.maxHP; HP += newMaxHP − oldMaxHP; kiRegen = maxKi>>6;
  damage_cap = maxHP/3 + 1`. Ki is **not** refilled, HP only gains the max-HP delta. If maxHP grew,
  the "level up" message + sfx 6 (`0x08005BBC`).
  VERIFIED (`test_levelup.py`: exp 340 + wolf 70 → Lv2, HP 80→175/200, maxKi 150, Ki 30→31).
* **Defense is a constant 250** for the player at every level (only writer: `init_char`
  `0x08006E62`; `0x08006EFE` is the enemy branch). STATIC + observed.
* There is no strength/attack stat: player damage is derived from max HP (punch, ki blast) or
  from Ki spent (Kamehameha).

### 2.2 Movement — VERIFIED
* Walk: `controller+6 = 16` sixteenths → **1 px/frame**. Movement code `0x08002CE0` is an
  if/else chain: **only one direction per frame, priority UP > DOWN > LEFT > RIGHT; no diagonals**
  (`test_diag.py`). Collision against the map collision layer 0 with the 16×8 foot box.
* Flight (R, `0x08002EBE`): needs flight energy > 0 and a passable point on the flying collision
  layer in front; state 14 "Flying", speed multiplier 2 → **2 px/frame**, consumes **1 energy per
  8 frames** (`0x08002A88`, `0x08003290`). R again (or energy 0) lands if the ground under the
  player is passable, otherwise Goku flies back to the take-off point. Energy never regenerates;
  feather pickups (items 31/32/33) add 5/10/20 up to the level's max. While flying
  (`0x030007DC = 1`) enemies do not melee or fire ki, and Ki does not regenerate.
  VERIFIED (energy 6→0 in 48 frames, 2 px/frame, feather 1→6).

### 2.3 Controls
A = punch / talk / pick up; B = selected ki move (hold to charge); L = cycle ki move among learned
ones (`0x08003524`, bitmask `0x0300004D`; Ki Blast always available); R = fly/land; START = pause.
Moves are learned by the script command `0x08006216` (`learned |= 1 << id`; command class
`0x08166B28`). Its only two instances: `0x08543DE4` (id 1, Kamehameha — King Kai after the Gregory
mallet training on map 13, text `0x081612E9`) and `0x08544070` (id 2, Solar Flare — reward text
`0x08166468`, the script then warps to map 40). STATIC + text evidence.

### 2.4 Hidden invincibility code — VERIFIED (`test_cheat.py`)
During the Shenron (dragon) intro scene the scene callback `0x080048FA` compares the last six
newly-pressed buttons (ring buffer `0x03003D1C`, checker `0x0800A6B2`) with the u16 list at
`0x08011462`: **UP, DOWN, LEFT, RIGHT, B, A**. On a match it plays sfx 7 and sets `0x03000C18 = 1`.
With the flag set, `apply_damage` never lowers the player's HP (it still returns "damaged"/"killed"
codes for the hurt animation). Entering it in the emulator set the flag to 1.

### 2.5 Options
Pause → Options contains only Text Speed, Music Volume, SFX Volume (screenshot verified): there is
no difficulty setting.

### 2.6 Hurtbox / foot box
Sprite def `goku` `0x085C1884`: foot box 16×8 at the entity position; hurtbox (all facings)
x 2..14, y −22..6 relative to the position (used for enemy projectiles). Enemy melee does not use
boxes (tile distance ≤ 1).

---------------------------------------------------------------------------------------------

## 3. Combat

### 3.1 Punch — VERIFIED (`verify_melee.py` 10/10, `combat_trace.py`)
* A in Standing/Walking → `0x08002E80` broadcasts msg 0. Each enemy handler (`0x08005604`) checks
  whether the **single point in front of the player** (`0x08003446`) lies inside its per-facing
  hurtbox. Point offsets from the player position: down (+8,+8), up (+8,−8), left (−2,−8),
  right (+18,−8). The first entity that accepts takes the hit (no multi-hit).
* Damage: `dmg = (maxHP · (384 + rand2(128))) >> 9` (75.0 %–99.8 % of max HP; secondary RNG).
* The player plays Melee1 or Melee2 (`frame & 1`), **16 frames** (`0x08002A52`), sfx 2, cannot
  act meanwhile. The hit is resolved on the press frame (startup 0, active 1 frame, recovery 16).
  If nothing accepted the msg, the same animation plays as a whiff.
* No combo system: the two punch animations alternate randomly; damage is identical.

### 3.2 Ki moves — VERIFIED (`test_ki.py`) unless noted
Creation: `fire_move(player_entity, move)` `0x0800B3DC` spawns a projectile entity with state
table `0x08011E44[move]`, sprite `0x08011E38[move]`; player enters state 5 (Ki pose) and stays in
it until 16 frames after release (`0x08002B9A`).

**Charging** (`0x0800B436`): while B is held and the owner is in state 5, on every frame with
`frame & 7 == 0`: `cost = (maxKi + 15) >> 4`; if `Ki >= cost` then `Ki −= cost; charge += cost`.
A tap therefore costs **0 Ki**.

| move | damage | details |
|---|---|---|
| 0 Ki Blast | `maxHP / 3` (independent of charge) | speed 3 px/frame along facing; lifetime = 10 + Ki spent frames (`0x0800B5BA`); size 6×6, 8×8 (charge+10 > 20) or 16×16 (> 72); stops at walls or on the first entity hit (`0x0800283C`); destroys breakable obstacles |
| 1 Kamehameha | **Ki spent** while charging (0 if tapped) | beam 8×8 that grows 3 px/frame toward the facing for 16 frames (max 56 px) or until it reaches a wall; every growth frame tests all entities (`0x0800286A`) but an enemy with invulnerability > 0 ignores it → one hit per enemy; sfx 0x0C if 2·charge ≥ maxKi else 0x08; then 8-frame retract |
| 2 Solar Flare | 0 | freezes **all enemy AI** for `60 + min(charge,maxKi)·300/maxKi` frames (60..360) via global timer `0x03003B34`, sets awake ground enemies to idle; next Solar Flare allowed at `release + duration + 60` (VERIFIED: charge 125/400 → 153 frames) |

Ki blast numbers verified: 200 dmg at maxHP 600 for a tap and a 60-frame charge; Kamehameha 175 dmg
after 7 charge ticks of 25. Tap ki blast flew exactly 10 frames.

### 3.3 Hit resolution (`hit()` = `0x08007416`) — VERIFIED
`hit(attacker, defender, flag, raw)`:
1. defender invulnerability `char+0x14 ≠ 0` → "clink" (sfx 0x1D, at most every 20 frames), stop.
2. attacker or defender dead → stop.
3. attacker is an **enemy**: if `flag == 0` (melee) attacker → state 5; `raw = min(raw, cap)`,
   `cap = player_maxHP/3 + 1` (`0x03000C1C`, refreshed by `0x08006D70`). ⇒ the player can never lose
   more than ⅓ of max HP (+1) per enemy hit. attacker is the **player**: defender aggro timer
   `char+0x1E = 900 + rand(900)`.
4. `defender.+0x25C = attacker` (last attacker), `attacker.+0x258 = defender` (target).
5. `apply_damage` (below). Result 2 → hurt (sfx 0x16/0x1C enemy, 0x14 player); enemy hurt:
   **stun 20 frames, invulnerable 80 frames**; player hurt: flash only, state forced back to
   Standing (cancels a charge — VERIFIED `test_interrupt.py`: only the damaging hit broke the Ki
   pose, 0-damage hits did not) — **no player invulnerability**. The cap value is recomputed only
   when stats are refreshed (init / level-up), not when max HP is poked. Result 1 → death (state 7, sfx 0x17),
   exp/level-up if the killer is the player. Result 0 → clink.
* **No knockback**, no hit-stun movement for anyone (hurt only changes state/timers). **No block.**

### 3.4 Defense: `apply_damage(att, def, flag, dmg)` = `0x08007352` — VERIFIED 113/113 (`verify_damage.py`)
```
if defender.team != 1:
    if defender.stun (+0x0A) != 0:              return 0
    if defender.+0x0B == 2 and defense != 0:     return 0     # dormant enemies
if defense != 0:
    r = rand(10000)                              # 0..10000 inclusive
    if   r > 9000: dmg = (defense > dmg)    ? 0 : dmg - defense
    elif r > 7500: dmg = (defense/2 > dmg)  ? 0 : dmg - defense/2
    elif r > 5000: dmg = (defense/4 > dmg)  ? 0 : dmg - defense/4
    elif r > 4800: return 0                      # 2 % "miss"
    # r <= 4800: full damage (48 %)
if dmg >= hp: hp = 0; alive = 0; return 1          # (god-mode 0x03000C18 protects the player)
if dmg == 0: return 0
hp -= dmg; return 2
```
All compares are signed. Annotated disassembly:
```
0800737A ldr  r0,[r5,#0x60]      ; defense
08007384 ldr  r0,=10000 ; bl rand ; r0 = 0..10000
0800738A ldr  r1,=9000  ; cmp r0,r1 ; ble ...
08007390 ldr  r0,[r5,#0x60] ; movs r1,#0 ; cmp r0,r4 ; bgt keep0 ; subs r1,r4,r0   ; dmg-def
0800739E ldr  r1,=7500  ... asrs r0,r0,#1 (def/2) ...
080073B4 ldr  r1,=5000  ... asrs r0,r0,#2 (def/4) ...
080073CA movs r1,#0x4B ; lsls r1,#6 (4800) ; cmp ; ble full ; movs r0,#0 ; return
080073D6 ldr  r0,[r5,#0x4C] ; cmp r4,r0 ; blt damaged       ; dmg >= hp -> kill
```
Expected damage for `dmg ≫ def`: `0.48·d + 0.25·(d−def/4) + 0.15·(d−def/2) + 0.10·(d−def)` ≈
`0.98·d − 0.2125·def`.

### 3.5 Death / game over
Enemy death: char state 7 → removed from the AI lists (dead chars go to list `0x03003B48`), entity
plays the Death animation (enter `0x08005E12` notifies scripts `0x08005478(1, actor_id)`; after 64
frames `0x0800293C` sends `0x08005478(2, actor_id)`). **No item drops.**
Player death: state 7 → game-over sequence (angel Goku screen) — observed in the emulator.

### 3.6 Status effects
Only timers on the char struct: stun (+0x0A), invulnerability (+0x14), aggro (+0x1E), the global
Solar-Flare freeze (`0x03003B34`) and flight. No poison/burn/etc.

---------------------------------------------------------------------------------------------

## 4. Enemy attacks — VERIFIED (`measure_attacks.py`, henchman runs)
Decided in `0x0800F774(self, target, d)` where `d` = Manhattan distance in 8-px tiles
(`0x0800F570`, `|dx|+|dy|` of `char+0x78/+0x7C`).
* **Melee** (`d ≤ 1`, player not flying): face the target, `raw = hp/4 + rand(hp/2)` using the
  attacker's **current** HP; then `hit(self, target, 0, raw)` → attacker plays Melee1 (16 frames) then
  a 45-frame cooldown. Observed wolf (350 HP) damage 105–251 (formula 87–262), interval 69–77
  frames when glued to the player.
  **Bug**: `rand(n)` computes `asr(int32(prod·(n+1)), 16)`; for `n ≥ 32768` the product can exceed
  2³¹ and the result is negative. Enemies with HP ≥ 65536 (henchmen 4–6, Ginyu force, Frieza) can
  roll negative damage; on a "full damage" defense roll this **heals** the player (observed:
  henchman_5 dealt −13557).
* **Ki blast** (`d > 1`): requires `ki > maxKi/16`, the target roughly in line (the attacker's
  point lies within the target's x-band `[tx − w/2, tx + 1.5w)` or y-band), `rand(100) ≤ 1`
  (2/101 per decision) and the player not flying. `power = rand(ki/2) + ki/4`; the projectile
  (`0x0800C010`, table `0x0801206C`) charges like the player's (`(power+7)>>3` per 8 frames, ~60
  frames) and then flies at 3 px/frame until it leaves the screen. **Damage = owner maxHP/3**
  (fly-enter `0x0800B554` overwrites the power) capped at player maxHP/3+1. Vegeta (name starting
  with 'v') uses sprite `0x086B3400`. Observed 26666 (maxHP 80000) and 5000 (maxHP 15000).
  (With Ki ≥ 65536 the same rand bug can make `power` negative; the charge loop then ends
  immediately and the blast fires without spending Ki — STATIC.)
* Otherwise: path toward the target (`0x0800F914` / `0x08006F1A` flow-field pathfinding on 8-px
  tiles, layer = char+0x10).

---------------------------------------------------------------------------------------------

## 5. Enemy AI (generic, all enemies & bosses) — STATIC, key parts VERIFIED
AI tick `0x08008018` (every frame):
1. If the Solar-Flare timer `0x03003B34` ≠ 0: decrement and skip everything else.
2. Every 30..90 frames (`rand(60)+30`, `0x03003B30`) or whenever the player enters a new tile,
   the "player moved" flag is set and the player flow field (`0x0800F478`) is rebuilt.
3. **Dormant list** (`0x03003B54`), only when the flag is set: an enemy wakes (+0x0B=1, moved to
   the awake list) if `dist²(player, enemy) < wake_r²` (`char+0x0E = 96·aggro_tiles²`, i.e.
   radius ≈ 9.8·aggro_tiles px) **or** its aggro timer is running, **and** it is within 144 px
   (0x5100 = 144²) of the screen centre. VERIFIED (`test_wake.py`, wolf radius² 9600: wakes at
   dist² 9216, stays asleep at 9604). Roaming enemies (move type 2–5) that are on screen may
   also wake spontaneously: `rand(65535) < 200` per check while fewer than 4 chars are awake
   (+0x0B=3).
4. **Awake list** (`0x030047F0`): per enemy: decrement aggro timer (while it runs, each frame a 2 %
   chance `rand(10000) > 9800` to forget its target; when it expires non-aggressive types forget
   immediately), decrement invulnerability, decrement stun (when it reaches 0 the enemy resumes
   walking: state 1 ground / 3 flying). Skipped while state > 3. When the player-moved flag is set
   and the enemy is neither in range (wake radius or aggro timer) nor within 144 px of the screen
   centre it goes back to sleep (+0x0B=2, idle). Ki regen `+maxKi>>6` every 16 frames. Then either
   continue its path or run the **decision** `0x0800755A`. Overlapping awake enemies: the slower one
   (`+0x0C`) stops and gets an 8-frame wait.

Decision `0x0800755A` (target = last attacker `+0x25C`, else current target `+0x258`):
* **With a target**: target type 4 → flee (`0x0800F9AC`). Otherwise by **fight style** (+0x40):
  0 → attack; 1 → flee while `tHP + tHP/8 ≥ tMaxHP` (target nearly unhurt), else attack;
  2 → attack only if own HP > target HP, else flee; 3 → if own HP < maxHP/2 then `rand(100) ≤ 5`
  → flee, else attack. "Attack" = §4.
* **Without a target**: the player becomes the target if Manhattan tile distance ≤ `aggro_tiles`.
  Then by **target type** (+0x3E): 1 aggressive → attack; 2 assist → attack only if the player is
  in its 5-slot list `+0x260` (filled by scripts — INFERRED), else flee; 3 opportunist → attack only
  if the player already has an attacker (`player+0x25C ≠ 0`), else flee; 0/4 → flee.
* **No target in range** and path finished → **move type** (+0x3C, jump table `0x08007758`):
  0/1/2 → walk back to guard tile (+0x70/+0x74, templates use (0,0) ⇒ effectively stand still);
  3 → patrol up to 5 waypoints (+0x240) else wander; 4 → random wander within
  `min(aggro_tiles,14)` tiles (`0x0800F6A0`); 5 → wander but return home when Manhattan distance
  from leash home ≥ leash radius.
* Flee (`0x0800F9AC`): pick a tile on the far side of the threat (mirrored offset, else sideways).

Enemy movement speed = `SPEED_TABLE[class]` sixteenths of a pixel per frame (`0x080116A4`:
8, 10, 12, 14, 16, 18, 19, 20). VERIFIED for the wolf (19/16 px/frame measured).

---------------------------------------------------------------------------------------------

## 6. Enemy roster — `data/log1/enemies.json` (templates `0x086B6094`, 0x40 bytes)

| id | template name | sprites | HP | Ki | Def | EXP | speed (1/16 px/f) | aggro tiles | move/target/fight | flying | ROM |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | squirrel | squirrel | 1 | 0 | 0 | 5 | 18 | 2 | 4/4/0 | 0 | 0x086B6094 |
| 1 | snake | snake | 3 | 0 | 0 | 5 | 8 | 4 | 4/3/2 | 0 | 0x086B60D4 |
| 2 | wolf | wolf | 350 | 0 | 200 | 70 | 19 | 10 | 3/1/3 | 0 | 0x086B6114 |
| 3 | piccolo | piccolo | 100 | 600 | 50 | 0 | 19 | 8 | 1/0/0 | 0 | 0x086B6154 |
| 4 | raditz | raditz | 8000 | 1350 | 700 | 0 | 16 | 10 | 1/1/0 | 0 | 0x086B6194 |
| 5 | nappa | nappa | 14000 | 500 | 3000 | 0 | 16 | 10 | 1/1/0 | 0 | 0x086B61D4 |
| 6 | strong trex | oozaru1, oozaru2, trex | 18000 | 0 | 600 | 4000 | 10 | 10 | 1/1/0 | 0 | 0x086B6214 |
| 7 | trex | trex2 | 7000 | 0 | 300 | 600 | 10 | 10 | 1/1/0 | 0 | 0x086B6254 |
| 8 | pterodactyl_1 | ptero1 | 2000 | 0 | 200 | 300 | 12 | 8 | 4/1/0 | 1 | 0x086B6294 |
| 9 | pterodactyl_2 | ptero2, ptero3 | 8000 | 0 | 400 | 2000 | 12 | 8 | 2/0/0 | 1 | 0x086B62D4 |
| 10 | robber_1 | robber1, robber3 | 7000 | 0 | 100 | 1000 | 12 | 8 | 2/0/0 | 0 | 0x086B6314 |
| 11 | robber_2 | robber2 | 7000 | 0 | 100 | 1000 | 12 | 8 | 2/0/0 | 0 | 0x086B6354 |
| 12 | devil_1 | devil1 | 500 | 0 | 0 | 100 | 16 | 8 | 1/4/0 | 0 | 0x086B6394 |
| 13 | devil_2 | devil2 | 2147483647 | 0 | 0 | 1 | 20 | 8 | 1/4/0 | 0 | 0x086B63D4 |
| 14 | crab | crab | 240 | 0 | 0 | 70 | 12 | 2 | 3/1/0 | 0 | 0x086B6414 |
| 15 | bubbles | bubbles | 1 | 0 | 0 | 0 | 19 | 10 | 4/4/0 | 0 | 0x086B6454 |
| 16 | gregory | gregory | 1 | 0 | 0 | 0 | 20 | 10 | 4/4/0 | 0 | 0x086B6494 |
| 17 | burter | ginyu | 150000 | 77000 | 10000 | 0 | 16 | 10 | 1/1/0 | 0 | 0x086B64D4 |
| 18 | reccome | recoome | 60000 | 45000 | 7000 | 0 | 16 | 10 | 1/1/0 | 0 | 0x086B6514 |
| 19 | jeice | burter, jeice | 90000 | 76000 | 9000 | 0 | 16 | 10 | 1/1/0 | 0 | 0x086B6554 |
| 20 | henchman_1 | hench1 | 10000 | 16500 | 100 | 10000 | 16 | 8 | 1/1/3 | 0 | 0x086B6594 |
| 21 | henchman_2 | hench2, hench3 | 10000 | 18500 | 100 | 10000 | 16 | 8 | 1/1/1 | 0 | 0x086B65D4 |
| 22 | henchman_3 | hench4, hench5 | 15000 | 12000 | 100 | 10000 | 18 | 8 | 4/1/3 | 0 | 0x086B6614 |
| 23 | henchman_4 | hench6 | 70000 | 40000 | 100 | 30000 | 18 | 8 | 1/1/0 | 0 | 0x086B6654 |
| 24 | henchman_5 | hench7 | 80000 | 40000 | 100 | 30000 | 18 | 8 | 1/1/0 | 0 | 0x086B6694 |
| 25 | henchman_6 | hench8 | 70000 | 40000 | 5000 | 30000 | 18 | 8 | 1/1/0 | 0 | 0x086B66D4 |
| 26 | frieza_1 | frieza1 | 300000 | 150000 | 41000 | 300000 | 16 | 10 | 1/1/0 | 0 | 0x086B6714 |
| 27 | frieza_2 | frieza2 | 400000 | 300000 | 45000 | 500000 | 16 | 10 | 1/1/0 | 0 | 0x086B6754 |
| 28 | frieza_3 | frieza3 | 600000 | 700000 | 100000 | 800000 | 18 | 10 | 1/1/0 | 0 | 0x086B6794 |
| 29 | frieza_4 | frieza4 | 1000000 | 1200000 | 10000 | 0 | 19 | 10 | 1/1/0 | 0 | 0x086B67D4 |
| 30 | snakequeen | snkqueen | 3000 | 0 | 350 | 0 | 12 | 8 | 4/1/0 | 0 | 0x086B6814 |
| 31 | spirit | spirit4, spirit5 | 4000 | 0 | 400 | 1000 | 10 | 4 | 4/0/0 | 0 | 0x086B6854 |
| 32 | vegeta | vegeta1 | 16000 | 80000 | 1000 | 0 | 16 | 10 | 4/1/0 | 0 | 0x086B6894 |
| 33 | vegeta | vegeta2 | 20000 | 100000 | 3500 | 0 | 18 | 10 | 4/1/0 | 0 | 0x086B68D4 |

Columns are raw template values (move/target/fight = +0x10/+0x12/+0x14). Template names are the
developer labels stored in the ROM; the sprite→template table (`0x0815A69C`) disagrees with them in
places (sprite `burter` → template "jeice", `ginyu` → "burter", `trex`/`oozaru` → "strong trex").
Per-enemy hurtboxes, animations present, sprite-def/anim-table/palette addresses (for matching the
graphics extraction) and the maps each enemy appears on are in `enemies.json`.

### 6.1 Template format (0x40 bytes) — STATIC (init_char `0x08006E68..0x08006F16`)
| off | size | → char | meaning |
|---|---|---|---|
| 0x00 | u32 | | id |
| 0x04 | ptr | +0x04 | name |
| 0x08 | u32 | +0x02 | 2 or 3 (unknown, not read by combat code) |
| 0x0C | s32 | +0x84 | exp reward |
| 0x10 | s16 | +0x3C | move type (0..5, §5) |
| 0x12 | s16 | +0x3E | target type (0..4) |
| 0x14 | s16 | +0x40 | fight style (0..3) |
| 0x16 | s16 | +0x42 | unused by the generic AI (always 0) |
| 0x18 | s16 | +0x44 | ≥2 → group leader (+0x90 = self, +0x98 = 1) |
| 0x1A | s16 | +0x46 | 1 = flying collision layer (pterodactyls) |
| 0x1C | s16 | +0x48 | aggro / wander radius in tiles; wake radius² = 96·v² |
| 0x1E/0x20 | s16 | +0x68/+0x6C | leash home tile (move type 5) |
| 0x22 | s16 | +0x64 | leash radius (tiles, Manhattan) |
| 0x24/0x26 | s16 | +0x70/+0x74 | guard tile (move type 0–2) |
| 0x28 | s16 | +0x34 | speed class → `SPEED_TABLE` |
| 0x2A/0x2C | s16 | +0x78/+0x7C | initial tile (overwritten at spawn) |
| 0x2E/0x30 | s16 | +0x80/+0x82 | (all 0) |
| 0x34 | s32 | +0x54,+0x58 | Ki = max Ki |
| 0x38 | s32 | +0x4C,+0x50 | HP = max HP (0 → 1) |
| 0x3C | s32 | +0x60 | defense |
Derived at spawn: kiRegen = maxKi>>6, speed, wake radius, roams flag, collision layer.

### 6.2 Behaviour summary per enemy (from the parameters above + §5)
* **Squirrel / Bubbles / Gregory** (target type 4): wander and always run away; 1 HP. Bubbles and
  Gregory are King Kai's training targets (map 13).
* **Snake**: wanders; only engages if the player is already fighting something, and only if its HP
  (3) exceeds the player's — in practice it flees. 5 exp.
* **Wolf**: patrols/wanders, aggressive within 10 tiles (wakes within ~98 px), fight style 3 (may
  flee below half HP), fast (19/16 px/f), def 200 — a level-1 punch (78–104) only lands on the 48 %
  full-damage roll.
* **Crab**: aggressive but tiny aggro radius (2 tiles, wake ~19.6 px), patrols.
* **Pterodactyls**: fly (ignore ground collision); ptero_1 wanders + aggressive; ptero_2 assist-type
  (passive unless scripted).
* **Robbers** (map 17): guard + assist type, i.e. passive until a script adds Goku to their list.
* **T-rex** (sprite "trex" = template 6, 18000 HP, def 600; "trex2" = template 7, 7000 HP): slow
  (10/16 px/f) guards, aggressive.
* **Henchmen** (Frieza soldiers, maps 24–34): 1/1/x guards; henchman_1/3 may flee when hurt
  (style 3), henchman_2 waits until the target is hurt (style 1). They are the only common enemies
  with Ki (ki blasts). henchman_4..6 trigger the negative-damage bug.
* **Spirits** (map 12, Hell): wander, never target the player on their own (type 0 → flee when
  approached; fight back when attacked).
* **Devils** (map 12): flee; devil_2 has HP 0x7FFFFFFF (unkillable).
* **Bosses** — same AI, aggressive guards with fight style 0 (always attack). VERIFIED for Raditz:
  talking to the Raditz NPC on map 9 runs his script, which re-initialises the actor as template 4
  (8000 HP, 1350 Ki, def 700) with the generic enemy handler `0x08005605`; over 4000 frames glued to
  Goku he only used melee (54 hits, raw 2082–5891 = 2000 + rand(4000), one every ~74 frames).
  Bosses: Raditz (map 9),
  Nappa and Vegeta (map 23, Vegeta 32 then form 33), Ginyu force (map 30), Frieza forms 1–4
  (maps 40/41), Princess Snake (map 11). Their big Ki pools mean frequent ki blasts are possible
  but still only at 2 % per aligned decision. Fight start, transformations and endings are scripted
  (not traced). Exp reward 0 for most story bosses (Frieza forms 1–3 give 300k/500k/800k).

---------------------------------------------------------------------------------------------

## 7. Items — `data/log1/items.json` (table `0x085C224C`, 24 bytes)
* Pickups are fixed map objects; press A in front (msg 0, `0x0800D03C`). No drops.
* Herbs (ids 11–13): heal `maxHP/4` (`0x0800170C`, refused at full HP); carry limit: total herbs < 6
  to pick more. Senzu bean (19): full heal (`0x08001730`), carry limit 3. Feathers (31/32/33):
  instant flight energy +5/+10/+20 (`0x080032A2`, VERIFIED). Everything else (flowers, stones,
  saplings, fruit, repair kit, capsules, artifacts, egg, boat, magazines) is a quest item with no
  use effect.

## 8. RNG — `data/log1/rng.json`, `model.py`
Main RNG `0x08005CA8`, state `a=0x0300098C, b=0x03000990` (zero at boot, never seeded):
```
a' = (a + 0x02CBBC71) ^ b
b' = (rotl(b,3) + 0x0632D80F) ^ a'
rand(n) = asr32(int32(((a'*b') & 0xFFFF) * (n+1)), 16)     # [0,n]; negative if product >= 2^31
```
Secondary RNG `0x0800A6F4` (state `0x03003D24/28`, same recurrence, output `a'*b'`), advanced at least
once per frame; `rand2(n) = (((a'*b')>>16)*n)>>16` ∈ [0,n−1]; used for punch damage.
Main RNG users: defense roll, enemy melee/ki rolls, aggro timer, path timer, wander/flee choices,
wake roll. VERIFIED (all damage verifications reproduce the emulator using these models).

## 9. Move / projectile data format ("move_format")
A move in LoG1 is *code + small tables*; there is no generic move bytecode. To author a new
projectile move you need:

1. **State table** (array of 0x14-byte rows, one per projectile state; e.g. Ki Blast `0x081C7B14`):
   `+0 enter(entity)`, `+4 update(entity)` (logic, every frame), `+8 anim_tick(entity)` (every
   frame: advance `+0x10`/`+0x12`, end the state), `+0xC visible(entity)` (returns 1 if on screen,
   default `0x0800AF81`), `+0x10 draw(entity, oam_cursor)` (default `0x0800B01F`). Thumb pointers (|1).
   Player moves use 3 states: 0 charge (enter `0x0800B3FC` places the projectile at
   `owner.pos + spawn[facing]`), 1 travel, 2 vanish/explode.
2. **Sprite definition** (0x2C bytes; e.g. `0x086B3AA8`): `+0 u16 w, +2 u16 h` (hitbox size for
   projectiles / foot box for fighters), `+4..+0x23` four per-facing hurtboxes `{s16 x0,y0,x1,y1}`
   (down, up, left, right), `+0x24` palette pointer, `+0x28` animation table pointer.
3. **Animation table**: 16 bytes per state = 4 pointers (down, up, left, right) to a **frame
   descriptor** `{s16 dx, s16 dy, u16 w, u16 h, u8 tiles_per_frame, u8 attr1 bits (bit3 = h-flip),
   u8 OAM shape, u8 OAM size, u32 4bpp tile data}`; frame *f* uses
   `tiles + f·tiles_per_frame·32` (`0x0800B0C4`).
4. **Per-facing parameter tables** read by the callbacks:
   spawn `0x08011E50` `{dx,dy,flags}`, ki-blast launch `0x08011E68` `{dx,dy,vx,vy}`, Kamehameha
   grow `0x08011E88` `{dx,dy,dw,dh}` (+0x20 retract, +0x40 wall retract, +0x60 start offset).
5. **Hit delivery**: in `update`, build the msg-1 record
   `{rect, damage, enemy_owned, once_flag, owner_char}` and broadcast with `0x0800283C` (first
   target, ends the projectile) or `0x0800286A` (all targets); test walls with
   `0x08004FFA(map, rect, layer=2)` (returns non-zero if passable).
6. Register it: `fire_move` indexes `0x08011E38` (sprite) / `0x08011E44` (state table) by move id;
   names at `0x08010000`; L cycles ids 0–2 (`0x08003524`, `cmp #3`).
Damage/charge semantics to copy: charge via `0x0800B436(projectile, cost)`; damage formulas §3.2.
A JSON description of the three moves with all table values is in `moves.json`.

## 10. Open gaps
* Story scripts were only sampled: they are lists of 8-byte commands `{command-class ptr, u32 arg}`
  (some commands take extra words, e.g. warp `0x08166B08` {map, xy}); command classes live around
  `0x08166AC0..0x08166B90`. Triggers, flag setting, boss transformations and the "assist" lists of
  robbers/pterodactyl_2 were not decoded; boss behaviour beyond the generic AI is therefore not
  documented.
* The flow-field pathfinder (`0x0800F478`, `0x08006F1A`, `0x08007A4E`) is described only at a high
  level.
* Template fields +0x08, +0x16, +0x2E/+0x30 have no observed effect in combat code.
* Sound ids are raw numbers (`0x0800ACFE(id)`), not mapped to names.
* ssgoku / gokumallet variants: stat effects (if any) not found — INFERRED none.

## 11. Reproduce
`.venv/bin/python re/log1/extract_all.py` (runs everything), or individually `extract_maps.py` (first), then `extract_enemies.py`, `extract_levels.py`,
`extract_moves.py`, `extract_items.py`, `extract_characters.py`, `extract_formulas.py` → `data/log1/`.
Verification scripts: `verify_damage.py`, `verify_melee.py`, `test_ki.py`, `test_levelup.py`,
`test_diag.py`, `test_wake.py`, `test_cheat.py`, `test_interrupt.py`, `test_raditz.py`, `log_boss.py`, `measure_attacks.py`, `combat_trace.py`; formula reference implementation `model.py`. Tools: `warp.py`
(load any map at any position by hijacking `load_map`), `survey.py`, `drive.py`, `strip.py`,
`watch.py`, `ents.py`, `game.py`, `fulldis.py`/`pdis.py` (disassembly listing in
`work/log1_dis.txt`), `xref.py`.

Save states (`work/states/`): `log1_kamehouse_start.ss` (first controllable frame),
`log1_wolf.ss` (map 2 next to a wolf), `log1_raditz.ss` (map 9, next to the Raditz NPC),
`log1_raditz_fight.ss` (Raditz fight just started), `log1_crab.ss` (map 2 crabs), `log1_item.ss` (map 2 next to a
feather), `log1_hench.ss` (map 34 Frieza henchmen), `log1_maps/log1_mapNN.ss` (every map, default
entrance).
