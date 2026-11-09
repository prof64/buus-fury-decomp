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

## Semantic changes for ports (wf-core) - enemy AI and world

These change behaviour beyond single nodes; a port of the simulator should follow them:

* **Object update order**: the world updates newest objects first (0x08011A1E walks its list from the end); the
  player object is added after the area's enemies, so it updates before them; objects created later (shots,
  summons) update before the player.
* **AI composites** (`ai.ts`): Sequence (0x08018F0C) - a finished child exits and the next enters; after the last
  the index wraps to 0 without re-entering the first child; enter enters the child at the *current* index; exit
  exits it without resetting the index. Selector (0x08019192) - first non-zero child result. ParallelAll / Any
  (0x08019108 / 0x08019162) - every child every update, AND / OR of the results, no per-child completion. Cond
  (0x080342AF) - the child's result is returned and a finished child is not exited. A GotoState inside a tree does
  not stop the running tree's update (the replaced tree finishes its evaluation); 0x0800AB0C ignores the root's
  result (no restart).
* **Distances** between footprint centres ((x0 + x1) >> 1 of the 0x0800FDFA box), `dist2 <= r2` (signed). The
  player reference stays valid after a KO. IfTimeInState counts from obj+0x210, stamped only by Block's exit.
* **Animations**: every enemy SetAnim(0) enters the idle state (enter draws rand(255)); enemies created mid-fight
  draw it in their constructor; nothing switches walk -> idle when a movement stops (the next node's SetAnim does;
  AlignWithPlayer's end and StandStill's enter SetAnim(0)); a sprite without the requested animation still runs the
  node's controller (no idle, no rand); attack controllers end with exactly one SetAnim(0).
* **Hit messages**: kind 1 (`foot`) tests the receiver's footprint; damage-only messages (`noReact`, the flames)
  skip the reaction; not-hittable receivers do not accept.
* **ROM sine table** 0x087B6EEC = trunc(sin(i * 2pi / 256) * 65536) (all 256 checked); frame record boxes
  (`data/battle/buu_frame_boxes.json`, 0x0800FE34) for picture-based tests (ghost homing / contact).
