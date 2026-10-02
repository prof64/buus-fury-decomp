# Buu's Fury — the story / event script language (complete spec)

Everything the game does in story, cutscenes, NPC talk, map triggers, item use, equipment effects, loot and
boss phase handovers runs through one bytecode VM (Webfoot engine, same family as LoG2) plus an event-graph
layer around it. This file is the spec; the implementation is `engine/crates/wf-script` (loader, exact
interpreter, decompiler) and `engine/crates/wf-script-rhai` (the remake's runtime); the modder API is
`engine/docs/scripting.md`.

Status words: **VERIFIED** = observed in the emulator; **DISASM** = read from the code; **INFERRED** =
deduced. Part B (from §B1) is the runtime chapter: blocking, director, command classes, text format, traces.

## 1. Architecture in one page

* **Events** (the "script table" `0x0879B704`, 522 entries): each entry is a *header* = array of u32 node
  words indexed by node number. Word `< 0x10000` = terminal (finish with that value); else a node
  `{u16 next, u8 kind, u8 arg, payload}`: bytecode (kinds 0 raw / 4 compressed), dialog text (5, 6 with
  printf args computed by bytecode, 1/2 unused), choice menu (3), goto another event (7). A bytecode node
  that ends with exactly one value on its stack selects the next node — that is how dialogs branch and how
  boss phases hand over (#450: Janemba → Super Janemba). Runner `0x08012588` (blocking, nested main loop);
  stepper `0x080124F8`; node handler table `0x080728AC`. VERIFIED (traces).
* **Bytecode units**: 1766 nodes reachable in events + 692 inline scripts referenced straight from ROM
  records (map triggers, spawn conditions, NPC callbacks, items, equipment, quest log, AI) + 82 enemy
  drop scripts (enemy type `+0x18`, found by the emulator trace) = **2540 units**.
* **The VM** (`0x08004038`) is synchronous: a unit runs from offset 0 to `end` in one C call. Frame-spanning
  behaviour (walks, fades, text boxes, menus, waits) lives in natives that run the game's main loop
  themselves (`run_mode` `0x0803D8A0`). No yield opcode exists. VERIFIED.
* **State**: per interpreter object (0x6C bytes): operand stack (16 × s32, unchecked), loop-counter stack
  (4, unchecked), pc, pointer to the param block P0..P3. Global G0..G3 at `0x03001058` (persist, shared by all
  scripts). L0..L3 at `0x03001020` (getters 0x08–0x0B / setters 8–11). Context stack `0x03001034` (depth ≤ 8).
  Nested `run_script`/`run_event` share and may overwrite the caller's P block. VERIFIED.
* **Natives** (`0x08055098`): 248 builtins, each pops its arguments and optionally pushes one result.
  **Setters** (`0x08055478`): 18, setter n pairs with getter native n (0–3 G, 4–7 P, 8–11 L, 12 hp, 13 max_hp,
  14 energy, 15 max_ep, 16 zenie, 17 a byte at `0x03002BB0`).

## 2. Encoding

* Raw code: kind-0 node at `node+3`, inline scripts at their address. Compressed (kind 4): a **headerless**
  Webfoot-LZ bit stream at `node+4` (no `type,size` container), inflated by `0x08049120` → IWRAM
  `0x03000040` into a 0x800-byte heap buffer. `wf-script` wraps it in a fake container; byte-identical with
  the game's decompressor on all 141 cached emulator outputs (VERIFIED, test `rom_scripts`). Note:
  `wf-tables::script::script_code` still uses `Rom::lz(blob+4)` (expects a container) — wrong for kind 4.
* varint (`01`, `1d`): 7-bit groups MSB first, bit 7 = continue, accumulated in a 32-bit register, then
  zig-zag (odd → −(v>>1), arithmetic shifts). Values beyond ±2^30 are not representable.
* Branch operands: 16-bit **big-endian, unsigned**, relative to the operand's own address. `jmp`/`jz` only
  jump forward (`target = operand + u16`); `loop` only backward (`target = operand − u16`).

## 3. Opcodes (table `0x080554C0`, 31 entries; all DISASM, behaviour VERIFIED by the trace replays)

Stack notation: `top` = last pushed, `second` = the one below. All arithmetic is 32-bit wrapping.

| op | operands | name | semantics |
|---|---|---|---|
| 00 | u8 | push | push u8 (zero-extended) |
| 01 | varint | push | push signed varint |
| 02 | u8 n | native | call native n (handler gets the interp object, pops/pushes itself) |
| 03 | u8 n | set | pop v; setter[n](v) |
| 04 | | and | pop b, a; push (a≠0 && b≠0) — both operands were already evaluated (no short circuit) |
| 05 | | or | pop b, a; push (a≠0 \|\| b≠0) |
| 06 | | neg | top = −top |
| 07 | | add | second + top |
| 08 | | sub | second − top |
| 09 | | mul | second × top (low 32 bits) |
| 0A | | div | second / top, truncating (`0x08046E58`, divisor = top); ÷0 → runtime raise (fatal) |
| 1E | | mod | remainder of second / top (sign of second); ÷0 fatal |
| 0B–10 | | eq ne gt ge lt le | signed compare second ? top → 0/1 |
| 11 | | end | null handler: the interpreter returns; the stack left behind is the result |
| 12 | u16 | jmp | pc = operand + u16 |
| 13 | u16 | jz | pop; if 0: pc = operand + u16 else pc = operand + 2 |
| 14 | | pop | drop top |
| 15 | u8 n | inc | getter n, then setter n(v+1) (statement) |
| 16 | u8 n | preinc | getter n; v+1 replaces it on the stack; setter n(v+1) |
| 17 | u8 n | postinc | getter n (stays); setter n(v+1) |
| 18 | u8 n | dec | getter n, setter n(v−1) |
| 19 | u8 n | predec | like 16 with −1 |
| 1A | u8 n | postdec | like 17 with −1 |
| 1B | | loop_push | pop count onto the loop stack |
| 1C | u16 | loop | top loop counter −= 1; ≠0: pc = operand − u16; =0: pop loop stack, pc = operand + 2 (do-while: count 0 runs 2^32 times) |
| 1D | u8 n, varint×n | pushn | push n varints in order |

Edge semantics kept by the interpreter: stack > 16 or loop stack > 4 is an error (the GBA would corrupt the
interp object); native 0x8C pushes nothing when there is no player object (a game bug); `end` may be reached
with different stack depths ("ragged" ends: `jz L; push 10; L: end` — a node that only sometimes picks the
next node).

## 4. Natives: overview

248 natives: **54 queries** (results, no effect), **107 immediate** effects, **56 queued cutscene
commands** (`enqueue`: block until done outside a `begin_parallel` batch), **31 modal** (always block: fades,
waits, camera scrolls, menus, mini-games, `run_event`). Status: 196 VERIFIED, 52 INFERRED (full table §9;
source `engine/annotations/script_natives.json`, made from three decodes `script_natives_{a,b,c}.json` by
`engine/tools/make_script_natives_annotation.py`, which also resolves name clashes). Engine pointer block
used by natives: `0x08054FBC` → {+0 `0x3003300`, +4 `0x3003898`, +8 `0x30038A4`, +0xC `0x300197C`, +0x10
`0x3001990` entity table, +0x14 GameData `0x3001068`, +0x18 `0x3001C4C`, +0x1C director `0x3002BAC`, +0x20
camera track `0x3002BD8`, +0x24 `0x30019A4` camera/view, +0x28 `0x3002A64`}.

Groups: G/P/L access 0x00–0x0B; character stats 0x0C–0x10 (form-aware energy: SS3/SS gauge or EP); area
0x12–0x14; character/form strings 0x15–0x17; menu result 0x18 (a choice node's selection); `rand` 0x19,
`chance` 0x1A (game PRNG `0x0803FAAC`); story flags 0x1B–0x1E, 0xBA/0xBB; audio 0x1F–0x24; BG layers and
blending 0x25–0x2A; collision rectangles 0x2B–0x2E; HUD 0x2F/0x30; camera 0x31–0x34, 0x6A–0x70; actors
0x35–0x37, 0x3B–0x61, 0xA1–0xA3; props 0x62–0x69; scenes/warps 0x71–0x77; screen effects 0x78–0x7E;
items 0x7F–0x87, 0x92–0x94; waits 0x88/0x89; menus 0x8C/0x8D/0xCE/0xCF; boss bar 0x8F–0x91; party/forms/
skills 0x95–0x9D, 0xA4–0xA8, 0xD0, 0xE9–0xF3, 0xF7; timers 0xAE–0xC1; XP/level 0xC2/0xC3/0xC8/0xDD;
shops 0xCD–0xCF; run_event 0xB5–0xB9; run_script 0xD6–0xD8; equipment effects 0xDF–0xE8, 0xF5;
anti-tamper 0xAB/0xF6.

Actor ids (`0x080001C6`): 0 = player; 1–8 = party characters' current sprites; 9 = `[0x03001BAC]`; ≥ 10 =
sprite id of a live world object (149 Janemba, 154 Super Janemba); < 0 = map placement −(id+1). Commands for
an unresolved actor are silently dropped (a missing camera target can hang a pan forever).

## 5. Catalog (`engine/annotations/script_catalog.json`, 2.4 MB; compact `script_events.json`, `script_inline.json`)

* 522 events, 4506 statically reachable nodes: 2645 text (kind 5), 35 text+args (6), 17 choice menus (3),
  1294 raw bytecode (0), 437 compressed bytecode (4), 78 gotos (7); kinds 1/2 unused. 412 events contain
  bytecode; 10 have no static reference (#5, 36, 123, 268–270, 361, 383, 447, 472).
* Purposes (events / inline): NPC talk 173 / –; cutscene 113 / 31; map event 94 / 342; boss fight 67 / 33;
  dialog 31; quest/flag 18 / 12; engine hooks 13; shop 6; loot 5; spawn conditions – / 115; quest log –
  / 120; item use – / 23; equipment effect – / 15; area selector – / 1. Plus 82 enemy drop scripts.
* Attachments (reference counts): map triggers with inline script 271, map triggers running an event 166,
  fixed event #11 43, area on-enter 167 / on-leave 4, area value-or-script fields 3, spawn conditions
  (actors 482, map objects 74, other 6), NPC talk 191, enemy on-defeat callbacks `{0x0800424F, event}` 130,
  other area callbacks 147, quest log `0x08720564` 40 × 3, item use (supply +0x30) 23, equipment effects
  (+0x28) 15, enemy drop scripts (type +0x18) 82, AI InlineScript node 1 (Vegeta 0x88, `0x0870EA28`) and
  RunScript nodes 4 (#121, #474–476), events from bytecode: 173 run_event, 28 run_script, 78 gotos.
* Engine hooks: #0 death / Lazarus Crystal revive, #1 link-cable disconnect, #2 per-melee-hit, #3 level-up
  (result read back), #4 new-game prologue, #6 save-file chapter label (returns a string id), #8–#10 timers,
  #13/#15/#14 unconfirmed, #497 loot (from drop scripts), #496/#499/#500 loot helpers.
* Boss handovers: on-defeat callbacks run events (#450 Janemba → Super Janemba: Vegeta takes over, the
  player character switches, Super Janemba spawns via `walk_to`/`change_sprite` and `become_map_object`);
  Buu's three-phase fight runs #474 → #475 → #476 from AI RunScript nodes.
* Ids: `sNNN` = event (table index), `sNNN_nK` = bytecode of node K, `inline_0xADDR` = raw bytecode — shared
  with `engine/crates/wf-world` (`maps/<id>/objects.json`).

## 6. Decompiler and runtime (summary; details in `engine/docs/scripting.md`)

* Lifting with exact evaluation order; all 2540 units decompile and **structure** (if/else chains incl.
  jump-threaded `else`, counted loops, ragged ends). Native arities are consistent with the stack depth at
  every join of every unit (a strong check of the 248 pop/push counts).
* Target: Rhai, run-to-completion handlers with an engine-owned action queue; splits (`return next(k)`)
  only where a query follows queued actions, after value-returning menus, after `run_event`, after
  `run_script` of a splitting callee. Output: `scripts/events/sNNN.rhai`, `scripts/inline/inline_0xADDR.rhai`.

## 7. Verification

| | method | result |
|---|---|---|
| (a) game vs interpreter | 7 emulator traces (Janemba → Super Janemba handover, intro cutscene + map change + difficulty menu, NPC talk, item use, loot drops ×2, HFIL arrival): each root replayed in `vm::Thread` with the game's native results; every host call must match native id, args, code offset, G0..G3, P0..P3; game-variable writes and text/choice nodes must match (`wf-script/tests/emulator_traces.rs`) | 92 roots, 824 host calls, 0 mismatches; a perturbed result is detected |
| (b) interpreter vs Rhai | every event, every node-0 `run_script`, every inline + drop script, 4 host seeds, identical effect traces + results (`wf-script-rhai/tests/differential.rs`) | 1673 cases × 4 seeds = 6692 runs, 0 mismatches (2.35 M effects); the 189 runs that error do so on both sides (host-invented node indices, 0-count loops) |
| rollback | snapshot at every block, JSON round-trip, restore, re-run (Rhai and VM) | identical |
| native vs wasm | `engine/experiments/wf-script-dev/wasm-check` (node) | identical digest over 6.7 M effects |
| decompression | 141 compressed units vs the game's decompressor | identical |

Gaps: 52 natives INFERRED only (list in §9: status column); command-class behaviour names partly provisional
(§B3.5); text kinds 1/2 unused, 6 box style DISASM-only; equipment/skill scripts not traced in the emulator
(covered by (b) only); some inline-script call sites not enumerated (AI script-condition node `0x08033308`,
deferred event object `0x08035B94`, callback object `0x0800F62C`, scene object `0x08025A96`); 0x11/0x14/0x29/
0x6D/0x6E/0x9E–0xA0/0xA7/0xC4/0xC5/0xC9/0xDB/0xDC/0xEB/0xF0/0xF4/0xF7 meanings partly unknown.

## 9. Native and setter reference

<!-- GENERATED natives -->
| id | name(args) | pops→push | kind | blocks | status | semantics (engine function) |
|---|---|---|---|---|---|---|
| 0x00 | `get_g0()` | 0→1 | query | no | VERIFIED | Push global G0 (word at 0x03001058). G0..G3 are shared by all scripts, including nested run_script calls; write with setter 0x00. (reads word 0x03001058) |
| 0x01 | `get_g1()` | 0→1 | query | no | VERIFIED | Push global G1 (word at 0x0300105C). G0..G3 are shared by all scripts, including nested run_script calls; write with setter 0x01. (reads word 0x0300105C) |
| 0x02 | `get_g2()` | 0→1 | query | no | VERIFIED | Push global G2 (word at 0x03001060). G0..G3 are shared by all scripts, including nested run_script calls; write with setter 0x02. (reads word 0x03001060) |
| 0x03 | `get_g3()` | 0→1 | query | no | VERIFIED | Push global G3 (word at 0x03001064). G0..G3 are shared by all scripts, including nested run_script calls; write with setter 0x03. (reads word 0x03001064) |
| 0x04 | `get_p0()` | 0→1 | query | no | VERIFIED | Push parameter P0 of the current parameter block ([0x03001030], = interp+0x5C unless the caller passed a block). Nested run_script natives pass the caller's block, so P0..P3 are shared with callees too. (reads [[0x03001030]+0]) |
| 0x05 | `get_p1()` | 0→1 | query | no | VERIFIED | Push parameter P1 of the current parameter block ([0x03001030], = interp+0x5C unless the caller passed a block). Nested run_script natives pass the caller's block, so P0..P3 are shared with callees too. (reads [[0x03001030]+4]) |
| 0x06 | `get_p2()` | 0→1 | query | no | VERIFIED | Push parameter P2 of the current parameter block ([0x03001030], = interp+0x5C unless the caller passed a block). Nested run_script natives pass the caller's block, so P0..P3 are shared with callees too. (reads [[0x03001030]+8]) |
| 0x07 | `get_p3()` | 0→1 | query | no | VERIFIED | Push parameter P3 of the current parameter block ([0x03001030], = interp+0x5C unless the caller passed a block). Nested run_script natives pass the caller's block, so P0..P3 are shared with callees too. (reads [[0x03001030]+12]) |
| 0x08 | `get_l0()` | 0→1 | query | no | VERIFIED | Push global L0 (second bank of 4 globals at 0x03001020; not reset by the runner). (reads word 0x03001020) |
| 0x09 | `get_l1()` | 0→1 | query | no | VERIFIED | Push global L1 (second bank of 4 globals at 0x03001024; not reset by the runner). (reads word 0x03001024) |
| 0x0A | `get_l2()` | 0→1 | query | no | VERIFIED | Push global L2 (second bank of 4 globals at 0x03001028; not reset by the runner). (reads word 0x03001028) |
| 0x0B | `get_l3()` | 0→1 | query | no | VERIFIED | Push global L3 (second bank of 4 globals at 0x0300102C; not reset by the runner). (reads word 0x0300102C) |
| 0x0C | `get_hp()` | 0→1 | query | no | VERIFIED | Push current HP of the active character record GameData+0x4F8+0x68*GameData[0]. (reads s16 rec+0x16) |
| 0x0D | `get_max_hp()` | 0→1 | query | no | VERIFIED | Push max HP of the active character. (reads s16 rec+0x18) |
| 0x0E | `get_energy()` | 0→1 | query | no | VERIFIED | Push the active character's energy bar: if the current form (rec+0x14) has flag bit1 -> SS3 gauge (u16 rec+0x20), else bit0 -> SS gauge (u16 rec+0x1E), else EP (s16 rec+0x1A). (form table 0x086B82EC (24 B) flags byte; rec+0x20/+0x1E/+0x1A) |
| 0x0F | `get_max_ep()` | 0→1 | query | no | VERIFIED | Push max EP of the active character. (reads s16 rec+0x1C) |
| 0x10 | `get_zenie()` | 0→1 | query | no | VERIFIED | Push the Zenie count. (reads u32 GameData+0x4F4 (0x0300155C)) |
| 0x11 | `get_variant_byte()` | 0→1 | query | no | INFERRED | Push byte 0x03002BB0 (also written by setter 0x11). Read by the player skill code at 0x0801C416 (selects a 0/1/other branch when firing); purpose not decoded. Unused by table scripts. (reads u8 0x03002BB0) |
| 0x12 | `get_world()` | 0→1 | query | no | VERIFIED | Push the current world (area group) index. (reads u8 [0x0300197C]+0 (area manager)) |
| 0x13 | `get_map()` | 0→1 | query | no | VERIFIED | Push the current map index within the world. (reads u8 [0x0300197C]+1) |
| 0x14 | `get_map_variant()` | 0→1 | query | no | INFERRED | Push the area manager byte +2 (map variant / entrance index; 1 in every captured state). Unused by table scripts. (reads u8 [0x0300197C]+2) |
| 0x15 | `get_character()` | 0→1 | query | no | VERIFIED | Push the active character index (0 = Goku ... 8; character record order). (reads u8 GameData+0) |
| 0x16 | `get_form_name()` | 0→1 | query | no | VERIFIED | Push a POINTER to the active form's name string (e.g. "Goku"); used as a text argument for dialog formatting. (form table 0x086B82EC +2 (u16 string id) -> string table 0x0806BCE8) |
| 0x17 | `get_form_label()` | 0→1 | query | no | VERIFIED | Push a POINTER to the active form's upper-case label (e.g. "GOKU", the dialog speaker label). (form table +4 (u16 string id) -> string table) |
| 0x18 | `get_menu_result()` | 0→1 | query | no | INFERRED | Push the last menu/choice result (word 0x03002A60; menu code at 0x0800AED8/0x0800B342/0x08030B76 writes it, -1 = cancelled). (reads word 0x03002A60) |
| 0x19 | `rand(n)` | 1→1 | query | no | VERIFIED | Replace n with a uniform random integer in [0, n) from the game PRNG (state 0x03004A5C). (0x0803FACC rand(n) = ((next()&0xFFFF)*n)>>16) |
| 0x1A | `chance(percent)` | 1→1 | query | no | VERIFIED | Replace percent with 1 with probability percent/100 (else 0). Consumes one PRNG step. (0x0803FADE chance(p) = (next()&0xFFFF) < p, p = percent*0x28F5D>>8) |
| 0x1B | `get_flag(flag)` | 1→1 | query | no | VERIFIED | Replace flag id with 1 if the story flag is set, else 0. (0x08004364(GameData, flag): bit (flag&7) of GameData+0x55+(flag>>3)) |
| 0x1C | `not_flag(flag)` | 1→1 | query | no | VERIFIED | Replace flag id with 1 if the story flag is CLEAR. (1 - 0x08004364(GameData, flag)) |
| 0x1D | `set_flag(flag)` | 1→0 | immediate | no | VERIFIED | Set story flag. (0x08004380: GameData+0x55+(f>>3) \|= 1<<(f&7)) |
| 0x1E | `clear_flag(flag)` | 1→0 | immediate | no | VERIFIED | Clear story flag. (0x08004396: bit clear) |
| 0x1F | `play_sfx(sfx)` | 1→0 | immediate | no | VERIFIED | Play sound effect sfx (0..63) at full volume on the first free of 16 SFX voices (dropped if none free). (PlaySfx 0x0803E378(voices 0x03003898+0x5C4, SFX table 0x083192A8+12*sfx, vol 0x40, pan 0)) |
| 0x20 | `play_sfx_volume(sfx, volume)` | 2→0 | immediate | no | VERIFIED | Play sound effect at volume (0..64). (PlaySfx 0x0803E378(..., vol=volume, pan 0)) |
| 0x21 | `stop_sfx(sfx)` | 1→0 | immediate | no | VERIFIED | Stop the (most recently allocated) voice playing sfx. (0x0803E302: free the voice playing that SFX entry) |
| 0x22 | `stop_all_sfx()` | 0→0 | immediate | no | INFERRED | Stop all 16 SFX voices. (StopAllSfx 0x0803E2EC) |
| 0x23 | `play_music(song)` | 1→0 | immediate | no | VERIFIED | Start song (0..52) unless that song is already playing (player+5 == 1 and player+0xC == header). (PlaySong 0x0803F110(player 0x030038A4, song table 0x083BB78C+20*song)) |
| 0x24 | `stop_music()` | 0→0 | immediate | no | VERIFIED | Stop the music sequencer. (player 0x030038A4 +5 = 0) |
| 0x25 | `show_bg(layer)` | 1→0 | immediate | no | VERIFIED | Show background layer 0..3. (0x080096AC: DISPCNT \|= 0x100<<layer (shadow scene 0x030019A4+0xA2)) |
| 0x26 | `hide_bg(layer)` | 1→0 | immediate | no | VERIFIED | Hide background layer 0..3. (0x08009692: DISPCNT &= ~(0x100<<layer)) |
| 0x27 | `set_bg_priority(layer, priority)` | 2→0 | immediate | no | VERIFIED | Set a background layer's priority (BGxCNT low bits). (0x080096C4: scene+0xB4+4*layer = priority; BGxCNT = u16 0x080716D8[mode][layer] + priority) |
| 0x28 | `set_blend_targets(layer, mask)` | 2→0 | immediate | no | VERIFIED | Set whether a BG layer is a first/second alpha-blend target. (0x080096EA: BLDCNT bit (1<<layer) = mask&1 (1st target), bit (0x100<<layer) = mask&2 (2nd target)) |
| 0x29 | `entity_call(slot, a, b)` | 3→0 | immediate | cond. | INFERRED | Call virtual method +0xC of the object in slot `slot` of the table at 0x03001990 with the pair (a, b). Unused by table scripts; with garbage arguments it did not return within a frame. (obj = word[[0x08054FBC+0x10]=0x03001990 + 4*slot]; obj->vtable[+0xC](obj, &{a, b})) |
| 0x2A | `set_blend_alpha(eva_percent, evb_percent)` | 2→0 | immediate | no | VERIFIED | Set alpha-blend coefficients from percentages (100 -> 16/16). (0x08009728: BLDALPHA = (eva*0x290>>12) \| (evb*0x290>>12)<<8) |
| 0x2B | `open_area(x0, y0, x1, y1)` | 4→0 | immediate | no | VERIFIED | Set the bits of the 8x8-tile rectangle [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3) (pixels) in the 256x256 collision bitmap(s) walk (+0xA8) and shot (+0xB0); bit 1 = passable, so this opens the area (doors, barriers). The third grid's purpose is not decoded. (0x08009930(rect, grid, 1) on scene 0x030019A4 walk (+0xA8) and shot (+0xB0)) |
| 0x2C | `block_area(x0, y0, x1, y1)` | 4→0 | immediate | no | VERIFIED | Clear the bits of the 8x8-tile rectangle [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3) (pixels) in the 256x256 collision bitmap(s) walk (+0xA8) and shot (+0xB0); bit 1 = passable, so this blocks the area (doors, barriers). The third grid's purpose is not decoded. (0x08009930(rect, grid, 0) on scene 0x030019A4 walk (+0xA8) and shot (+0xB0)) |
| 0x2D | `open_area_alt(x0, y0, x1, y1)` | 4→0 | immediate | no | VERIFIED | Set the bits of the 8x8-tile rectangle [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3) (pixels) in the 256x256 collision bitmap(s) third grid (+0xAC); bit 1 = passable, so this opens the area (doors, barriers). The third grid's purpose is not decoded. (0x08009930(rect, grid, 1) on scene 0x030019A4 third grid (+0xAC)) |
| 0x2E | `block_area_alt(x0, y0, x1, y1)` | 4→0 | immediate | no | VERIFIED | Clear the bits of the 8x8-tile rectangle [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3) (pixels) in the 256x256 collision bitmap(s) third grid (+0xAC); bit 1 = passable, so this blocks the area (doors, barriers). The third grid's purpose is not decoded. (0x08009930(rect, grid, 0) on scene 0x030019A4 third grid (+0xAC)) |
| 0x2F | `show_hud()` | 0→0 | immediate | no | VERIFIED | Show the status HUD (slides in) if hidden. (0x080046F4: GameData+1 \|= 2; creates HUD object 0x0800B854 (vt 0x08049FEC) in world) |
| 0x30 | `hide_hud()` | 0→0 | immediate | no | VERIFIED | Hide the status HUD (slides out). First instruction of most cutscene scripts. (0x08004764: GameData+1 &= ~2; broadcast message 0x15 (HUD removes itself)) |
| 0x31 | `camera_center_on(actor, speed)` | 2→0 | modal | yes | VERIFIED | Scroll the camera until the actor's bounding-box centre is mid-screen, at `speed` px/frame on the dominant axis (frames = max(\|dx\|,\|dy\|)/speed + 1). Blocks until done. (bbox 0x0800FDFA -> center - (120,80); camera mover 0x08015B68/0x08007858 (vt 0x0804AB24); run_mode 0x0803D8A0) |
| 0x32 | `camera_scroll_to(x, y, speed)` | 3→0 | modal | yes | VERIFIED | Scroll the camera's top-left corner to (x, y) world pixels at `speed` px/frame on the dominant axis. Blocks. (camera mover 0x08015B68 / 0x08007858; run_mode 0x0803D8A0) |
| 0x33 | `camera_follow(actor)` | 1→0 | immediate | no | VERIFIED | Make the camera track the actor. (adds follow object (vt 0x0804D300, or 0x0804E324 when scene+0x90 != 0) to camera controller 0x03002BD8 (0x08021094)) |
| 0x34 | `camera_release()` | 0→0 | immediate | no | INFERRED | Remove the camera follow behaviour (camera stays where it is). (0x080210B6(camera controller 0x03002BD8)) |
| 0x35 | `spawn_actor(sprite, x, y)` | 3→0 | immediate | no | VERIFIED | Create a cutscene actor (NPC class) of sprite id `sprite` (>= 10: sprite table 0x086B6BD8) at world pixel (x, y), facing down. Later natives address it by the same sprite id. (descriptor = 0x08000158(sprite); actor = 0x0801111C(0, desc, &{s16 x, s16 y}) (vt 0x0804CEF4); world 0x03001C4C add 0x0801191A) |
| 0x36 | `spawn_map_object(index)` | 1→0 | immediate | no | VERIFIED | Instantiate object `index` of the current area's object list (enemies, NPCs, triggers placed by the map) now. (0x08009108(area manager 0x0300197C, index): entry = area.obj_table(+0x14)[index]; obj = entry->fn(entry); world add) |
| 0x37 | `count_actors(actor)` | 1→1 | query | no | VERIFIED | Replace id with the number of live world objects with that sprite (or map-object index for negative ids). Used as "is X present". (id >= 0: world 0x08011CCA(kind 5, descriptor) ; id < 0: kind 7, map object -(id+1)) |
| 0x38 | `begin_parallel()` | 0→0 | immediate | no | VERIFIED | Start a batch: following actor commands are queued on their actors' tracks and run concurrently instead of blocking one by one. (0x08019348: director 0x03002BAC +0 = 1) |
| 0x39 | `wait_for_actor(actor)` | 1→0 | immediate | no | INFERRED | Restrict the next end_parallel to wait only for the listed actors' tracks (other tracks keep running in the background). (appends the actor object to the group list [0x03001004+4] (vt 0x0804DEEC, created on demand)) |
| 0x3A | `end_parallel()` | 0→0 | modal | yes | VERIFIED | Close the batch and block until all queued tracks (or only the wait_for_actor ones) are empty. (group list set: run_mode(group); else 0x0801934E: run_mode(wait-all 0x0804C0C4); director +0 = 0) |
| 0x3B | `fire_wave_beam(actor, dir, frames)` | 3→0 | enqueue | cond. | INFERRED | Actor faces dir (sets actor+0x10), plays the firing animation (raw anim 0x12) and after 16 ticks spawns the wavy pink beam effect 0x080286D4 at actor + offset table 0x0877C430[dir]; done after `frames` more checks, then idle. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C7D8; director enqueue 0x0801936A) |
| 0x3C | `play_anim(actor, anim)` | 2→0 | enqueue | cond. | VERIFIED | Play animation `anim` (script anim id, mapped through 0x080725E8+0x88 to the sprite's animation list; actor vtable +0x28) and stay on the track until it finishes (actor vtable +0x34); anim 0 completes immediately. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804D818; director enqueue 0x0801936A) |
| 0x3D | `set_anim(actor, anim)` | 2→0 | enqueue | cond. | VERIFIED | Switch the actor to animation `anim` (actor vtable +0x28) and complete immediately (no wait). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804E0E8; director enqueue 0x0801936A) |
| 0x3E | `fire_ball_at(actor, x, y)` | 3→0 | enqueue | cond. | VERIFIED | Actor turns toward (x, y) (4-way from the angle), plays the throw animation (raw anim 0x14) and at tick 8 launches a pink energy ball 0x0802E444 toward the point; done after 24 ticks once the ball is gone. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C414; director enqueue 0x0801936A) |
| 0x3F | `puff(actor)` | 1→0 | enqueue | cond. | VERIFIED | Spawn the dust-cloud "poof" effect (0x0801952C kind 0x37) at the actor's bounding-box centre (appear/vanish smoke). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804B2B4 (offsets 0,0); director enqueue 0x0801936A) |
| 0x40 | `puff_offset(actor, dx, dy)` | 3→0 | enqueue | cond. | VERIFIED | Spawn the dust-cloud effect at the actor's bbox centre + (dx, dy). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804B2B4; director enqueue 0x0801936A) |
| 0x41 | `shoot_at_actor(actor, target, speed)` | 3→0 | enqueue | cond. | INFERRED | Actor plays the shoot animation (raw anim 3) and fires a ki blast (0x08027CBC, effect 0x0871370C) toward the target actor's bbox centre at `speed` (px per 100 frames, see 0x080192CE); done when the blast has flown its distance (blast removed). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DCE0; director enqueue 0x0801936A) |
| 0x42 | `shoot_at(actor, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Like shoot_at_actor but toward point (x, y). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DCF4; director enqueue 0x0801936A) |
| 0x43 | `hit_flash(actor)` | 1→0 | enqueue | cond. | VERIFIED | Send message 0x0C (object vt 0x0804A7A0) to the actor: it blinks red like a hit reaction. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804B918; director enqueue 0x0801936A) |
| 0x44 | `walk_to(actor, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Walk (walk animation, facing the dominant direction) in a straight line to world pixel (x, y) at `speed` px per 100 frames (frames = dist*100/speed + 1); snaps to the target at the end. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804D0A4 (built by 0x0805082C, mode 0x0F); director enqueue 0x0801936A) |
| 0x45 | `jump_to(actor, x, y, air_frames)` | 4→0 | enqueue | cond. | VERIFIED | Jump to (x, y): 8-frame crouch (pose msg 0xE,0), airborne for air_frames with linear motion and a parabolic arc, then 8-frame landing; total air_frames + 16. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804AE5C; director enqueue 0x0801936A) |
| 0x46 | `fire_beam(actor, dir, frames)` | 3→0 | enqueue | cond. | VERIFIED | Actor faces dir, charges (24 ticks) and fires a Kamehameha-style beam (0x080206B8, level definition 0x0871DA88, per-direction table 0x0877B200) for `frames`, then idles and removes the beam. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C428; director enqueue 0x0801936A) |
| 0x47 | `face_actor(actor, target)` | 2→0 | enqueue | cond. | VERIFIED | Turn the actor toward the target actor (dominant axis of the position difference); instant. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804AE70; director enqueue 0x0801936A) |
| 0x48 | `puff_at(actor, x, y)` | 3→0 | enqueue | cond. | VERIFIED | Spawn the dust-cloud effect at world pixel (x, y) (actor only selects the track). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C43C; director enqueue 0x0801936A) |
| 0x49 | `glide_to(actor, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Move linearly to (x, y) at `speed` px per 100 frames WITHOUT changing animation or facing (floating, flying, being carried). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804AE84; director enqueue 0x0801936A) |
| 0x4A | `actor_wait(actor, frames)` | 2→0 | enqueue | cond. | VERIFIED | Idle on the actor's track for `frames` frames (counter decremented once per frame, done at 0). With actor 0 outside a batch this is the plain "wait N frames". Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804AB58; director enqueue 0x0801936A) |
| 0x4B | `queue_sfx(actor, sfx)` | 2→0 | enqueue | cond. | INFERRED | Play sound effect sfx (volume 0x40) when this track reaches the command (sequenced sound). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C450; director enqueue 0x0801936A) |
| 0x4C | `screen_flash(actor, r, g, b, frames)` | 5→0 | enqueue | cond. | VERIFIED | Fade the whole palette toward colour (r, g, b) (0..255) and back over `frames` (min 2; peak at frames/2) via the palette fade routine 0x08007074. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C800; director enqueue 0x0801936A) |
| 0x4D | `remove_actor(actor)` | 1→0 | enqueue | cond. | VERIFIED | Delete the actor from the world (broadcast message 0x12 about it first). If it was the current talk target (0x03001BAC), sets 0x03002A58 = 1. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804AE9C; director enqueue 0x0801936A) |
| 0x4E | `change_sprite(actor, sprite)` | 2→0 | enqueue | cond. | VERIFIED | Replace the actor with a new cutscene actor of sprite id `sprite` at the same position and facing; the remaining queued commands move to the new actor (0x080192BA). Used for transformations (e.g. Janemba -> Super Janemba). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804B2C8; director enqueue 0x0801936A) |
| 0x4F | `become_map_object(actor, index)` | 2→0 | enqueue | cond. | INFERRED | Replace the cutscene actor with map object `index` of the current area (0x08009124) at the actor's position/facing; hands a cutscene NPC over to the real enemy/NPC (boss fight start). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DEA0; director enqueue 0x0801936A) |
| 0x50 | `move_to(actor, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Like walk_to but with movement mode 2 instead of 0x0F (identical path and timing; the mode byte cmd+0x24 selects the animation/state used while moving). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804D0A4 (0x0805082C, mode 2); director enqueue 0x0801936A) |
| 0x51 | `set_pose(actor, anim, frame)` | 3→0 | enqueue | cond. | VERIFIED | Hold animation `anim` at sub-frame `frame` (actor vtable +0x30: static pose state 0x03001B00, actor+0x11 = frame). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DD1C; director enqueue 0x0801936A) |
| 0x52 | `tint(actor, r, g, b, a)` | 5→0 | enqueue | cond. | VERIFIED | Recolour the actor with the nearest (squared distance) of 20 preset palettes in table 0x08752EEC (8-byte entries: RGBA key, palette pointer); (255,255,255,x) = white flash, (0,0,0,0)-like keys restore. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804D4A8 (built by 0x0802B8F0); director enqueue 0x0801936A) |
| 0x53 | `face(actor, dir)` | 2→0 | enqueue | cond. | VERIFIED | Set the actor's facing (actor+0x10): 0 down, 1 up, 2 left, 3 right. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DEC8; director enqueue 0x0801936A) |
| 0x54 | `set_actor_speed(actor, value)` | 2→0 | enqueue | cond. | INFERRED | Write value<<10 to actor+0x1AC (when scene 0x030019A4+0x90 != 0) or actor+0x180; INFERRED to be a movement speed. Unused. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C814; director enqueue 0x0801936A) |
| 0x55 | `set_attachment(actor, kind, variant, dx, dy)` | 5→0 | enqueue | cond. | INFERRED | Attach the record matching (kind, variant) from the 4-entry table 0x086B759C (0x08013E38; none -> detach) to the actor (0x0800ECB0, actor+0x5C) and set its offset actor+0x178 = (dx, dy). Scripts mostly pass zeros (detach). Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804BF14; director enqueue 0x0801936A) |
| 0x56 | `set_actor_byte8(actor, value)` | 2→0 | enqueue | cond. | INFERRED | Write byte actor+8 (0x38 for every actor seen; the attack commands also set it to 0x38). Unused by scripts. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804DD30; director enqueue 0x0801936A) |
| 0x57 | `shake_screen(actor, intensity, frames)` | 3→0 | enqueue | cond. | VERIFIED | Shake the camera (object 0x0804A720 via 0x0800797C; amplitude from intensity, period (100-intensity)/16+1) and keep the track busy for `frames`. Enqueues a command object on the actor's cutscene track (actor+0x18C list, director 0x03002BAC via 0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. If the actor id resolves to no object the command is destroyed without effect. (command vtable 0x0804C828; director enqueue 0x0801936A) |
| 0x58 | `show_item_get(actor, item)` | 2→0 | enqueue | cond. | INFERRED | Item-get presentation on ACTOR without changing the inventory: start plays SFX 45 (table 0x083192A8 entry 0x87*4/12), puts the actor in its hold-up pose (actor vt+0x14 / 0x0800F53C) and spawns the item icon above it (item >= 0: supply table 0x083BD2F8 icon; item < 0: equipment -item, table 0x086F82EC); done after 60 frames, then the pose is released (0x0800F558 / vt+0x28). (0x0801936A enqueue cmd vt 0x0804C83C on actor track) |
| 0x59 | `play_anim_frames(actor, anim, frame_frames, loops, wait, loop_start, last_frame)` | 7→0 | enqueue | cond. | VERIFIED | Play actor animation ANIM (state id = table 0x08072670[anim], identity table) frame by frame: each cel lasts FRAME_FRAMES frames, cels 0..LAST_FRAME; after the last cel restart at LOOP_START; LOOPS > 0 = number of passes (0 = forever). If WAIT != 0 the command completes when the passes are done (then the actor returns to idle, 0x0800EE78); if WAIT == 0 the command completes immediately and the animation keeps running. (0x0801936A enqueue cmd vt 0x0804E268; controller 0x0804DE58 via 0x0800ECDA/0x0800ED18) |
| 0x5A | `emote_wait(actor, emote)` | 2→0 | enqueue | cond. | VERIFIED | Show emote balloon EMOTE (sweat drop, anger mark, ...) above ACTOR and complete when the balloon animation ends (~23 frames). (0x0801936A enqueue cmd vt 0x0804C850 -> 0x08016304 (balloon object, graphics table 0x086B8128[emote])) |
| 0x5B | `emote(actor, emote)` | 2→0 | enqueue | cond. | VERIFIED | Same balloon as emote_wait but the command completes immediately (balloon plays on its own). (0x0801936A enqueue cmd vt 0x0804DD44 -> 0x08016304) |
| 0x5C | `fire_beam_alt(actor, dir, frames)` | 3→0 | enqueue | cond. | VERIFIED | ACTOR takes its firing pose (state 0x38) facing DIR and emits an energy beam effect from the table offset for that direction; after 24 frames the beam is extended (0x080207D2); command completes after FRAMES frames, beam removed (0x0802082A), actor back to idle. (0x0801936A enqueue cmd vt 0x0804DD58 -> effect 0x080206B8 (offsets table 0x0877B254[dir], 12 bytes)) |
| 0x5D | `set_pos(actor, x, y)` | 3→0 | enqueue | cond. | VERIFIED | Teleport ACTOR to (X, Y) px (actor+0x1A0/+0x1A4 = v<<8, around actor vt+0x3C/+0x40 refresh); completes at once. (0x0801936A enqueue cmd vt 0x0804B940) |
| 0x5E | `fire_blast(actor, dir, frames)` | 3→0 | enqueue | cond. | VERIFIED | Like fire_beam but with the wide flame/ki-wave effect (graphic 0x08725038). (0x0801936A enqueue cmd vt 0x0804C864 -> effect 0x080206B8 (offsets table 0x0877B2A8[dir])) |
| 0x5F | `walk_to_actor(actor, target, dx, dy)` | 4→0 | enqueue | cond. | VERIFIED | Walk ACTOR to the position of actor TARGET offset by (DX, DY) px at 1 px/frame; completes on arrival. (0x0801936A enqueue cmd built by 0x08015AF0(0, target_obj, &{dx,dy})) |
| 0x60 | `walk_to(actor, x, y)` | 3→0 | enqueue | cond. | VERIFIED | Walk ACTOR to (X, Y) px at speed 100 (1 px/frame); completes on arrival. (0x0801936A enqueue cmd built by 0x0805082C(0, &{x,y}, 1, 100)) |
| 0x61 | `walk_to_speed(actor, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Walk ACTOR to (X, Y) at SPEED/100 px per frame; completes on arrival. (0x0801936A enqueue cmd built by 0x0805082C(0, &{x,y}, 1, speed)) |
| 0x62 | `prop_poof(prop, vanish)` | 2→0 | enqueue | cond. | VERIFIED | Play the 8-frame-per-step dust/poof frames (table 0x087581FC, LZ-inflated into the prop OBJ VRAM) on PROP: 5 steps normally; with VANISH != 0, 8 steps and the prop is hidden at the end (+0x1C = 0). (0x0801936A enqueue cmd vt 0x0804E0FC on prop track) |
| 0x63 | `prop_hide(prop)` | 1→0 | enqueue | cond. | VERIFIED | Hide map prop PROP (prop+0x1C = 0); completes at once. (0x0801936A enqueue cmd vt 0x0804DEB4 (+8 = 0) on prop track) |
| 0x64 | `prop_show(prop)` | 1→0 | enqueue | cond. | VERIFIED | Show map prop PROP (prop+0x1C = 1); completes at once. (0x0801936A enqueue cmd vt 0x0804DEB4 (+8 = 1) on prop track) |
| 0x65 | `prop_effect_at(prop, x, y)` | 3→0 | enqueue | cond. | INFERRED | Spawn effect object kind 0x37 at (X, Y) (after reserving 0x400 bytes in pool 0x0874EA44), sequenced on PROP's track; completes at once. (0x0801936A enqueue cmd vt 0x0804C43C on prop track -> 0x0801952C(0, &{x,y}, 0x37)) |
| 0x66 | `prop_move_to(prop, x, y, speed)` | 4→0 | enqueue | cond. | VERIFIED | Move PROP linearly to (X, Y) px at SPEED/100 px per frame (frame count from 0x080192CE); completes on arrival. (0x0801936A enqueue cmd vt 0x0804B92C on prop track; step math 0x080192CE) |
| 0x67 | `prop_jump_to(prop, x, y)` | 3→0 | enqueue | cond. | VERIFIED | Move PROP to (X, Y) along a jump arc in 44 frames (vx = dx*0x5D3>>8, vy0 = dy*0x5D3-0xBA60>>8 - 0x200, gravity +0x20/frame, 1/256 px units); completes on landing. (0x0801936A enqueue cmd vt 0x0804DD08 on prop track) |
| 0x68 | `prop_wait(prop, frames)` | 2→0 | enqueue | cond. | INFERRED | Wait FRAMES frames on PROP's track (sequencing inside a batch). (0x0801936A enqueue cmd vt 0x0804AB58 (countdown 0x080159F5) on prop track) |
| 0x69 | `prop_set_pos(prop, x, y)` | 3→0 | enqueue | cond. | INFERRED | Teleport PROP to (X, Y) (moves +0x24/+0x28 and its rect +0x2C by the delta); completes at once. (0x0801936A enqueue cmd vt 0x0804CD60 on prop track) |
| 0x6A | `camera_follow_wait(actor, frames)` | 2→0 | enqueue | cond. | INFERRED | Make the camera follow ACTOR (same follow objects as native 0x33: 0x0804E324 or 0x0804D300 via 0x08021094), then wait FRAMES frames. (0x0801936A enqueue cmd vt 0x0804DCCC on the global track [0x03002BD8]) |
| 0x6B | `camera_pan_to(x, y, speed)` | 3→0 | enqueue | cond. | VERIFIED | Pan the camera so its top-left corner moves to (X, Y) px at SPEED/100 px per frame (frame count fixed at start by 0x080192CE); the result is clamped to the map bounds; completes after the computed frames. Camera following is stopped. (0x0801936A enqueue cmd built by 0x08021174(0, &{x,y}, speed) on the global track) |
| 0x6C | `camera_release_wait(frames)` | 1→0 | enqueue | cond. | VERIFIED | Stop camera following (0x080210B6, same as native 0x34), then wait FRAMES frames. (0x0801936A enqueue cmd vt 0x0804B2A0 on the global track; 0x080210B6) |
| 0x6D | `camera_set_param8(v)` | 1→0 | enqueue | cond. | INFERRED | Stop following; camera+8 = V<<10 and derive camera+0x10 from it with float math (constants 0x3BB7E762, 0x4222F9DB, 0x44800000), then refresh (0x0802ED98). Purpose unknown (third camera coordinate / depth). (0x0801936A enqueue cmd vt 0x0804C7EC on the global track; soft-float 0x08054ADC.. on camera 0x030019A4) |
| 0x6E | `camera_set_param12(v)` | 1→0 | enqueue | cond. | INFERRED | Stop following; camera+0xC = V<<10; refresh (0x0802ED98). (0x0801936A enqueue cmd vt 0x0804E0D4 on the global track) |
| 0x6F | `camera_set(x, y)` | 2→0 | enqueue | cond. | VERIFIED | Stop following and put the camera top-left at (X, Y) px (camera+0/+4 = v<<10; through 0x080094F0 when the camera mode flag camera+0x90 is clear); completes at once. (0x0801936A enqueue cmd vt 0x0804D480 on the global track; camera 0x030019A4) |
| 0x70 | `camera_set3(x, y, z)` | 3→0 | enqueue | cond. | VERIFIED | Stop following; camera+0/+4/+8 = X/Y/Z << 10; refresh. (0x0801936A enqueue cmd vt 0x0804D494 on the global track) |
| 0x71 | `save_field_on_next_scene()` | 0→0 | immediate | no | VERIFIED | Arm a flag so that the next load_scene / load_scene_* (0x72..0x74) first snapshots the current field (0x08009196: world descriptor -> 0x03001968, camera, map objects, entities) so that return_to_field (0x75) can restore it. (writes 0x03001004+8 = 1) |
| 0x72 | `load_scene_fade(world, map, variant, x, y)` | 5→0 | modal | yes | VERIFIED | Fade out (64 frames), load map WORLD/MAP as a cutscene scene (no player object), layout VARIANT (0x080089A0), camera top-left at (X, Y), running the map's load scripts (map desc +0x34, +0x30) and spawning its objects; fade back in over 16 frames. Saves the field first if save_field_on_next_scene was armed. (fade out 0x08050714(0,64)+0x0803D8A0; 0x08009030(world_obj 0x0300197C, world, map, variant, &{x,y}, 1, 1); fade-in mode 0x0804C588 (16 frames)) |
| 0x73 | `load_scene(world, map, variant, x, y)` | 5→0 | immediate | no | VERIFIED | Load map WORLD/MAP as a scene immediately (no fades; scripts wrap it in fade_out/fade_in), same as load_scene_fade otherwise (last flag 0). (0x08009030(world_obj, world, map, variant, &{x,y}, 1, 0)) |
| 0x74 | `load_scene_fade_noobj(world, map, variant, x, y)` | 5→0 | modal | yes | INFERRED | As load_scene_fade but with the first flag 0: the loop that spawns the map's conditional objects (0x08004784 condition test per map object) is skipped. (like 0x72 but 0x08009030(..., 0, 1)) |
| 0x75 | `return_to_field()` | 0→0 | modal | yes | VERIFIED | Fade out (16), restore the field saved by save_field_on_next_scene (world descriptor, camera, objects, entities; re-runs the map's +0x34 script), fade in (16). (fade out 16; 0x080091B8(world_obj) restore snapshot + run map script +0x34; fade-in mode 16) |
| 0x76 | `warp_fade(world, map, variant, x, y)` | 5→0 | modal | yes | VERIFIED | Fade out, move the PLAYER to map WORLD/MAP at (X, Y) facing DIR (camera centred: x-112, y-76), fade in; returns when visible again. NOTE: the 3rd argument is the map VARIANT (layout), not a facing (cross-checked by engine/crates/wf-world warps). (0x03002A58 = 1; player vt+0x28(0); fade out 64; 0x08008D44(world_obj, world, map, &{x,y}, dir, 1); fade-in mode 0x080495B4 (4)) |
| 0x77 | `warp(world, map, variant, x, y)` | 5→0 | immediate | no | VERIFIED | Move the PLAYER to map WORLD/MAP at (X, Y) facing DIR immediately (no fades; scripts wrap it). NOTE: the 3rd argument is the map VARIANT (layout), not a facing (cross-checked by engine/crates/wf-world warps). (0x03002A58 = 1; player vt+0x28(0); 0x08008D44(world_obj, world, map, &{x,y}, dir, 0)) |
| 0x78 | `fade_out(frames)` | 1→0 | modal | yes | VERIFIED | Fade the screen to black over FRAMES frames. (0x08050714(0, frames) + 0x0803D8A0) |
| 0x79 | `fade_in(frames)` | 1→0 | modal | yes | VERIFIED | Fade back in from black over FRAMES frames. (mode vt 0x0804C588 (+8 = frames)) |
| 0x7A | `fade_out_white(frames)` | 1→0 | modal | yes | VERIFIED | Fade the screen to white over FRAMES frames. (mode vt 0x0804BBD4 built by 0x080176AC (0x40C bytes)) |
| 0x7B | `fade_in_white(frames)` | 1→0 | modal | yes | VERIFIED | Fade back in from white over FRAMES frames. (mode vt 0x0804C5B8) |
| 0x7C | `mosaic_out(frames)` | 1→0 | modal | yes | VERIFIED | Pixelate (mosaic) the screen over FRAMES frames, ending black (palette RAM zeroed). (mode vt 0x0804A5B4 (MOSAIC on BG0..3, palettes cleared)) |
| 0x7D | `mosaic_in(frames)` | 1→0 | modal | yes | VERIFIED | Reverse of mosaic_out over FRAMES frames. (mode vt 0x0804A070) |
| 0x7E | `flash_white(frames)` | 1→0 | modal | yes | VERIFIED | White flash: fade to white and back, ~2*FRAMES+1 frames in total. (mode vt 0x0804A9D8 (frames clamped to >= 2)) |
| 0x7F | `item_count(item)` | 1→1 | query | no | VERIFIED | Number of supply item ITEM in the inventory. (reads u8 GameData(0x03001068)+2+item) |
| 0x80 | `give_item(item)` | 1→0 | enqueue | cond. | VERIFIED | Add one supply item ITEM and play the item-get presentation on the player (60 frames, SFX 45). (GameData+2+item += 1 (saturates at 255); enqueue cmd vt 0x0804C83C on the player track) |
| 0x81 | `add_item(item, n)` | 2→0 | immediate | no | VERIFIED | Add N of supply item ITEM silently. (GameData+2+item += n (saturates at 255)) |
| 0x82 | `take_item(item)` | 1→0 | immediate | no | VERIFIED | Remove one supply item ITEM (no underflow check). (GameData+2+item -= 1 (u8 wrap, no check)) |
| 0x83 | `take_items(item, n)` | 2→0 | immediate | no | VERIFIED | Remove N of supply item ITEM (no underflow check). (GameData+2+item -= n (u8 wrap)) |
| 0x84 | `drop_zenie(size)` | 1→0 | immediate | no | VERIFIED | Drop a zenie coin of SIZE (0..3 = 5/25/50/100 Z, coin defs 0x086FC250) at the loot position 0x03001010 (set by the dying enemy). (0x08018346(&loot_pos 0x03001010, size)) |
| 0x85 | `drop_item(item)` | 1→0 | immediate | no | VERIFIED | Drop supply item ITEM (table 0x083BD2F8) at the loot position, only if enough free memory (0x080116FC >= 0x1000) and object slots remain. (0x0800D30E(0, item, &loot_pos, ...) added to registry 0x03001C4C) |
| 0x86 | `drop_equipment(item)` | 1→0 | immediate | no | VERIFIED | Drop equipment ITEM (table 0x086F82EC) at the loot position (same capacity checks). (as 0x85 with -item) |
| 0x87 | `drop_zenie_at(x, y, size)` | 3→0 | immediate | no | INFERRED | Drop a zenie coin of SIZE at (X, Y). (0x08018346(&{x,y}, size, zero velocity 0x08054FF0)) |
| 0x88 | `wait(frames)` | 1→0 | modal | yes | VERIFIED | Suspend the script for FRAMES frames (the world keeps running). (mode vt 0x08049978 (+4 = frames)) |
| 0x89 | `wait_button()` | 0→0 | modal | yes | VERIFIED | Suspend until the A button is pressed. (mode vt 0x0804CF64) |
| 0x8A | `spawn_map_object_force(index)` | 1→0 | immediate | no | INFERRED | Spawn object INDEX of the current map's object list unconditionally (ignores its story-flag condition). (0x08009172(world_obj, index): current map desc+0x10 entry index -> factory -> object list [0x080716A0+0x10]) |
| 0x8B | `refresh_map_objects()` | 0→0 | immediate | no | INFERRED | Re-evaluate the current map's conditional objects (after story flags changed, e.g. a chest opened or an NPC left). (0x08008AA0(world_obj, 0): clear registry (0x08012900), reload layout (0x08009670), respawn map objects whose condition 0x08004784 holds; 0x03002A58 = 1 if [0x03002A64]+0xCC) |
| 0x8C | `save_menu(flag)` | 1→1 | modal | yes | VERIFIED | Open the Save Game screen (FLAG stored at mode+0x2F8, meaning unknown); pushes exit code - 1. If there is no player object nothing happens and NOTHING is pushed (stack imbalance in the original). (quick fade 0x0805058C; mode vt 0x08049504 (0x304 bytes, base 0x08005460(obj,7,0xD,0)); result pushed; restore + fade in 0x08050568) |
| 0x8D | `character_menu()` | 0→0 | modal | yes | VERIFIED | Open the Switch Character screen; if the character changed, refresh the player sprite/controller. (mode 0x0802E83C(0, player); on result 2 the player sprite is rebuilt for the new character) |
| 0x8E | `return_to_title()` | 0→0 | immediate | no | INFERRED | Abort the game and switch to the title-screen mode (song 45). The script returns immediately (its mode stack is gone). (0x03002A58 = 1; clear registry 0x08011B04; 0x08012628; stop music 0x0803E2EC; pop all modes 0x0803D8F2; replace mode with vt 0x0804A404 (plays song 45 = title music)) |
| 0x8F | `show_boss_bar(actor)` | 1→0 | immediate | no | VERIFIED | Show the boss HP gauge (slides in at the top right) bound to ACTOR. (0x080195A8(0, actor_obj, 0, 100) -> HP gauge object vt 0x0804CE28 (OBJ pool 0x087196E0)) |
| 0x90 | `show_boss_bar_ex(actor, a, b)` | 3→0 | immediate | no | INFERRED | show_boss_bar with explicit gauge parameters A (default 0) and B (default 100). (0x080195A8(0, actor_obj, a, b)) |
| 0x91 | `hide_boss_bar()` | 0→0 | immediate | no | INFERRED | Detach/hide the boss HP gauge (first gauge that answers message 0x13). (0x08011C70(registry, 0x13, 0): message 0x13 to entities; the gauge class (0x08019721) answers by detaching from its actor) |
| 0x92 | `equipment_count(item)` | 1→1 | query | no | VERIFIED | Number of copies of equipment ITEM owned (entries of the equipment list whose low 28 bits == ITEM). (0x0800441E(GameData, item): list at GameData+0xE4) |
| 0x93 | `give_equipment(item)` | 1→0 | enqueue | cond. | VERIFIED | Add equipment ITEM and play the item-get presentation on the player. (0x08004448(GameData, item) (append, max 250); item-get cmd vt 0x0804C83C on the player track with -item) |
| 0x94 | `take_equipment(item)` | 1→0 | immediate | no | VERIFIED | Remove one copy of equipment ITEM from the list. (0x08004468(GameData, item)) |
| 0x95 | `set_character(char)` | 1→0 | immediate | no | VERIFIED | Make party character CHAR (0 Goku .. 8) the active character, cancel the Holy Water bonus and rebuild the player object for the new character. (GameData[0] = char; Holy Water flag GameData+0x4DC = 0; 0x08010692(player, rec) ; 0x0801712C/0x0801711C release/load form graphics) |
| 0x96 | `in_party(char)` | 1→1 | query | no | VERIFIED | Whether character CHAR has joined the party (record flag bit 0). (char record (0x03001560+0x68*char) byte0 bit0) |
| 0x97 | `join_party(char)` | 1→0 | immediate | no | VERIFIED | Character CHAR joins the party. (record byte0 \|= 1) |
| 0x98 | `leave_party(char)` | 1→0 | immediate | no | VERIFIED | Character CHAR leaves the party. (record byte0 &= ~1) |
| 0x99 | `set_char_flag2(char)` | 1→0 | immediate | no | VERIFIED | Set record flag bit 1 of CHAR (meaning not identified; already set for character 1 in the snakeway save). (record byte0 \|= 2) |
| 0x9A | `clear_char_flag2(char)` | 1→0 | immediate | no | VERIFIED | Clear record flag bit 1 of CHAR. (record byte0 &= ~2) |
| 0x9B | `is_super_saiyan()` | 0→1 | query | no | VERIFIED | Whether the active character is in a Super Saiyan or SS3 form (form flag bits 0/1). (form table 0x086B82EC[active rec+0x14].flags & 3) |
| 0x9C | `char_sprite(char)` | 1→1 | query | no | VERIFIED | Sprite id of character CHAR's current form (e.g. 117 = Goku base). (form table[rec(char)+0x14].u16 +0xE (gfx id)) |
| 0x9D | `set_char_form(char, sprite)` | 2→0 | immediate | no | VERIFIED | Set character CHAR's form to the form whose sprite is SPRITE; if CHAR is active, rebuild the player sprite. (0x08013DD6(rec, sprite); if char is active: 0x0801712C, player 0x0800EDF8(sprite def 0x08000158(sprite)), vt+0x28, 0x08010DAA, 0x0801711C) |
| 0x9E | `spawn_marker_a(x, y)` | 2→0 | immediate | no | INFERRED | Spawn a static OBJ sprite object (64x64 graphic descriptor 0x08055000) at (X, Y). (0x08050690(0, &{x,y}, gfx desc 0x08055000, 0) static OBJ object vt 0x0804BA50) |
| 0x9F | `spawn_marker_b(x, y)` | 2→0 | immediate | no | INFERRED | As spawn_marker_a with graphic descriptor 0x0805500C. (0x08050690(0, &{x,y}, gfx desc 0x0805500C, 0)) |
| 0xA0 | `spawn_box(x, y, w, h)` | 4→0 | immediate | no | INFERRED | Spawn a box object at (X, Y) with size (W, H) (drawn as a solid dark-red 16x16 sprite; debug/blocker). (0x080279E0(0, &{x,y,w,h})) |
| 0xA1 | `actor_x(actor)` | 1→1 | query | no | VERIFIED | X position (px) of ACTOR. (actor+0x1A0 >> 8) |
| 0xA2 | `actor_y(actor)` | 1→1 | query | no | VERIFIED | Y position (px) of ACTOR. (actor+0x1A4 >> 8) |
| 0xA3 | `actor_dir(actor)` | 1→1 | query | no | VERIFIED | Facing direction of ACTOR (0 down, 1 up, 2 left, 3 right). (u8 actor+0x10) |
| 0xA4 | `learn_skill(char, skill)` | 2→0 | immediate | no | VERIFIED | Append SKILL to character CHAR's skill list (rec+0x0D.., count rec+0x0C) at level 0 (rec+0x2E+2*i = 0). (0x08013A90(rec, skill)) |
| 0xA5 | `forget_skill(char, skill)` | 2→0 | immediate | no | VERIFIED | Remove SKILL from CHAR's list (shifting ids and level pairs down; reset the selected skill if out of range). (0x08013ABA(rec, skill)) |
| 0xA6 | `replace_skill(char, old, new)` | 3→0 | immediate | no | VERIFIED | Replace skill id OLD with NEW in CHAR's list, keeping its slot and level. (0x08013B12(rec, old, new)) |
| 0xA7 | `watch_this_game(level)` | 1→1 | modal | yes | VERIFIED | Run the "Watch This!" button-sequence challenge (memorise and repeat button prompts) with difficulty LEVEL; pushes exit code - 1 (success flag). (mode 0x08027DB0(0, level); result - 1 pushed) |
| 0xA8 | `fuse_characters(fused_char, fusion_kind, member_a, member_b, seconds)` | 5→0 | immediate | no | VERIFIED | Build fusion character record rec(fused) from rec(member_a)/rec(member_b): rec+2 = form_char(member_b) \| form_char(member_a)<<4 \| fusion_kind<<8 (fusion components); maxHP/HP/maxEP/EP/stats = (a+b)*0x9A>>8 (60% of the sum); rec(fused)+8 = seconds*60 frames (or -1 if seconds == -1 = permanent); rec(fused)+0 \|= 3 (unlocked+active); rec(member_a/b)+0 &= ~1 (members unavailable while fused). (0x0801375E) |
| 0xA9 | `spawn_map_item_at(item_slot, x, y)` | 3→0 | immediate | no | INFERRED | If item_slot < current map header byte 6: create the map's item pickup #item_slot (map +0x1C list; def+4 = item id, >=0 supply / <0 equipment; pickup vtable 0x0804C508) at pixel (x, y) and add it to the world. (0x080090E6 -> 0x0800DCDC) |
| 0xAA | `spawn_map_item(item_slot)` | 1→0 | immediate | no | INFERRED | Same as spawn_map_item_at but the position/extra args are the globals 0x03001010/0x03001018 (normally 0 = the pickup's own placement). (0x080090E6 -> 0x0800DCDC) |
| 0xAB | `show_radar()` | 0→0 | immediate | no | VERIFIED | Anti-tamper check: sum bytes 0x08000000..0x0800009F (mod 256) != 0x33 -> [0x03001004] = 1 (read by native 0xF6). Then create the radar HUD entity (vtable 0x0804B400, OBJ tiles from 0x08753008; top-left semicircle) and add it to the world. The radar accepts targets through world message 0x1B (native 0xAC) and resets on message 0x0B. (0x0802B9A8 (+ ROM header checksum)) |
| 0xAC | `radar_add_target(x, y)` | 2→0 | immediate | no | INFERRED | Broadcast world message 0x1B with (x, y); the radar entity appends the point to its target list (+0x1C count, 8-byte entries) and shows a blip. (broadcast(world, 0x1B, {x, y})) |
| 0xAD | `respawn_conditional_map_objects()` | 0→0 | immediate | no | INFERRED | For each conditional object def of the current map (map +0x18 list, count map byte 5): if its condition (0x08004784, story-flag test) now holds, spawn it (0x0800C7E2). Used after setting story flags so NPCs/objects appear without reloading the map. (0x08008AFE) |
| 0xAE | `start_countdown(seconds)` | 1→0 | immediate | no | VERIFIED | Start the countdown timer: GD byte 1 bit 2 = running, GD+0x4D4 = remaining frames (seconds*60), decremented by the game loop. (GD+1 \|= 4; GD+0x4D4 = seconds*60) |
| 0xAF | `stop_countdown()` | 0→0 | immediate | no | INFERRED | Stop the countdown timer (clear GD byte 1 bit 2). (GD+1 &= ~4) |
| 0xB0 | `show_countdown_hud()` | 0→0 | immediate | no | VERIFIED | Create the timer HUD entity (vtable 0x0804BACC) in countdown mode (shows GD+0x4D4 as SS:hh, top-right). (0x0802FCA0(0, 0) + world_add) |
| 0xB1 | `hide_timer_hud()` | 0→0 | immediate | no | VERIFIED | Broadcast message 0x1C: the timer HUD entity (handler 0x0802FEEF) removes itself. (broadcast(world, 0x1C, 0)) |
| 0xB2 | `show_title_text(string_id, x, y)` | 3→0 | immediate | no | VERIFIED | Create a big outlined title-text entity for string table entry string_id, centred at (x, y) (e.g. "Match 2 Fight!"). Removed by 0xEF. (0x0802BFBC -> 0x0802BECC + world_add) |
| 0xB3 | `sin_scale(radius, angle)` | 2→1 | query | no | VERIFIED | Push (radius * sin_table[angle & 0xFF]) >> 16 (angle in 1/256 turns, table is 16.16). (sine table 0x087B6EEC) |
| 0xB4 | `cos_scale(radius, angle)` | 2→1 | query | no | VERIFIED | Push (radius * sin_table[(angle + 64) & 0xFF]) >> 16. (sine table 0x087B6EEC) |
| 0xB5 | `run_event(event)` | 1→0 | modal | yes | VERIFIED | run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at 0x03002A5C, cancel flag 0x03002A58. (0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)) |
| 0xB6 | `run_event(event, p0)` | 2→0 | modal | yes | VERIFIED | Write P0 = p0 into the CURRENT parameter block (visible to the caller afterwards), then run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at 0x03002A5C, cancel flag 0x03002A58. (0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)) |
| 0xB7 | `run_event(event, p0, p1)` | 3→0 | modal | yes | VERIFIED | Write P0 = p0, P1 = p1 into the CURRENT parameter block (visible to the caller afterwards), then run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at 0x03002A5C, cancel flag 0x03002A58. (0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)) |
| 0xB8 | `run_event(event, p0, p1, p2)` | 4→0 | modal | yes | VERIFIED | Write P0 = p0, P1 = p1, P2 = p2 into the CURRENT parameter block (visible to the caller afterwards), then run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at 0x03002A5C, cancel flag 0x03002A58. (0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)) |
| 0xB9 | `run_event(event, p0, p1, p2, p3)` | 5→0 | modal | yes | VERIFIED | Write P0 = p0, P1 = p1, P2 = p2, P3 = p3 into the CURRENT parameter block (visible to the caller afterwards), then run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at 0x03002A5C, cancel flag 0x03002A58. (0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)) |
| 0xBA | `get_flag_bits(first_flag, count)` | 2→1 | query | no | VERIFIED | Push the integer whose bit i = story flag (first_flag + i), i < count. (0x08004364 per bit) |
| 0xBB | `set_flag_bits(first_flag, count, value)` | 3→0 | immediate | no | VERIFIED | value clamped to 0..2^count-1; story flag (first_flag + i) = bit i of value. (0x08004380 / 0x08004396 per bit) |
| 0xBC | `show_stopwatch_hud()` | 0→0 | immediate | no | VERIFIED | Create the timer HUD entity in stopwatch mode (shows GD+0x4D8 counting up). (0x0802FCA0(0, 1) + world_add) |
| 0xBD | `hide_stopwatch_hud()` | 0→0 | immediate | no | VERIFIED | Same handler body as 0xB1 (separate table entry). (broadcast(world, 0x1C, 0)) |
| 0xBE | `reset_stopwatch()` | 0→0 | immediate | no | VERIFIED | Stopwatch frames = 0. (GD+0x4D8 = 0) |
| 0xBF | `start_stopwatch()` | 0→0 | immediate | no | VERIFIED | Stopwatch running (game loop increments GD+0x4D8 each frame). (GD+1 \|= 0x10) |
| 0xC0 | `stop_stopwatch()` | 0→0 | immediate | no | INFERRED | Stop the stopwatch. (GD+1 &= ~0x10) |
| 0xC1 | `get_stopwatch()` | 0→1 | query | no | VERIFIED | Push stopwatch frames. (GD+0x4D8) |
| 0xC2 | `give_xp(amount)` | 1→0 | immediate | cond. | VERIFIED | Give XP to the current character (same routine as kill XP); level-ups run their popups/levelling inline and block the script while shown. (0x08013914(rec(cur), amount)) |
| 0xC3 | `get_xp()` | 0→1 | query | no | VERIFIED | Push XP of the current character. (rec(cur)+0x24) |
| 0xC4 | `set_game_bit_20()` | 0→0 | immediate | no | INFERRED | Set GameData byte 1 bit 5 (set together with the stopwatch in the race script #163; cleared by 0xC5). (GD+1 \|= 0x20) |
| 0xC5 | `clear_game_bit_20()` | 0→0 | immediate | no | INFERRED | Clear GameData byte 1 bit 5. (GD+1 &= ~0x20) |
| 0xC6 | `run_power_struggle()` | 0→0 | modal | yes | VERIFIED | Run the button-mash power-struggle gauge mini-game (bar at the bottom of the screen) as a modal mode; used in the Spirit Bomb vs Kid Buu finale (#463/#464). (run_mode(vtable 0x0804B5D8)) |
| 0xC7 | `run_punch_minigame(mode)` | 1→1 | modal | yes | VERIFIED | Run the "Press A Button To Punch" gauge mini-game as a modal mode; push (mode result - 1). (0x08030384(0, mode) + run_mode) |
| 0xC8 | `get_level()` | 0→1 | query | no | VERIFIED | Push the level (50..200) of the current character from its XP. (0x080135D2(rec(cur))) |
| 0xC9 | `broadcast_1f()` | 0→0 | immediate | no | INFERRED | Broadcast world message 0x1F (receiver not identified). (broadcast(world, 0x1F, 0)) |
| 0xCA | `heal_party()` | 0→0 | immediate | no | INFERRED | For all 9 character records: HP = maxHP, EP = maxEP; if rec+0x13 bit 0, also 0x08013C66 (refresh the live player). (rec(0..8): HP = maxHP, EP = maxEP) |
| 0xCB | `screen_rect_x_at_actor(actor, w, h)` | 3→1 | query | no | INFERRED | Centre a w x h rectangle on the actor's bounding-box centre, clamp it to the camera/map bounds; push its left x (used to place fusion-dance / cutscene props on screen). (0x080001C6, 0x0800FDFA, 0x08009816(camera)) |
| 0xCC | `screen_rect_y_at_actor(actor, w, h)` | 3→1 | query | no | INFERRED | As 0xCB but pushes the rectangle's top y. (0x080001C6, 0x0800FDFA, 0x08009816(camera)) |
| 0xCD | `shop_add_item(category, item, price, flag)` | 4→0 | immediate | no | VERIFIED | Append a shop entry {u8 category (0 supply, 1 equipment), u8 flag, u16 item, u32 price} to the shop list (count at 0x03003174, 8-byte entries) consumed by the buy screen 0xCE. (0x08031808 (list 0x03003174)) |
| 0xCE | `open_shop_buy()` | 0→0 | modal | yes | VERIFIED | Fade out, run the shop BUY screen over the list from 0xCD (modal), restore the world (0x08011DB4/0x08011E0E, camera 0x0800938C), fade in. Blocks until the player leaves the shop. (run_mode(0x0805058C fade) ; 0x080315B0 buy screen ; fades) |
| 0xCF | `open_shop_sell(price_percent)` | 1→0 | modal | yes | VERIFIED | As 0xCE but the SELL screen; items sell for price*price_percent/100 (shops pass 50). (0x08031E84(0, price_percent)) |
| 0xD0 | `set_form(form)` | 1→0 | immediate | no | VERIFIED | Transform the current character: leave the old form (0x0801712C(form def)), rec(cur)+0x14 = form, refresh the player entity (0x08010692), enter the new form (0x0801711C). (0x0801712C, 0x08010692, 0x0801711C) |
| 0xD1 | `run_credits()` | 0→0 | modal | yes | VERIFIED | Run the staff-credits sequence (Webfoot Technologies, Jeffrey Lim, ...) as a modal mode. (0x080335BC(0, 1) + run_mode) |
| 0xD2 | `a_pressed()` | 0→1 | query | no | VERIFIED | Push 1 if the A button was newly pressed this frame (input manager 0x03003300: +4 = keys pressed this frame, +6 = released). ([0x03003300]+4 bit 0) |
| 0xD3 | `form_name_ptr(char)` | 1→1 | query | no | VERIFIED | Push a POINTER to the UTF-16 name of the character's current form (for %s in formatted dialog). (string table[form_def(rec(char)+0x14)+2]) |
| 0xD4 | `sleep_mode()` | 0→0 | modal | yes | INFERRED | GBA sleep: disable interrupts except keypad, SWI 3 (Stop) until the wake key combo, then wait for release. (0x0803FA38) |
| 0xD5 | `is_equipped(item)` | 1→1 | query | no | VERIFIED | Push 1 if equipment `item` is in its slot (equipment table 0x086F82EC + 0x2C*item + 0x14 = slot) on the current character. (rec(cur)+0x3C+4*slot(item)) |
| 0xD6 | `run_script(script)` | 1→0 | immediate | cond. | VERIFIED | Synchronously run ONLY node 0 of resource entry `script` (must be kind 0 or 4, else assertion) with the CURRENT parameter block (shared P0..P3) and shared globals G0..G3; its return value is discarded. Blocks only if the callee blocks. (0x08004098) |
| 0xD7 | `run_script(script, p0)` | 2→0 | immediate | cond. | VERIFIED | P0 = p0 in the CURRENT parameter block, then run_script(script). (0x08004098) |
| 0xD8 | `run_script(script, p0, p1)` | 3→0 | immediate | cond. | VERIFIED | P0 = p0, P1 = p1 in the CURRENT parameter block, then run_script(script). (0x08004098) |
| 0xD9 | `show_destination_label(set)` | 1→0 | immediate | no | INFERRED | Create a destination-label HUD entity (vtable 0x0804BAF4) for label set `set` (table 0x0875A73C+8); its 32 entries are filled by 0xDA. (0x0803643C(0, set) + world_add) |
| 0xDA | `set_destination_label(index, string_id)` | 2→0 | immediate | no | INFERRED | Broadcast message 0x23: the label entity stores string_id at entry index and redraws if it is the current one. (broadcast(world, 0x23, {index, string_id})) |
| 0xDB | `set_area_flag_c(enable)` | 1→0 | immediate | no | INFERRED | Set the area manager's +0xC flag (meaning not identified). ([0x0300197C+0xC] = enable != 0) |
| 0xDC | `set_entity_value(id, value)` | 2→0 | immediate | no | INFERRED | Find the first world entity that answers message 8 with `id`; set its +0x18 = value and +4 = +0x30 + value. (0x08011C70(world, 8, id)) |
| 0xDD | `party_xp_total()` | 0→1 | query | no | VERIFIED | Push the summed XP of character records 0..4. (sum rec(0..4)+0x24) |
| 0xDE | `string_ptr(string_id)` | 1→1 | query | no | VERIFIED | Push a POINTER to string table entry string_id (for %s formatting). (string table 0x0806BCE8) |
| 0xDF | `eq_add_hp_regen(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x10 (u16) += n*0x445 >> 8 (HP regeneration per frame /256). (eqacc+0x10 (u16) += n*0x445 >> 8) |
| 0xE0 | `eq_add_thorns(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x08 (u16) += n*0xA4 >> 6 (thorns damage). (eqacc+0x08 (u16) += n*0xA4 >> 6) |
| 0xE1 | `eq_add_end(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x1C += n; eqacc+0x20 += n (END (melee) and END (energy)). (eqacc+0x1C += n; eqacc+0x20 += n) |
| 0xE2 | `eq_add_pow(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x18 += n (POW). (eqacc+0x18 += n) |
| 0xE3 | `eq_add_spd(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x24 += n (SPD). (eqacc+0x24 += n) |
| 0xE4 | `eq_add_str(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x14 += n (STR). (eqacc+0x14 += n) |
| 0xE5 | `eq_halve_block_damage()` | 0→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x0C (u16) /= 2 (damage taken while blocking). (eqacc+0x0C (u16) /= 2) |
| 0xE6 | `eq_halve_energy_cost()` | 0→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x01 (u8) /= 2 (EP cost of energy attacks). (eqacc+0x01 (u8) /= 2) |
| 0xE7 | `eq_halve_energy_block_cost()` | 0→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x02 (u8) /= 2 (EP cost of blocking energy). (eqacc+0x02 (u8) /= 2) |
| 0xE8 | `eq_scale_ss_cooldown()` | 0→0 | immediate | no | VERIFIED | Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat recalculation): eqacc+0x0E (u16) = x*7 >> 3 (Super Saiyan drain/cooldown rate). (eqacc+0x0E (u16) = x*7 >> 3) |
| 0xE9 | `set_skill_level(char, skill, level)` | 3→0 | immediate | no | VERIFIED | Find `skill` in the character's known-skill list (rec+0x0D.., count rec+0x0C) and set both level bytes (rec+0x2E+2i, +0x2F+2i) = level; asserts (0x0803DF14) if the skill is not known. (0x08013B54(rec(char), skill, level)) |
| 0xEA | `get_skill_level(char, skill)` | 2→1 | query | no | INFERRED | Push the level byte (rec+0x2F+2i) of a known skill; asserts and pushes 0 if unknown. (0x08013BCC(rec(char), skill)) |
| 0xEB | `set_game_bit_40()` | 0→0 | immediate | no | VERIFIED | Set GameData byte 1 bit 6. (GD+1 \|= 0x40) |
| 0xEC | `add_map_marker(x, y, highlight)` | 3→0 | immediate | no | VERIFIED | Create a small blinking marker entity (vtable 0x0804D634) centred at (x, y); highlight selects the current-location style. Used by the world-map screen script #15 (one marker per unlocked area). (0x08026DE4(0, {x-4, y-4}, highlight != 0) + world_add) |
| 0xED | `get_stat_points(char)` | 1→1 | query | no | VERIFIED | Push unspent stat points. (rec(char)+0x2C) |
| 0xEE | `add_stat_points(char, n)` | 2→0 | immediate | no | VERIFIED | Give unspent stat points. (rec(char)+0x2C += n (u16)) |
| 0xEF | `hide_title_text()` | 0→0 | modal | yes | VERIFIED | Broadcast message 0x27 (title-text entities remove themselves) and run a 1-frame modal wait so the removal is drawn before the script continues. (broadcast(world, 0x27, 0); run_mode(vtable 0x08049978, 1)) |
| 0xF0 | `set_actor_field_1a8(actor, value)` | 2→0 | immediate | no | INFERRED | Write actor object +0x1A8 = value<<10 (field not identified). (0x080001C6(actor)+0x1A8 = value << 10) |
| 0xF1 | `unequip_slot(char, slot)` | 2→0 | immediate | no | INFERRED | If the slot does not hold its default item (0x080555B8[slot]), move the equipped item back to the inventory (GD+0xE4 list, entries: top nibble = owner+1, low 28 bits = equipment id) and put the default back. (0x08004544(GD, char, slot)) |
| 0xF2 | `boost_stats(seconds, str, pow, end, spd)` | 5→0 | immediate | no | VERIFIED | seconds == 0: permanently add to the current character's base STR/POW/END/SPD (u8). Otherwise timed buff: GD+0x4DC = seconds*60 frames, GD+0x4E0/4E4/4E8/4EC = str/pow/end/spd, GD+0x4F0 = 0 (applied by 0x080046AE during stat recalculation); then refresh the player (0x0801064C). (seconds == 0: rec(cur)+0x28..0x2B += ; else GD+0x4DC.. ; 0x0801064C) |
| 0xF3 | `level_up_to(char, level)` | 2→0 | immediate | no | INFERRED | level clamped to 200; if the character's level is lower: apply level-up growth from current to target (0x08013678) and set XP to the target level threshold (0x080135BC). (0x080135D2, 0x08013678, 0x080135BC) |
| 0xF4 | `show_energy_sphere(x, y, radius, param)` | 4→0 | immediate | no | VERIFIED | Create the large BG-layer sphere effect (vtable 0x080493B4, 0x830 bytes) at (x, y) with radius and a parameter, replacing BG-effect slot 0. Used by the Spirit Bomb finale (#461-#463, #480). (0x0803A84A(0, 0, {x, y}, radius, param, 1) -> BG effect slot 0 (0x0800927E)) |
| 0xF5 | `eq_add_crit(n)` | 1→0 | immediate | no | VERIFIED | Equipment-effect accumulator: critical-hit chance percent += n. (eqacc+0x00 (u8) += n) |
| 0xF6 | `is_tampered()` | 0→1 | query | no | VERIFIED | Push the anti-tamper flag set by 0xAB (ROM header checksum mismatch). Script #101 then runs the dialog graph #37 "The game will not run on this hardware." ([0x03001004]) |
| 0xF7 | `start_timed_special(seconds)` | 1→0 | immediate | no | VERIFIED | Timed buff with no stat bonuses but GD+0x4F0 = 1, which stat recalculation (0x080046AE) copies into the record byte +0x67 (special flag; effect not identified). (GD+0x4DC = seconds*60; GD+0x4E0..0x4EC = 0; GD+0x4F0 = 1; 0x0801064C) |

| setter | name | semantics | status |
|---|---|---|---|
| 0 | `G0` | word 0x03001058 = value | VERIFIED |
| 1 | `G1` | word 0x0300105C = value | VERIFIED |
| 2 | `G2` | word 0x03001060 = value | VERIFIED |
| 3 | `G3` | word 0x03001064 = value | VERIFIED |
| 4 | `P0` | [[0x03001030]+0] = value (current parameter block) | VERIFIED |
| 5 | `P1` | [[0x03001030]+4] = value (current parameter block) | VERIFIED |
| 6 | `P2` | [[0x03001030]+8] = value (current parameter block) | VERIFIED |
| 7 | `P3` | [[0x03001030]+12] = value (current parameter block) | VERIFIED |
| 8 | `L0` | word 0x03001020 = value | VERIFIED |
| 9 | `L1` | word 0x03001024 = value | VERIFIED |
| 10 | `L2` | word 0x03001028 = value | VERIFIED |
| 11 | `L3` | word 0x0300102C = value | VERIFIED |
| 12 | `hp` | Active character HP = clamp(value, 0, maxHP) (s16 rec+0x16). If the player object exists: value <= 0 calls player->vtable[+0x7C] (knock-out) and the record comes from player->vtable[+0x80]; if rec+0x13 bit0 (fusion) 0x08013C66 syncs the components. | VERIFIED |
| 13 | `max_hp` | Sets BOTH HP and max HP (rec+0x16, +0x18) = value; fusion sync as above. | VERIFIED |
| 14 | `energy` | clamp(value, 0, maxEP) into the form's energy slot (SS3 gauge +0x20 / SS gauge +0x1E / EP +0x1A, same rule as get_energy). | VERIFIED |
| 15 | `max_ep` | Sets BOTH EP and max EP (rec+0x1A, +0x1C) = value. | VERIFIED |
| 16 | `zenie` | Zenie (0x0300155C) = max(value, 0). | VERIFIED |
| 17 | `variant_byte` | byte 0x03002BB0 = value (see native 0x11). | VERIFIED |
<!-- /GENERATED -->

---------------------------------------------------------------------------------------------------------

# Part B — runtime (blocking, director, scenes, text, traces)


## B1. Three layers

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

## B2. VM state (per interpreter object, 0x6C bytes, usually on the C stack)

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

## B3. Blocking: modes, the director, per-actor command queues

### B3.1 run_mode (`0x0803D8A0`)

Mode stack at `0x03000F90`: `+0` current mode, `+4` frame counter, `+8` depth, `+0xC..` saved modes.
`run_mode(m)`: save current, `m->enter()` (vtable +4), make it current, then loop (`0x0803D7FA`):
`cur->frame()` (+0xC), wait for VBlank / system tick, `cur->step()` (+0x14) until it returns non-zero; then
`m->exit()` (+8) and restore. Most script modes inherit the "game" step `0x08007041`: run the normal world
update `0x0800965C` (actors, triggers, camera, text) and then ask `+0x28 is_done()`. So **a blocking native
= N full game frames, then return**. VERIFIED (block durations in every trace).

### B3.2 Per-actor command queues ("tracks")

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

### B3.3 The director (`0x03002BAC` = `[0x08054FBC+0x1C]`) — batch vs immediate

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

### B3.4 Which natives block (frame-spanning)

From `script_native_calls.py` (static, reaches run_mode / enqueue / commit / run_scene) plus traces:

- **Actor/camera commands (block in immediate mode, return at once in batch):** 0x3B–0x70 (except the
  pure setters among them), 0x80, 0x93. Command classes: §B3.5.
- **Own run_mode (always block):** 0x31, 0x32 (camera pan to actor / point, wait arrival), 0x3A (commit),
  0x72–0x7E family (0x72/0x73 cutscene backdrop change with/without fade, 0x74/0x75/0x79 fades, 0x76, 0x78
  fade-out N, 0x7A–0x7E screen effects), 0x88 and 0xEF (wait N frames: 0x88(120) = 120 frames VERIFIED),
  0x89, 0x8C, 0x8D, 0xA7, 0xC6, 0xC7, 0xCE, 0xCF, 0xD1.
- **Nested modal scene:** 0xB5–0xB9, 0xC2 = `run_scene(idx [, P0..P3])` (`0x08012588`; writes P into the
  current block first). Blocks until the scene exits. VERIFIED: 0xB7(516,1450,1451) = difficulty menu,
  466 frames, scene #516's script sees P=[1450,1451,0,0].
- **Map change:** 0x77 warp(world, map, variant, x, y) blocks ~24 frames while the new area loads (spawn
  conditions run nested) and **the script continues in the new map** (VERIFIED intro frame 5226 → 5250).
  No longjmp kills the running script on this path.
- Measured durations of the natives seen in the traces: `script_trace_index.py` (blocking(>1f) lists). A
  "1f" gap can just be CPU time crossing VBlank and is not blocking.

### B3.5 Command classes (vtables constructed by natives; full disassembly `work/buu/script/cmd_classes_asm.txt`)

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

### B3.6 Actor ids (`0x080001C6 actor(id)`)

- `0` = the player object (`[0x03001C34]`), `9` = `[0x03001BAC]` (special companion object),
- `1..8` = the sprite of character record `id-1` (Goku, Gohan, Goten, Trunks, Vegeta, Gotenks, Vegito,
  Gogeta), looked up as "first world object whose sprite def is that"; the player matches its own
  character's id (VERIFIED: actor 1 = player while Goku is played),
- `>= 10` = sprite table id (`0x086B6BDC`), first world object using that sprite (149 = Janemba, 154 =
  Super Janemba — VERIFIED),
- `< 0` = map placement tag `-(id+1)` (world message 7).
Native **0x95(char)** switches the played character (reloads the player sprite) — VERIFIED in #450.

## B4. Scenes (the table at 0x0879B704)

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

## B5. Text and dialog format

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

## B6. Ground-truth traces (verification part a)

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

## B7. Notes for the re-implementation

- The only places a script can be suspended are blocking native calls. A resumable interpreter therefore
  only needs to yield there; its suspended state is: the stack of interpreter frames (pc, value stack ≤ 16,
  loop stack ≤ 4, P pointer), G0..G3, the director batch flag + group, the actor/camera command queues and
  the scene objects (state, current step). All plain integers — cheap to snapshot for rollback.
- Determinism: the VM itself is integer-only; randomness only via natives 0x19/0x1A (game PRNG).
- Order of side effects inside one frame matters (nested scripts run in the middle of the outer script's
  wait, sharing G and P); a faithful port keeps the "run nested scripts to completion before resuming"
  rule.
