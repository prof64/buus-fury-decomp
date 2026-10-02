# Buu's Fury lockstep harness: the simulator's player against the emulator, frame by frame

The harness runs the same scenario in the real game (headless mGBA, `tools/emu`) and in the battle simulator and
compares every game tick: position, facing, the animation actually shown (raw index, frame index and the ROM frame
record), the player's state object, HP, EP, form, the RNG state and every sound call (PlaySfx / StopSfx / PlaySong);
in the enemy scenarios also the enemy's position, facing and HP and every ComputeDamage result. It reports the first
frame where they differ. Lag frames (video frames on which the game logic did not run: the frame counter
`0x03000F94` did not advance - frequent in the forest) are recorded and skipped by the simulator (no step, no input).

```
.venv/bin/python tools/battle/lockstep/record.py [NAME_GLOB ...] [--print]   # emulator -> data/battle/buu_lockstep/fNN.json
cd site && npx vitest run tests/battle/buu_lockstep.test.ts                     # replay every trace in the simulator
cd site && LOCKSTEP='f02_skill15' npx vitest run tests/battle/buu_lockstep.test.ts   # one scenario (regex), with a report
LOCKSTEP_CTX=10 / LOCKSTEP_BRIEF=1                                              # more context rows / one line per scenario
```

A report shows the rows before the first divergence, emulator (`emu`) above simulator (`sim`):

```
f02_skill15: first divergence at frame 262 (sfx); input: keys 0x0 (segment frames 237..356)
    261 emu idle  raw  1 f  0 dir 0 pos 160.00,150.00 hp 900 ep 856 form 2 rom 850c13c rng e2e5bbb5
        sim idle  raw  1 f  0 dir 0 pos 160.00,150.00 hp 900 ep 856 form 2 rom 850c13c rng e2e5bbb5
>>  262 emu idle  raw  1 f  0 ... stop:32 play:33:64
>>      sim idle  raw  1 f  0 ...
```

## Files

| file | what |
|---|---|
| `tools/battle/lockstep/emu_lockstep.py` | emulator side: scenario setup, per-frame recording, sound breakpoints, injected hits, `STATE_NAMES` (state object vtable -> name) |
| `tools/battle/lockstep/scenarios.py` | the scenarios (generic per form + skills + extras); add new ones here |
| `tools/battle/lockstep/record.py` | runs scenarios, writes `data/battle/buu_lockstep/fNN.json` (merges by name) |
| `site/tests/battle/buu_lockstep.ts` | simulator side: trace loading, sim setup from the trace, replay, compare, report |
| `site/tests/battle/buu_lockstep.test.ts` | one test per trace; `KNOWN` = understood, unmodelled divergences (skipped with the reason) |
| `data/battle/buu_anim_timing.json` | `tools/battle/gen_buu_anim_timing.py`: SetAnim id -> raw / state / tick / shift / end (ROM tables) |
| `data/battle/buu_form_names.json` | `tools/battle/gen_buu_names.py`: the game's names of the forms (Dojo roster strings) |

## Scenarios

