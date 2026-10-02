# Buu's Fury — script runtime (execution model, scenes, text, ground-truth traces)

Scope: how bytecode scripts run over time in the real game (what blocks, what resumes them, what state they
share), the scene/step layer around them, the dialog text format, and the emulator traces used to verify a
re-implementation. Opcode semantics and native names live in SCRIPT.md (this file is merged into it).

Status words: **VERIFIED** = observed in the emulator (traces below); **DISASM** = read from the code;
**INFERRED** = deduced, not observed.

Tools (all new, `re/buu/`): `script_trace.py` (tracer library + CLI), `script_trace_{run,npc,item,loot,hfil,
janemba2}.py` (scenarios), `script_trace_show.py` / `script_trace_index.py` (viewers), `script_scene_dump.py`
(scene step lists + bank text), `script_native_calls.py` (per-native call graph / blocking classification →
`work/buu/script/native_calls.json`), `script_cmd_classes.py` / `script_cmd_summary.py` (command classes →
`work/buu/script/cmd_*.txt`), `script_text_codes.py` (control-code survey → `work/buu/script/text_codes.txt`),
`script_mode_probe.py`, `script_actor_probe.py`, `script_area_objs.py`, `script_callers.py`.

---------------------------------------------------------------------------------------------------------

## 1. Three layers

1. **Bytecode VM** (`0x08004038`). Strictly synchronous: a script runs from its first opcode to `end` (0x11)
   inside one C call. It never yields by itself.
2. **Blocking natives.** Anything that spans frames is a native that *runs the game's main loop itself* —
   `run_mode(obj)` (`0x0803D8A0`) pushes a "mode" object and loops whole game frames (`0x0803D7FA`) until the
   mode says it is done; then the native returns and the script continues with the next opcode. The VM is
   simply suspended on the C stack meanwhile. VERIFIED (traces: e.g. `wait` 0x88(120) returns 120 frames later).
3. **Scenes** (`run_scene` `0x08012588`). The "script table" `0x0879B704` is really a **scene table**: every
   entry is a state machine of typed steps (script, dialog text, menu, goto …). Bytecode is one step kind.
   VERIFIED (traces show scene #4 → #38 → #516, scene #450 states 0 → 2 → 5 → exit).

Everything runs on one thread. While a script is blocked, the world keeps updating, and *other* scripts
can start and finish nested inside that frame loop (map spawn-condition scripts during a map change, the
difficulty-menu scene inside native 0xB7, per-hit script #2 during a cutscene wait). Nesting is strictly
LIFO: an outer script cannot resume before every inner one finished. VERIFIED (intro trace, frames
4069, 4560, 5249).

## 2. VM state (per interpreter object, 0x6C bytes, usually on the C stack)

| offset | content |
|---|---|
| +0x00 | stack depth `sp` (entries at +4 + 4·i; 16 slots, **no overflow check**) |
| +0x04..+0x43 | value stack (s32) |
| +0x44 | loop-stack depth (entries at +0x48 + 4·i; 4 slots, no check) |
| +0x58 | pointer to the active parameter block P0..P3 |
| +0x5C..+0x6B | the interpreter's own parameter block (used when the caller passes none; *not* cleared) |

- Context stack `0x03001034`: `[0]` = depth, `[1..]` = interpreter pointers. On entry depth++ and the
  interpreter is pushed; on `end` depth-- and `[0x03001030]` (current param pointer) is restored to the
  caller's `+0x58`. Slots run from `0x03001038` to `0x03001054`, so **nesting depth 8 is the hard limit**;
  a 9th level would overwrite G0 at `0x03001058` (DISASM/INFERRED; the game never gets close — max seen 3).
- Globals **G0..G3** (`0x03001058`, natives/setters 0..3): one set for the whole game, never reset between
  scripts. VERIFIED: G = [75, 7500, 1, 115] left by loot script #499 is still visible to unrelated scripts
  hundreds of frames later (`loot_then_gameover`).
- Params **P0..P3** (natives/setters 4..7) live in whatever block `[0x03001030]` points to. `run_script`
  natives 0xD6/0xD7/0xD8 write P0 (/P1) *into the caller's current block* and pass that same block down —
  the callee shares and can clobber the caller's P. VERIFIED: enemy drop script P=[128,251,136,0] →
  0xD8(497,75,132) → #497 enters with P=[75,132,136,0]; its 0xD7(499,75) → #499 enters with the same block.
- **Return value**: whatever is left on the value stack at `end`. Callers that care read it:
  - scene script steps: if exactly one value is left, it becomes the scene's next state (VERIFIED: #450
    state 0 returns 2 → next step is state 2);
  - value-or-script fields (`0x080089A0`/`0x080089D8` and friends): a record field `< 0x10000` is a
    constant, otherwise a code pointer whose run's top-of-stack is the value (spawn conditions, area
    fields; VERIFIED: spawn-condition script `0x08074D2C` returns [0]/[1] nine times during a map load);
  - `run_script` natives discard the callee's stack.
