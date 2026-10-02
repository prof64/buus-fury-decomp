# Buu's Fury lockstep harness: the simulator's player against the emulator, frame by frame

The harness runs the same scenario in the real game (headless mGBA, `tools/emu`) and in the battle simulator and
compares every frame: position, facing, the animation actually shown (raw index, frame index and the ROM frame
record), the player's state object, HP, EP, form, the RNG state and every sound call (PlaySfx / StopSfx / PlaySong).
It reports the first frame where they differ.

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
`pos`, `dir`, `arena` (`htc` = Hyperbolic Time Chamber 6-9, empty floor; `crater` = Giant Crater 8-71, has cliffs).

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

Trace file: `{source, form, traces: [{name, scenario, input, init, states, rows, sfx}]}`; `rows` are run-length
encoded `[n, x, y, dx, dy, dir, raw, frame, state, hp, ep, form, rom, rng]` (n frames, position advancing by dx, dy);
`sfx` = `[frame, 'play', id, vol] | [frame, 'stop', id] | [frame, 'song', id]`.

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
d-pad press twice within 20 frames (single key) -> dash, the facing is not changed; d-pad held -> facing L, R, U, D in
that order then walk; A -> melee, else B -> skill, also on the frame the walk starts); walk `0x080131BA` (horizontal
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
hittable, 16 per frame, form = down, gauges 0. Gauges drain only with the form objects `0x03001B68/6C/78/80/8C` (an
empty gauge at an overflow reverts; 7, 8, 17, 30, 37, 38 never run out). Saiyaman: shift-2 one-shot into form 14 / 10.

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
sound 31, EP 40 / 80 / 480). Hit-side details of the attack objects that the traces cannot see (no enemies in the
arena) keep the models fitted to the measured runs (`tests/battle/buu_moves.test.ts`).

**Sounds**: the player emits the exact driver calls (`audioEvent('sfx' | 'sfx_stop' | 'song')`, `audio/bus.ts`); the
director plays them with one voice per call and StopSfx stopping every voice of the id. Music: the arena map's own
song; the battle theme 44 only for the story fights whose scripts play it (Majin Buu at West City #26, the World
Tournament ring #313 / area 7-7) - `boss_music_types` in `data/buu/audio.json`. HUD low-HP beep: sound 41 (vol 32)
while 0 < HP <= maxHP / 5, > 60 frames apart. No sound for blocking, being hit (the attacker's code plays its own),
KO (song 8 200 frames later), level up 36.

## Coverage (lockstep, 576 traces)

All scenarios of every form pass except the known divergences below. Forms (sim id: game name): 0..8 Goku / SS Goku /
SS3 Goku (incl. halo / Other World variants), 9..18 Gohan (Teen Gohan, Mystic Gohan, Great Saiyaman, Gohan, SS Teen
Gohan, SS Gohan), 19..20 Goten, 21..25 Trunks, 26..30 Vegeta, 31..38 Gotenks, 39..40 Vegito, 41..44 Gogeta, 46..53
Android 18, Hercule, Krillin, Piccolo, Pikkon, Tien, Videl, Yamcha.

## Known divergences (not modelled)

* Story forms 24 / 25 (Trunks): their form objects (`0x03001B88` / `0x03001B8C`, updates `0x0800E592` / `0x0800E5F4`)
  count down and transform / revert on their own.
* Cutscene fusions 32 / 33 / 42 / 43: not hittable; the fusion ends after 113 frames into another character's record.
* NPC-sprite forms: Trunks 23 (dash / punch / Flame Kamehameha) and Android 18's punch read garbage def slots (IWRAM /
  OAM addresses), so their lengths depend on RAM contents.
* Great Saiyaman 14 + Super Saiyan: the game's canUse passes (up 255 != down 10) and the transformation reads form 255.
* Big Bang: the fired ball's flight / lifetime is the projectile model's (it leaves ~26 frames early, so its stop 32
  comes early); the game ball lives 156 frames in the empty arena.
* Not covered by traces (no enemies in the arena): Instant Transmission with targets (decoded and checked in the
  emulator on the forest save state), enemy hold states, candy, hits taken during skills other than the Kamehameha.