A scenario is a dict (`scenarios.py`): `name`, `form` (0..53), `input` = `[[keys, frames], ...]` (`'A+RIGHT'`, `''` =
none), optional `hits` = attack messages injected at the start of a frame (`{at, dir, atk, mult, str, pow}`),
`skills` (the record's skill list, slot 0 selected), `skill_level`, `hp` / `ep` / `maxep` / `ss` / `ss3`, `stats`,
`pos`, `dir`, `arena` (`htc` = Hyperbolic Time Chamber 6-9, empty floor; `crater` = Giant Crater 8-71, has cliffs;
`forest` = Southwest Forest 8-3), `enemy` = `{type, at: [dx, dy], hp, aggro, dummy}`.

**Enemy scenarios** (`*_vs_*`, Southwest Forest, player at (100, 192) facing right): the area's first enemy spawn record
(ROM `0x0807ACBC`) is patched to the enemy at the player + `at` (the other three records are parked at 0x7000). By
default the enemy is a **target dummy**: a Majin Fighter (0x44) whose aggro test (state 0x14B IfPlayerWithin2 r2, ROM
`0x087043F8 + 0xC`) is patched to -1 and whose on-hit tree (state table `0x0870F514` entry 0x14B, aux pointer) is the
ApplyDamage node `0x086FDA00` alone: hits only take HP, no reaction, no AI change, no RNG of its own. The simulator
patches its decoded AI the same way while the trace runs (`patchScenario`). This isolates the player's attack objects:
every message they send, its box, timing, multiplier and attack type show up as the enemy's HP per frame, the damage
log and the RNG. `real: True` scenarios (`f02_vs_enemy_*`) use unpatched enemies that wait far away during the setup
and are placed at frame 0 (their animation and creation stamps copied).

Setup (emulator): `buu_snakeway_start.ss`, the character / form / stats / skills written into the record, warp
(`re/buu/warp.py`) to the arena, position and facing written, 30 idle frames. Frame 0 values the simulator copies:
record (HP, EP, gauges, skills, levels, equipment), the RNG state, the animation (raw, frame, tick), the form
controller's accumulators (EP regen `+0xC`, SS drain `+0x14` of the object at player+0x22C), the SS cooldown, the
idle state's frame counter (`0x03002B48`), the camera's 22.10 position (`[0x080707E8]`) and the HUD's low-HP beep
timestamp. Injected hits call the player's message handler `0x08010DDE(player, 0, msg)` through
`re/buu/emutools.call` at the start of the frame, with a dummy attacker (STR/POW 30, a vtable of `bx lr`); the
simulator broadcasts the same message from an entity that the world updates first.

The generic set per form: `move` (walk in 4 + diagonal directions, dash in 4 directions incl. a side-step while
dashing, double tap + A), `punch` (taps, punch while walking, A held, turn + punch), `guard` (block, energy shield,
fade-out, B during block), `hurt` (hit from the front / back, blocked, i-frames, hit while walking), `bump` (dash into
the crater's cliff), `ko` (HP 1, one hit, 300 frames). Per skill of the character: tap / hold 130 / walk-right + hold
60; Super Saiyan: transform and revert, `ssdrain` (maxEP 40: the gauge runs out). Extras: Saiyaman, SS3, skill levels
1 / 2, Energy Blast refire, beam until 0 EP, low HP beep, 1300 idle frames (bored animation).

Trace file: `{source, form, traces: [{name, scenario, input, init, states, rows, sfx, erows?, lag?}]}`; `rows` are
run-length encoded `[n, x, y, dx, dy, dir, raw, frame, state, hp, ep, form, rom, rng]` (n frames, position advancing by
dx, dy); `sfx` = `[frame, 'play', id, vol] | [frame, 'stop', id] | [frame, 'song', id] | [frame, 'dmg', target,
damage]` (ComputeDamage `0x0800EABE`: target 0 player, 1 the enemy); `erows` = `[n, x, y, dx, dy, dir, hp<<10]`;
`lag` = the lag frames. `init` also holds the hittable flag (+0x1CC), the cutscene fusion's countdown (form object
+0x10) and the fusion partner's record.

Comparison rules: a sprite without the animation (the game reads a null or foreign def slot: raw -1 / 0 or no frame
record) only compares state / position / timing; when the def was swapped under a running animation (the KO's death
script) the ROM frame record decides.

## What the game does (decoded, verified by the traces)

**Animation model** (`site/src/battle/rulesets/buu/panim.ts`). No animation scripts: the actor shows the animation
struct at obj+0x50 (def+0x50 + 4·raw) at frame obj+0x11; its state object (obj+0x14) advances it. Per frame the actor
update (`0x0800EF08`) runs the form controller, then the state's tick (vtable +4), then the control (+8) of whatever
state is current after the tick (so a state that ends in its tick hands the same frame's input to the next state).
SetAnim(id) (`0x0800ECF0`) = raw `0x08072670[id] + 1` + state `0x0807273C[id]`:

| id | raw | state | tick | frames shown | end |
|---|---|---|---|---|---|
| 0 | 1 idle | form idle `0x03002B44` | idle: enter tick = rand(255); 2-frame anims blink (frame 1 while (tick & 255) > 240) | - | - |
| 1 | 2 walk | `0x03002B4C` | loop | 8 | - |
| 2 | 3 dash | `0x03002B3C` | loop, sound 25 (vol 32) when tick & 15 == 0 | 8 | - |
| 3, 9 | 4, 10 | `0x03001B10` | once | 8 | idle |
| 4..6 | 5..7 punch | `0x03001B50` | once; enter: swing sound 50 + rand(2) | 4 | idle |
| 7 | 8 guard | `0x03001B04` | none (frame 0) | - | - |
| 8 | 9 bored | `0x03002B38` | tick >> 5, then frames 2.. loop | 32 | - |
| 10, 11, 13, 14 | 11, 12, 14, 15 | `0x03001B58` | loop | 8 | - |
| 12, 17..22 | 13, 19..24 | `0x03001B18` | once | 16 | idle |
| 15 | 16 | `0x03001B48` | loop | 8 | - |
| 16 | 18 knockdown | `0x03001B40` | once | 32 | hold |
| 23 / 24 | 11 / 12 | transform up `0x03002BBC` / revert `0x03002BB4` | once | 16 | form change |

A sprite without the animation reads a null slot: 0 frames, so one-shot states end on their first tick (Gohan form
9's punch lasts 1 frame, form 12 has no hurt state).

**Base states** (`player.ts`): idle control `0x08013022` (R + guard animation -> block; >1024 idle frames -> bored;
d-pad press twice within 20 frames (single key) -> dash, the facing is not changed; d-pad held -> walk, the facing
taken from the newly *pressed* keys L, R, U, D in that order (a key held since before keeps the old facing for the
first walk frame); A -> melee, else B -> skill, also on the frame the walk starts); walk `0x080131BA` (horizontal
try-move then vertical, facing table `0x083C348C`; no d-pad -> idle; then A / B); dash `0x08012DEE` (whole-footprint
test; blocked -> hurt state with raw 10 + sound 59 + screen shake (8, 100); the other d-pad keys add the walk speed);
melee `0x08012A24` (one message to every object; any accepted -> sound 48 + rand(2), a critical -> shake (15, 75);
then SetAnim(4 + rand(3))); block `0x08012BC6` + the energy shield object `0x0801DFE0` (power 0..23 fades in, 24..55
loops, both resists 0 from 24, EP acc += rec+0x57·0x3277 >> 4; released B: the power runs down to 0 before the block
can end); hurt / bump state `0x0804D75C` (resists 0, knocked back 0.75 px per frame against the facing, 24 frames
counting the hit frame); a hit (`0x080105FA`): damage != 0 -> blocked from the front: pushed back 2 px, else facing =
hit dir ^ 1 and the hurt state; HP 0 -> KO (`0x0801088A`: raw 18, shift 5, then hold; transformed forms (flag 0x40)
fall back to form +0x17 and their form object stops sound 54; the death script starts song 8 200 frames later).
Screen shakes (`0x0800797C`) draw two `next()` per period from the game RNG.

**Forms**: Super Saiyan / SS3 (`0x0804E2B0`, the singleton `0x03002BBC` shares it): no EP paid, sound 54, 16 frames
per animation frame (176 for Goku), the gauge fills to maxEP·tick / (16·nframes); at the end SS cooldown = 0x37FFF,
form = up, the SS form object plays 54, SetAnim(0) whose exit stops 54. Revert (skill again / empty gauge): not
hittable, 16 per frame, form = down, gauges 0. Gauges drain only with the form objects `0x03001B68/6C/78/80` (an
empty gauge at an overflow reverts; 7, 8, 17, 30, 37, 38 never run out). Saiyaman: shift-2 one-shot into form 14 / 10.
Story Trunks 24 / 25 (form objects `0x03001B88` / `0x03001B8C`, updates `0x0800E592` / `0x0800E5F4`): no regen, gauges
or cooldown; a countdown (form 24: 120 + rand(180), form 25: 240 + rand(120), drawn after the idle's rand(255) when the
object is built at the end of a transformation) runs first in the player update; once negative, SetAnim(23)
(transform up) / SetAnim(24) (revert) every frame the player is not already in that state - interrupting anything, KO
included (the player is then idle at HP 0). Cutscene fusions 32 / 33 / 42 / 43 (object `0x03001B64`): idle without
control and not hittable; after the regen a countdown (300 at construction); negative -> the defuse script s014: no
controller / control for 297 frames, the state tick frozen at +90..+94, at +85 the partner's record takes over
(Trunks / Goku: form, HP, EP, skills; SetAnim(0) + the NPC's rand(255)), x -50 per frame +95..+174, snapped to
start - 16 px at +175, rand(255) at +295, control again at +297. The death script blocks the player update at KO + 199.

**Skills** (`player.ts` updSkill / skillTick, specs from the state code): Energy Blast (frame 1 + shot at tick 16,
queued B refires at 40), beams (charge 24 ticks, B read from 24, drain acc += D·factor >> 4, frame ((t >> 3) & 1) + 1;
the beam object grows 3 px per frame from 8 until its end passes the camera by 8 px and retracts 3 px per frame after
the release: stop 32 + sound 33 when gone), Flame Kamehameha (no drain on the fire frame, tick +2 per frame), Final
Flash (charge frames (t >> 2) & 1, fire at 24, stop 32 8 frames after the release), Big Bang (charge into the ball
while held up to 1000, release fires if tick >= 16 else fizzles), Masenko (fires on the B release edge, idle 31 frames
later), Burning Attack (launch at 16, idle at 32), Ultra Volleyball (frame t >> 4, ball at 16, idle after 32),
Kamikaze Ghost (ghost at 16, committed at 40, respawn while held), Dragon Fist (dragon at tick 16, the player moves
0.375 px per frame until tick 90, 5 EP per frame, launch on release / tick 300 / EP 0: stop 32 + sound 23), Energy
Punch (B read from tick 8, until tick 59), Energy Sword (charge 80 frames, 90-frame swing steerable at half walk
speed), Instant Transmission (target pick freezes the world: modal loop; teleport sequence 60 frames per target,
sound 31, EP 40 / 80 / 480).

**Attack objects** (verified against the target dummy, `*_vs_*`): every message goes to every object (0x08011C0C)
unless noted "first receiver" (0x08011C70); the enemy accepts a message whose box strictly overlaps its body box
(def+0x10 + 16 * dir at the integer position) - no per-target limit, it takes every overlapping message.
* Punch: PUNCH_RECT `0x083C344C`, once. Energy Blast (`0x0804B454`): spawned at the hand point `0x0800FF44` (the body
  box: down (cx, y), up (cx, cy), left (x0, cy), right (x1, cy) - per form), 3 px per tick, 16 x 16 box centred.
  Burning Attack launches from the same point.
* Beams 15 / 16 / 6 / 1 / 3 (`KameBeam`, class `0x080206B8`): at the fire position, len 8; head box +-8: wall -> stop;
  head message accepted -> hit effect (when now - next > 0: next = now + 3 + rand(5), sound 48 + rand(2), explosion with
  rand(32) x2 + rand(0x4000)), no growth; else +3 px while the head box overlaps the camera; then always the body
  message (position..head +6). Released: moves 3 px, len -3 per tick, gone at len < 0 (stop 32 + 33).
* Final Flash (`0x08036FA2`): at player + `0x0871DF9C`, length from the constructor's 24 px wall squares (+8 each),
  scale +0x20 per tick, half width 8 * scale >> 8, one message per tick; accepted: the same throttled sound.
* Flame Kamehameha links (`0x0802DB6C`): lv + 3 links 16 px apart (each spawns the next at its tick 8); per link per
  tick one message to the first receiver with the previous tick's record box; accepted: 48 + rand(2).
* Big Bang ball (`0x08037480`): at player + `0x0871DD9C`; flight 0.5 px per tick; every 4th tick one message, record
  box centre +- size[f], mult (charge * 0x5762 + 0xFFFF) >> 16, flies on; explodes only at a wall / on an Energy Blast
  (one message x 81 * charge, stop 32, sound 23, 7 harmless explosions); off camera on its far side: stop 32.
* Masenko: cursor d (Lv1/2 +-2 bouncing 16..70/100, Lv3 d-pad), the ball's 43-tick lob (vx = dx * d * 0x5F4 >> 8,
  vy - 0x200 + 0x20 per tick), one message at the landing tick with the flight record box (sound 48 + rand(3)).
* Ultra Volleyball wave (`0x080384C2`): box origin..origin + dir * len (+2), len 8 +4 per tick to 48 then moves 4 px;
  accepted / wall / 240 ticks -> retracts.
* Energy Punch (`0x0801B296`): every tick 8..59 while held, the melee rect (Lv3: `0x0871DF30`), x62 / x144 / x226.
* Energy Sword: every swing frame one message to the first receiver (x10, sub 15), accepted -> 48 + rand(2).
* Dragon Fist (`DragonFistFx`, chain `0x08038AD8`): 6 segments, root -> head each tick, middle segments eased toward
  their neighbours, the head steered by the d-pad (+-57 / +-33), every segment sends one message (x0.32, sub 18) until
  the launch (one x2.5 message with the head box); the player's control shifts the chain (v = 0x60 or 0, +0x20 per
  segment) below tick 90; a cancel explodes it at once, head first.
* Instant Transmission onto a target: arrive below the target (x0 + x1) / 2 - 8, y1 + 8, facing up, one punch at
  t = 24 (melee x1.5), 60 frames per target.

**Player hit reaction** (every attack type the same; `f02_hurt_kinds` injects sub 0..18 and a 0-multiplier hit,
`f02_hurt_block` blocked / shielded hits): not hittable (+0x1CC = 0) -> the message is not accepted; else ComputeDamage
(melee: crit chance + rand(26), energy: rand(26) - also when blocked or in i-frames), the 12-frame flash and a
ceil(damage / 1024) popup for every accepted hit (0 included); damage 0 -> nothing else; blocked from the front ->
pushed back 2 px; else facing = dir ^ 1, the hurt state (24 ticks, resists 0, 0.75 px per frame back); thorns: the
melee attacker takes (damage * thorns) >> 8. Enemies placed with the map update after the player (the player object is
added after the area's actors; the object list `0x08011A1E` updates newest first), so their melee lands after the
player's update.

**Sounds**: the player emits the exact driver calls (`audioEvent('sfx' | 'sfx_stop' | 'song')`, `audio/bus.ts`); the
director plays them with one voice per call and StopSfx stopping every voice of the id. Music: the arena map's own
song; the battle theme 44 only for the story fights whose scripts play it (Majin Buu at West City #26, the World
Tournament ring #313 / area 7-7) - `boss_music_types` in `data/buu/audio.json`. HUD low-HP beep: sound 41 (vol 32)
while 0 < HP <= maxHP / 5, > 60 frames apart. No sound for blocking, being hit (the attacker's code plays its own),
KO (song 8 200 frames later), level up 36.

## Coverage (lockstep, 633 traces)

All scenarios of every form pass except the known divergences below (`KNOWN` in the test, 17 skipped), including the
story forms 24 / 25, the cutscene fusions and 49 target-dummy scenarios (every skill of one form per character, the
levelled ones at Lv2 / Lv3, steering, Instant Transmission Lv1 / Lv3). Forms (sim id: game name): 0..8 Goku / SS Goku /
SS3 Goku (incl. halo / Other World variants), 9..18 Gohan (Teen Gohan, Mystic Gohan, Great Saiyaman, Gohan, SS Teen
Gohan, SS Gohan), 19..20 Goten, 21..25 Trunks, 26..30 Vegeta, 31..38 Gotenks, 39..40 Vegito, 41..44 Gogeta, 46..53
Android 18, Hercule, Krillin, Piccolo, Pikkon, Tien, Videl, Yamcha.

## Known divergences (not modelled)

* NPC-sprite forms: Trunks 23 (dash / punch / Flame Kamehameha) and Android 18's punch read garbage def slots (IWRAM /
  OAM addresses), so their lengths depend on RAM contents.
* Great Saiyaman 14 + Super Saiyan: the game's canUse passes (up 255 != down 10) and the transformation reads form 255.
* Cutscene fusion `ssdrain` (32 / 33): ~150 frames after the defuse the idle blink comes one tick late (unexplained; the
  other defuse traces match).
* Kamikaze Ghost (`f31_vs_skill13`): the ghost's homing (`0x08038F2C`) aims at the nearest enemy using the sprite frame
  boxes (`0x0800FE34`: frame record +4..+7) and moves `speed` along the dominant axis; not ported (the sim's ghost homes
  in on the body centre).
* Real enemies attacking (`f02_vs_enemy_*`): enemy-side AI / animation fidelity (e.g. the walk -> idle switch when a
  chase stops) diverges after the first ~10..90 frames; the AI composites now follow the game (a finished Sequence wraps
  without re-entering its first child, the root is not restarted, StandStill's enter is SetAnim(0)).
* Player special holds (candy, stone, freeze, mummy wraps, Enzyme latch, vampire, bear hug, grapple), flames' no-hurt
  hits and the dark sphere's circle test are decoded (work/ls/agentG) but not yet ported or lockstep-recorded.