* **Player KO / game over**: the death script's clock starts the frame *after* the KO, whatever dealt it (the script
  runner precedes the object updates; song 8 at KO + 201 ticks; an injected harness hit counts from its own frame).
  At KO + 199 the script freezes every other object (no enemy AI / RNG from there on) and sends every actor message
  0xF (0x0800F44A: the walk state's vfunc +0x10 = SetAnim(0), one rand(255); other states assumed no-op). From KO +
  200 enemies only run their animation state tick. A KO'd player still accepts overlapping hit boxes (no damage), so a
  wave hitting it puffs. The game frees the player object at KO + 440 (the harness stops comparing there).
* **IsAnim(id)** (vfunc +0x34, 0x0800FAC6): the actor's state object is SetAnim(id)'s and the raw matches. Movement
  nodes SetAnim(1) only unless IsAnim(1) (PathFollow 0x0803B9DA, AlignWithPlayer 0x0801CF6E), StandStill re-enters
  idle unless IsAnim(0), ChasePlayer within 2 px SetAnim(0)s every tick (0x0802A086, rand(255) each time), stopWalk
  SetAnim(0)s only when IsAnim(1) (the back-off's reversed walk is not).
* **Enemy energy wave** (0x0802A2EC / update 0x0802A3D2, shared by Cyborg / turret / soldier): ctor plays sound 35;
  each tick tests wall then hit at the *current* length, then grows 4 (to 48) or moves 4 px; > 240 ticks fades; no
  camera test.
* **Attack timings** (all verified): HeavyStrike strikes at tick 60 then SetAnim(4), ends back in idle;
  BioMechPunch hits at tick 16, ends at 40; CyborgShot wave at tick 1, ends (SetAnim(0)) from tick 16 once its wave
  is gone; HeavyPunch strike at 48, ends at 48 + 8 * frames; WindupMelee swing at 32, ends at 64 + 8 * (frames - 2),
  aborted with no swing when another SetAnim replaces its controller in the same tick. CirclePlayer (0x0802F23A)
  moves away on the major axis and toward on the minor; HoverDrift accumulates +-38 per tick per axis (reaching
  +-152); BackStep bumps first then moves 47 times; MechTransform swaps the paired graphics set (smaller boxes).
* **SetAnim(id)** (0x0800ECF0): the state object comes from the enemy table 0x080726B8 by *id*, only the raw from
  the sprite def: SetAnim(0) is always the idle state (rand(255)), SetAnim(1) the walk loop, even when a type shows
  that raw under another state too (Worm Body raw 1, Gunman raw 2). Nodes may install their own state object with
  per-frame code (`Enemy.stateHook`: tick + think run before the AI; any SetAnim replaces it; IsAnim is false
  under it) - the Buu fragment's PlayAnim0 (0x03003158, one IWRAM singleton shared by all fragments) and
  FreezePlayer (0x0300314C).
* **Input history** (0x0804023C / 0x08040284): a 64-entry ring of new presses stamped with the frame counter
  0x03000F94; `pressRun(mask, n, window)` = a press this frame and the last n masked presses within the window.
  Used by the freeze release (0xF3, 16, 90) and the mummy-wrap shake-off (d-pad, 8, 60). The frame counter's phase
  (`ctx.fc0`, 9634 at frame 0 of every f02 scenario) matters for VampireGrab's drain (counter & 15 == 0).
* **Holds**: VampireGrab (0x0804AB04) moves the *vampire* next to the player (0x08759334), escape = A/B/d-pad held
  on > 8 frames each within 10 of the last, drain POW every 16 counter ticks, exit SetAnim(9) with hit dir = facing
  ^ 1; KnockbackSlide's exit does nothing. Mummy wraps are real objects (builder 0x0802D79C, update 0x0802D940):
  stick 4 frames after a hit (sound 48 + rand(2)) into slots 0x030030BC, each stuck wrap -48 walk / dash and
  resists reset to 128 (0x0802D8E6), >= 8 stuck -> a new wrap bursts. Worm Head: segments spawned in the node's
  enter (one rand(255) each); turn = rand(200) == 0 -> dir ^= 2, then rand(2) == 0 -> dir ^= 1. Super Buu's candy
  beam (0x080287AC) grows 4 px to 64, slides, retracts on wall / hit / 240 frames; KiBlast is the player's blast
  object (16 x 16) from the body-box point 0x0800FF44.
* **More holds** (all verified by the hold recordings): Enzyme latch (LatchDrain 0x0804AA94) does *not* hold the
  player - SetAnimRaw(1) + a controller that glues the enzyme at its entry offset, 1 HP every 64 counter ticks;
  IfLatchOverlap is a pure overlap test (player body shrunk 6 px left / right vs the enzyme's 7 x 7 rect, which only
  exists facing down); IfButtonMashed (0x080363C4) counts new A/B/d-pad presses, a gap > `gap` resets, true at
  count > `presses`, state built with the tree. Bio Mech grapple arm (0x08037FB4): spawned at tick 28, box
  (hx, hy-12)..(hx+32, hy+12), grows 4 to 80, a front block cancels, else the player is reeled 4 px / frame. Bear hug
  (0x0802C4C9 / 0x0802C565): player at Spopovich + (8, -16) raw 21, d-pad presses toggle the frame, the 5th success of
  0x08040284(0xF0, 20, 90) releases, Yamu's SetAnim(9) releases too; Yamu's EnergyDrain walks to Spopovich +- 40 px
  (0x0800A1D4) and drains 1 EP / 8 ticks to EP - maxEP / 4 (counter starts at 0x6D, uninitialised heap). Candy: idle /
  walk controllers (16-frame hops, the timer only ends at a hop boundary), EatCandy removes the player object at t8,
  drains min(POW >> 2, HP) every 8th tick t24..111, a new player object at t120 (three rand(255)).
* **Body rects**: an empty per-facing body rect stays empty (nothing can hit that facing) - only a sprite without any
  box gets the default.
* **Kamikaze Ghost** (`0x08038F2C`, state 0x11B): homes on the nearest enemy using the per-animation-frame sprite boxes
  (frame record +4..+7, `buu_frame_boxes.json`), moving `speed` along the dominant axis.
* **Lockstep harness**: lag frames skipped; a real enemy is inert (state 0xC) until frame 1, when its AI is set to
  the scenario's state (default: the type's initial state) by an injected GotoState; a partner enemy at frame 2.

## Coverage (lockstep, 712 traces)

All scenarios pass except the known divergences below (`KNOWN` in the test, 9 skipped): every form, the story forms 24 /
25, the cutscene fusions, 49 target-dummy scenarios (every skill of one form per character, the levelled ones at Lv2 /
Lv3, steering, Instant Transmission Lv1 / Lv3, the Kamikaze Ghost's homing), the player's hurt reactions, the 7
original real-enemy attacks (melee, gun, bomb, beam, flames, mini, hand) for their full length, 71 real-enemy
recordings of 600 frames (`f02_vs_enemy_tXX`: one per enemy archetype - 50 - and the 21 main bosses, including KOs and
the game-over freeze) and the 8 special holds (`f02_vs_hold_*`: grapple, bear hug + Yamu's drain, stone, freeze,
mummy wraps, Enzyme latch, vampire, candy incl. being eaten). Forms (sim id: game name): 0..8 Goku / SS Goku / SS3 Goku
(incl. halo / Other World variants), 9..18 Gohan (Teen Gohan, Mystic Gohan, Great Saiyaman, Gohan, SS Teen Gohan, SS
Gohan), 19..20 Goten, 21..25 Trunks, 26..30 Vegeta, 31..38 Gotenks, 39..40 Vegito, 41..44 Gogeta, 46..53 Android 18,
Hercule, Krillin, Piccolo, Pikkon, Tien, Videl, Yamcha.

## Known divergences (not modelled)

* NPC-sprite forms: Trunks 23 (dash / punch / Flame Kamehameha) and Android 18's punch read garbage def slots (IWRAM /
  OAM addresses), so their lengths depend on RAM contents.
* Great Saiyaman 14 + Super Saiyan: the game's canUse passes (up 255 != down 10) and the transformation reads form 255.
* Cutscene fusion `ssdrain` (32 / 33): ~150 frames after the defuse the idle blink comes one tick late (unexplained; the
  other defuse traces match).
* Not reached by the recordings (inferred, unverified): message 0xF on non-walk states (no-op), MummyWrapBurst's
  spawn origin, HeavyStrike's exact reach, the Buu fragment's height bob and the candy-beam / vampire animation frames
  (cosmetic).

## Field scenarios (overworld)

The field game has its own recorders under `tools/field/` (same emulator helpers, frame 0 = the first logic frame
after an area load, lag frames recorded and skipped): `rec_transitions.py` (exits / edge exits / round trips →
`data/field/transitions_*.json`), `rec_npc.py` (map NPC behaviours and talk → `npc_*.json`; `npc_model.py` is the
Python spec), `rec_objects.py` (triggers, save points, breakables, drops, map items → `objects_*.json`). They are
replayed against the Rust core by `engine/crates/wf-core/tests/field_lockstep.rs` (`cargo test -p wf-core --test
field_lockstep`); mechanics in re/buu/notes.md §11 and the "Field: ..." sections, API in engine/docs/core.md §10.

## Visual audit (opponents, frame by frame against the emulator)

Every Battle-list opponent in its real AI, recorded in the emulator with the screen and every enemy object per frame,
replayed in the battle sim on the same map (8-3, arena rules off: the faithful core) and compared frame by frame.

* Scenarios: `opp_scenarios.py` - per opponent type t (`OPPONENTS`), `oa_tXX_{idle,left,above,below,punch,ki,ko}`:
  the player (lockstep default Goku, form 2) at (390, 290) in the forest's clearing; the opponent at its archetype
  offset / left / above / below / adjacent (A every 12 frames) / 64 px (Energy Blast every 20) / adjacent with 20 HP
  (death). A scenario is a lockstep scenario (`vs()` of scenarios.py: `input [[keys, frames]..]`, `at`, `etype`,
  `ehp`, `state`, `stats`, `hp`, `skills`, `partner`); add one to `SCENARIOS` for anything else.
* Record (emulator, headless, no sound; ~6 s for 600 frames): `.venv/bin/python tools/battle/lockstep/visual.py -j 4
  'oa_t44_*'` -> `work/ub/oa/NAME.json` (the lockstep trace + `vis`: per frame every live object of the enemy list -
  type, 24.8 x / y, facing, anim raw / frame / tick, HP << 10, AI state, height, hittable - every other world object's
  vtable, the camera) and `NAME.fb` (the screen after every frame, zlib'd RGB; frame 0 = before the first input).
* Compare (headless): `cargo run --release -p wf-app --example opp_audit -- work/ub/oa/oa_t44_*.json` (`OA_TILE=259`
  skips the map tile-clock fit for the 8-3 scenarios; `OA_OUT` the output dir, default `work/ub/oa/out`; `OA_PNG` the
  triptychs per scenario, 6; `OA_PNGAT=f,..` those frames too; `OA_NODN` leaves the damage numbers' boxes out of the
  pixel count; `OA_RNG=f,..` every generator call of those frames; `OA_SHOTS=f,..` our shots and draw order after
  them; the log's `obj_box` is each frame's OBJ-difference box; our animation raw is named by its first ROM slot like
  the recorder's; `OA_PACK=dir` also writes the gate's data, below). One line per scenario: the first diverging frame of each category - `player` (the
  lockstep row: state / anim / position / HP / EP / sounds / RNG), `dmg` (ComputeDamage results), `enemy` (position /
  facing / HP), `anim` (raw / frame / tick / height), `ai` (AI state), `others` (the other enemy objects: parts,
  summons, Janemba's hand), `pix` (OBJ pixels: a difference within 48 px of a sprite's or the game's enemies'
  anchors), `bg` (any other pixel: the HUD, the map) - and the count of frames with OBJ differences. `out/NAME.json`
  lists every differing frame with both sides' values; `out/NAME_fNNN.png` = game | ours | diff (red OBJ, blue bg).
* Not compared: frames 0 and 1 (the game's enemy waits off-screen until it is placed at frame 0 and its OBJ shows a
  frame late), the game-over screen. Our side draws the map's props (the field's; a battle world has none) and runs
  the map's tile clock at the fitted offset. The damage the game computes on Janemba's parts is not an event here
  (the `dmg` category shows it as `-1:...` on the game's side only).

### The audit gates (`cargo test -p wf-app --test audits`)

Both frame-by-frame audits are gated tests (`engine/crates/wf-app/tests/audits.rs`, headless, muted, parallel workers,
~20 s): the opponent audit above (462 scenarios, 215,555 frames) and the skill audit (538 segments, 180,160 frames:
`work/skillaudit`, playtest.md "Skill audit").

* **Data** (`data/audits/`, no pictures): per scenario one zlib'd JSON - `opp/NAME.json.z` = the recording's trace
  (`rows`, `vis`, the scenario) + `pic`: the fitted tile clock, the FNV-1a 64 hash of the game's picture of every
  frame, the torn frames' line specs; `skill/NAME.json.z` = the segment (without its `vram` rows: the audit runs
  without the tile-allocator check) + `pic` likewise. Packed from the recordings in `work/` by the audits by hand:
  `OA_PACK=data/audits/opp cargo run --release -p wf-app --example opp_audit -- work/ub/oa/oa_*.json` and
  `SA_PACK=data/audits/skill cargo run --release -p wf-app --example skill_audit -- work/skillaudit/segs/sa_*.json`
  (the comparers are shared with the gate: `tests/audit/{opp,skill,pack}.rs`). Re-pack after re-recording, or after
  a change to what the tear rule accepts.
* **Locked**: a scenario with no difference on any frame - state (the opponent audit's rows: player, damage, enemy,
  animation, AI, other enemy objects; the skill audit's segment harness) and picture - is locked: any difference
  fails the test.
* **Ratchet** (`data/audits/opp.json`, `skill.json`): every other scenario with `[first frame with a state difference,
  count, first frame whose picture differs, count]` (the skill audit's state count = the harness's differing
  comparisons). The test fails when one gets worse (an earlier first difference or more differing frames) and prints
  the ones that got better. Tighten it after a fix (read the printed changes first - it records what the run
  measured): `AUDIT_RATCHET=write cargo test --release -p wf-app --test audits`.
* **The tear rule** (as the pixel gate's, core.md): a differing frame counts as the game's when every line of the
  game's picture is ours or ours with one of the previous frame's states - every BG's scroll, some BGs' scroll (the
  registers are written one BG after the other: 8-3's frames show lines 0..8 all old, 9..33 BG2 / BG3 old), the map's
  animated tiles (their VRAM copy, also pixel by pixel inside a running copy), the palette - and the lines only an old
  state explains are all above the lines only the new picture explains (a picture old on every line would be a frame
  late, not torn). The HUD's gauge (its tiles copied into sprite memory at scanline 26-27 on some frames: playtest.md)
  is accepted old on lines above 27. `OA_NOTEAR` / `SA_NOTEAR` turn it off, `OA_TEARDBG` prints a frame's lines.
* Known, kept in the ratchet: what a kill gives in the story (the XP's level-up - its rolls, the hold, the jingle -
  the drop roll and the dropped item, the freed enemy object: a battle world has none of it, every `_ko` scenario and
  the `_punch` / `_ki` / `_fight` ones that kill differ from the kill on); two objects at an equal sort key (the
  game's unstable sort: the Worm Head and its first segment, a shield over the player); the beam charge's torn
  fragment (skill audit f9 / f110).