- Division (`div` 0x0A / `mod` 0x1E) calls the ARM runtime signed divide `0x08046E58(divisor=top,
  dividend=second)`: quotient truncated toward zero, remainder has the dividend's sign; divide by zero
  calls the runtime's raise(2) (`0x08047A1C`) — fatal (DISASM).

### Entry points (who runs bytecode)

| caller | what it runs |
|---|---|
| `0x08004098 run_script(interp, header, params)` | **only state 0** of a scene header (kind 0 raw at blob+3, kind 4 LZ at blob+4 → 0x800-byte heap buffer); any other kind asserts. Used by natives 0xD6–0xD8 (shared P) and by game code (enemy death `0x0800FC5E` runs #2 per damaging hit, etc.) |
| `0x08011F5A run_inline(…, code, state_ptr, scene)` | a scene's script step (kinds 0/4) |
| hook records `{handler, data}` | `0x0800421F`: run inline script at `data`; `0x08004237`: run code stored inside the record (+4); `0x0800424F`: **run_scene(data)**. Object/spawn records carry these (e.g. Janemba's spawn record +0x14 = `{0x0800424F, scene #450}` = on-defeat) |
| item / equipment / skill records | inline code pointers run directly by `0x08004038` (item use: VERIFIED, `item_use_0`) |
| value-or-script fields | spawn conditions, area/object parameters (see above) |

## 3. Blocking: modes, the director, per-actor command queues

### 3.1 run_mode (`0x0803D8A0`)

Mode stack at `0x03000F90`: `+0` current mode, `+4` frame counter, `+8` depth, `+0xC..` saved modes.
`run_mode(m)`: save current, `m->enter()` (vtable +4), make it current, then loop (`0x0803D7FA`):
`cur->frame()` (+0xC), wait for VBlank / system tick, `cur->step()` (+0x14) until it returns non-zero; then
`m->exit()` (+8) and restore. Most script modes inherit the "game" step `0x08007041`: run the normal world
update `0x0800965C` (actors, triggers, camera, text) and then ask `+0x28 is_done()`. So **a blocking native
= N full game frames, then return**. VERIFIED (block durations in every trace).

### 3.2 Per-actor command queues ("tracks")

- Every actor object has a track at `obj+0x18C` = `{head command, owner}`; the camera controller
  `0x03002BD8` (= `[0x08054FBC+0x20]`) has one too (camera commands, e.g. native 0x6B).
- Natives 0x3B–0x70, 0x80, 0x93 allocate a command object and call `enqueue(director, cmd, track)`
  (`0x0801936A`). If the actor id resolves to nothing the command is **silently deleted** (DISASM; seen as
  the Janemba stand-in hang below when a *camera* target is missing).
- Command object protocol (vtable): `+0` dtor, `+4 start(cmd, track)`, `+8 update(cmd, track)` per frame,
  `+0xC done?(cmd, track)`, `+0x10 hold?` (the last command of a track may stay active after completion,
  e.g. 0x47 "look at"), fields from `+8` = native arguments.
- Tick (actor message 0x0C, `0x080193AE`): `head.update(); if head.done(): if no next && head.hold(): idle
  (1) else pop+delete, start next (msg 0x0D)`; return 1 = idle, 2 = busy. Commands of one actor run
  **serially**; different actors run **in parallel**.

### 3.3 The director (`0x03002BAC` = `[0x08054FBC+0x1C]`) — batch vs immediate

- `director[0]` = batch flag. Native **0x38 begin_batch** sets it; native **0x3A commit** runs the batch.
- **Immediate mode** (flag 0): every enqueue ends with `run_mode(WaitAll)` (`0x0804C0C4`): start all heads
  (msg 0x0D to all world objects and the camera track), then each frame tick all queues (msg 0x0C) until
  **every** actor queue and the camera queue is idle. So each actor command blocks the script until it
  finishes. VERIFIED: `walk_to`-like 0x60(actor,x,y) blocks 77 frames when issued alone.
- **Batch mode**: enqueue just appends; commit (0x3A) runs `run_mode(group)` if a group was declared with
  native **0x39 wait_for(actor)** (group object `0x0804DEEC` in `0x03001004+4`: waits only for the listed
  actors), else `run_mode(WaitAll)`, then clears the batch flag. VERIFIED: three 0x60 walks inside
  0x38..0x3A return immediately and the commit blocks 77 frames (intro frame 5855); Janemba handover
  0x3C+0x49 batch commit blocks 17 frames.
- Remake mapping: a batch = a parallel group of per-actor timelines; commit = "wait until group idle";
  immediate command = a one-command batch. The suspended-script state is just (VM frames, director flag,
  queues) — all plain data.

### 3.4 Which natives block (frame-spanning)

From `script_native_calls.py` (static, reaches run_mode / enqueue / commit / run_scene) plus traces:

- **Actor/camera commands (block in immediate mode, return at once in batch):** 0x3B–0x70 (except the
  pure setters among them), 0x80, 0x93. Command classes: §3.5.
- **Own run_mode (always block):** 0x31, 0x32 (camera pan to actor / point, wait arrival), 0x3A (commit),
  0x72–0x7E family (0x72/0x73 cutscene backdrop change with/without fade, 0x74/0x75/0x79 fades, 0x76, 0x78
  fade-out N, 0x7A–0x7E screen effects), 0x88 and 0xEF (wait N frames: 0x88(120) = 120 frames VERIFIED),
  0x89, 0x8C, 0x8D, 0xA7, 0xC6, 0xC7, 0xCE, 0xCF, 0xD1.
- **Nested modal scene:** 0xB5–0xB9, 0xC2 = `run_scene(idx [, P0..P3])` (`0x08012588`; writes P into the
  current block first). Blocks until the scene exits. VERIFIED: 0xB7(516,1450,1451) = difficulty menu,
  466 frames, scene #516's script sees P=[1450,1451,0,0].
- **Map change:** 0x77(world, map, x, y, facing?) blocks ~24 frames while the new area loads (spawn
  conditions run nested) and **the script continues in the new map** (VERIFIED intro frame 5226 → 5250).
  No longjmp kills the running script on this path.
- Measured durations of the natives seen in the traces: `script_trace_index.py` (blocking(>1f) lists). A
  "1f" gap can just be CPU time crossing VBlank and is not blocking.

### 3.5 Command classes (vtables constructed by natives; full disassembly `work/buu/script/cmd_classes_asm.txt`)

Per-actor commands (done? semantics in brackets; "instant" = one tick):

The done-condition column type (instant / frame counter / flag / arrival) is DISASM for every class
(`cmd_summary.txt`); the behaviour words are VERIFIED where marked, otherwise INFERRED from the start
method's callees and the arguments seen in scripts — treat them as provisional names.

| class | natives | behaviour |
|---|---|---|
| 0x0804C7D8 | 0x3B (0x3A tail) | actor anim/state 0x12 for N frames (field +8 counter), spawns an effect at frame 16 [counter] |
| 0x0804D818 | 0x3C | play animation (actor, anim) [anim finished] — 0x3C(0,12) blocked 33 f VERIFIED |
| 0x0804E0E8 | 0x3D | set animation, no wait [instant] |
| 0x0804C414 | 0x3E | emote/effect above actor [effect finished] |
| 0x0804B2B4 | 0x3F, 0x40 | spawn effect at actor box centre [instant] |
| 0x0804DCE0 / 0x0804DCF4 | 0x41 / 0x42 | effect at actor, wait for it [effect flag] |
| 0x0804B918 | 0x43 | create helper object [instant] |
| (helper) | 0x44 | move actor to (x,y) at speed (fly/glide) — 0x44(0,90,150,200) 83 f VERIFIED |
| 0x0804AE5C | 0x45 | jump/arc to point [arrival] |
| 0x0804C428 / 0x0804DD58 / 0x0804C864 | 0x46 / 0x5C / 0x5E | timed effect sequences (0x080206B8) [sequence end] |
| 0x0804AE70 | 0x47 | keep facing another actor (persistent, hold=1) [instant] |
| 0x0804C43C | 0x48, 0x65 | spawn effect [instant] |
| 0x0804AE84 | 0x49 | linear move over N frames (x,y,frames) [flag +0x20] |
| 0x0804AB58 | 0x4A, 0x68 | wait N frames on this track [counter] — 0x4A(0,3) = 4 f VERIFIED |
| 0x0804C450 | 0x4B | play SFX [instant] |
| 0x0804C800 | 0x4C | screen/actor flash [counter] |
| 0x0804AE9C | 0x4D | remove actor from world [instant] |
| 0x0804B2C8 | 0x4E | **morph actor into sprite id** (new object at same place/facing, queue transferred) [instant] — VERIFIED (Vegeta NPC → Goku NPC in #450) |
| 0x0804DEA0 | 0x4F | attach/link actor to another [instant] |
| 0x0804DD1C | 0x51 | set actor property via message [instant] |
| 0x0804DEC8 | 0x53 | face direction (0 down,1 up,2 left,3 right) [instant] — VERIFIED (spin = 3,0,2,1) |
| 0x0804C814 / 0x0804DD30 / 0x0804CD60 / 0x0804BF14 | 0x54 / 0x56 / 0x69 / 0x55 | actor flags/visibility/sprite state [instant] |
| 0x0804C828 | 0x57 | walk/teleport-step to point at speed [arrival] — 0x57(149,30,60) 62 f VERIFIED |
| 0x0804C83C | 0x58, 0x80, 0x93 | give/spawn pickup or effect at actor, wait [counter +8] |
| 0x0804E268 | 0x59 | spawn object and wait for it [obj flag] |
| 0x0804C850 / 0x0804DD44 | 0x5A / 0x5B | path move (0x08016304) wait / no wait — 0x5A(161,2) 25 f VERIFIED |
| 0x0804B940 | 0x5D | set position (actor,x,y) [instant] — VERIFIED |
| (helpers) | 0x5F, 0x60, 0x61, 0x52, 0x50 | walk to (x,y) with path finding — 0x60 77 f VERIFIED |
| 0x0804E0FC | 0x62 | palette/fade effect on actor [flag] |
| 0x0804DEB4 | 0x63, 0x64 | set/clear actor flag [instant] |
| 0x0804B92C | 0x66 | move with acceleration (0x080192CE) [flag +0x30] |
| 0x0804DD08 | 0x67 | shake/oscillate [flag +0x2C] |
| 0x0804DCCC / 0x0804B2A0 | 0x6A / 0x6C | camera track: set behaviour, then wait N [counter]; 0x6B camera pan (x?,?,y) — 600 f VERIFIED |
| 0x0804C7EC / 0x0804E0D4 / 0x0804D480 / 0x0804D494 | 0x6D–0x70 | camera/screen layer commands [instant] |

Mode classes (run_mode objects): 0x0804C588 (0x72/0x74/0x75/0x79; fixed-length fade, +8 = frames),
0x080495B4 (0x76), 0x0804BBD4 (0x7A), 0x0804C5B8 (0x7B), 0x0804A5B4 / 0x0804A070 (0x7C / 0x7D, blend-register
flashes), 0x0804A9D8 (0x7E), 0x08049978 (0x88 / 0xEF wait), 0x0804CF64 (0x89), 0x08049504 (0x8C, full-screen
picture mode), 0x0804920C + 0x0804A404 (0x8E), 0x0804B5D8 (0xC6), 0x0804D27C (camera scroll wait, 0x31/0x32),
0x0804C0C4 (WaitAll), 0x0804DEEC (group wait), 0x08049D18 (modal scene). Names of individual natives are
the native-naming workers' job; the per-class code is in `cmd_classes_asm.txt`.

### 3.6 Actor ids (`0x080001C6 actor(id)`)

- `0` = the player object (`[0x03001C34]`), `9` = `[0x03001BAC]` (special companion object),
- `1..8` = the sprite of character record `id-1` (Goku, Gohan, Goten, Trunks, Vegeta, Gotenks, Vegito,
  Gogeta), looked up as "first world object whose sprite def is that"; the player matches its own
  character's id (VERIFIED: actor 1 = player while Goku is played),
- `>= 10` = sprite table id (`0x086B6BDC`), first world object using that sprite (149 = Janemba, 154 =
  Super Janemba — VERIFIED),
- `< 0` = map placement tag `-(id+1)` (world message 7).
Native **0x95(char)** switches the played character (reloads the player sprite) — VERIFIED in #450.

## 4. Scenes (the table at 0x0879B704)

- Entry = pointer to a **header**: `u32 step[state]` (length not stored). A word `< 0x10000` means
  **exit(value)**: the scene ends, `value` is written to the caller's result (only if `[0x03002A58]` = 0).
  Otherwise it points at a **step record** `{u16 next_state, u8 kind, u8 arg, payload…}`. The scene object
  (`0x0801246E`): +0 header, +4 result ptr, +8 u16 state, +0x14 param block (default: its own 4 words at
  +0x18), +0x28 current step. Each frame (`0x080124F8`) the current step is ticked; when it reports done,
  the next state is `next_state` unless a script step returned exactly one value. VERIFIED (all traces with
  `step` events).
- Step kinds (factory table `0x080728AC`, DISASM; VERIFIED where traced):

| kind | payload | behaviour |
|---|---|---|
| 0 | bytecode at +3 (`arg` is just the first opcode byte) | run script; 1 value left → next state |
| 1 | UTF-16 text at +4, `arg` = portrait | dialog box |
| 2 | +4 format string, +8 bytecode | run script, sprintf its results into the format (`0x0803DB7C`), dialog box |
| 3 | UTF-16 text at +4 | **menu / message window** (lines starting with `-` are options); choice index → native 0x18 |
| 4 | LZ-compressed bytecode at +4 | same as 0 (decompressed into a 0x808-byte heap buffer) |
| 5 | u32 bank text ref at +4, `arg` = portrait | dialog box with bank text (VERIFIED: every story line) |
| 6 | u32 bank text ref | same text source, alternate box (DISASM) |
| 7 | +4 = another scene header | goto: switch header, state 0 (VERIFIED intro #4 → #38) |

- So a cutscene is a mix of script steps (actor choreography via natives) and text steps; branching is by
  script steps returning a state index (#450: `if character == Goku return 3 else return 2`).
- Plain NPC talk is a one-step scene (kind 5 + exit) **with no bytecode at all** — VERIFIED (Gregory:
  scene #43, `npc_gregory` trace has only step events).
- Remaining kinds' exact box differences (1 vs 5 vs 6) are DISASM-level only.

## 5. Text and dialog format

- **Banks**: 7 LZ containers at `u32[0x087B5B64 + 4·b]` (6 × ~64 KB + 1 small), UTF-16, *not*
  zero-terminated. A text ref `r`: bank = `r & 0x3F`, start = `((r >> 6) & 0x7FFF) · 2` bytes, length =
  `r >> 21` UTF-16 units. The 1534-entry string table `0x0806BCE8` (zero-terminated) holds UI/item text.
- **Box placement prefix** (first character, consumed; `0x08012012`): `!` → narration box at y=40, `@` →
  system box at y=80, `#` → bottom box (y=120); none → placed by speaker/portrait.
- **Portrait `arg`**: 0 = none; 7 = none (alternate side); 1–6 = expression slot of the **current player
  character's** portrait set (`char_def[+0xF + arg]`, i.e. "Goku smiling" when you play Goku); ≥ 8 = global
  portrait index into `0x086F393C` (e.g. 144/145 = Vegeta). Speaker names are part of the text
  ("GREGORY: …").
- **Control units** (survey `work/buu/script/text_codes.txt`; renderer `0x0803DD66`, word-wrap at spaces,
  proportional widths, alignment byte 0 left / 1 centre / 2 right):
  - `0x000A` newline (DISASM);
  - `0x0008 X` set colour to palette index X for following glyphs (DISASM; X = 2 normal white, 7 highlight
    — names/places/buttons, 3, 5, 6 stat colours, `v` (0x76) value colour, 0x1D);
  - `0x000B` page break: wait for A, continue in a fresh box (INFERRED from context, e.g. "...at peace...
    {000B}Or was it?!");
  - `%s`, `%d` printf fields (kind-2 steps and UI strings);
  - text longer than the box width is truncated with ".." in single-line widgets (`0x0803DCC4`).
- Choice menus (kind 3) list options as `-Option` lines; scripts read the result with native 0x18
  (`[0x03002A60]`; VERIFIED in the shop scene #34 code: `if choice == 0 return 2 …`).

## 6. Ground-truth traces (verification part a)

`script_trace.py` single-steps only the frames in which the VM or a scene runs (others run at full speed
after a breakpoint probe) and records:

| event | when | fields |
|---|---|---|
| `scene` | `run_scene` entry | scene index, header, caller lr, P block |
| `step` | each scene state visit | scene, state, step kind/arg/next or `exit` value, bank text ref |
| `run` | `run_script` entry | script index, header, caller |
| `enter` | interpreter entry | code address, caller, depth, G, P, first code bytes |
| `native` / `set` / `inc`… | ops 0x02 / 0x03 / 0x15–0x1A | id, code offset, depth, stack **before**, stack **after** (`after`, `frame_after`, `G_after`, `P_after`), loop stack |
| `exit` | `end` | final stack (= return value), op count |

A re-implementation fed the recorded native results (the `after` stack minus the popped args) must issue
the same native calls with the same arguments in the same order. Files in `work/buu/script/traces/`:

| file | events | scenario | natural / injected |
|---|---|---|---|
| `boss_janemba_handover.json` | 689 (scene #450 + 465 natives) | Janemba dies in HFIL (18,14) → scene #450 (Vegeta takes over, player becomes Vegeta, Super Janemba appears) → fight | Janemba spawn natural (flags 123/125 + warp); Vegeta NPC (actor 5) **injected** through the game's own spawn path because the warp skipped the story step that places him; lethal hit **injected** through the normal damage path 0x0800FA2A; scene natural |
| `intro_cutscene.json` | 162 (scenes #4 → #38, nested #516) | new-game intro narration, Gohan's house, difficulty menu (0xB7), map change (0x77) with nested spawn-condition scripts, batch walks | natural (A mashed) |
| `npc_gregory.json` | 15 steps, 0 VM ops | NPC talk on Snake Way: scene #43 text only | natural |
| `item_use_0.json` | 14 | Miso Soup used twice from the menu (HP 40 → 60 → 80): is_equipped(119), get/set HP, consume, SFX | natural (inventory count and HP poked) |
| `loot_drop.json` | 562 (scripts #497, #499, #500, #2) | three enemies die → drop scripts, coin sizes, per-hit EP script #2 | lethal hits injected via 0x0800FA2A; deaths and drops natural |
| `loot_then_gameover.json` | 139 | same + level-up and the game-over sequence (scene #0 code) | as above; recorded before scene hooks existed (no `step` events) |
| `hfil_arrival.json` | 156 | arrival in HFIL (18,14): only script #2 | natural after warp |

Map event coverage: the intro trace contains the map-change native, spawn-condition scripts and scene
gotos; item-use, loot and dialog paths are separate traces. Not traced: equipment effect scripts and
skill-upgrade scripts (same VM, no new mechanism).

Known caveat (kept as a lesson): running #450 in a stand-in arena without the Vegeta NPC makes 0x4E drop
its command, and 0x31 (camera to actor 1) then pans toward garbage forever (171 655-frame scroll) — missing
actors silently break blocking natives. A remake should make "actor not found" explicit.

## 7. Notes for the re-implementation

- The only places a script can be suspended are blocking native calls. A resumable interpreter therefore
  only needs to yield there; its suspended state is: the stack of interpreter frames (pc, value stack ≤ 16,
  loop stack ≤ 4, P pointer), G0..G3, the director batch flag + group, the actor/camera command queues and
  the scene objects (state, current step). All plain integers — cheap to snapshot for rollback.
- Determinism: the VM itself is integer-only; randomness only via natives 0x19/0x1A (game PRNG).
- Order of side effects inside one frame matters (nested scripts run in the middle of the outer script's
  wait, sharing G and P); a faithful port keeps the "run nested scripts to completion before resuming"
  rule.
