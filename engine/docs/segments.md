# Segments: start wf-core from a real-game emulator state (crate `wf-import`)

Goal: match campaign chapters against the real game **in parallel**. Each chapter agent takes an emulator state
somewhere in the campaign, records the real game from there with its own input (a *segment*), imports the start
state into wf-core, replays the same input and reads the first divergence. The full New Game playthrough
(`wf-script-core/tests/playthrough.rs`) stays as it is.

## 1. One-command workflow

```
# a segment from any save state (keys: JSON list | {"frames":[{keys}]} ; or --program 'R:120,0:30:a' ; or
# --goals path/to/route.py:GOALS for tools/story_playthrough/driver.py goals)
.venv/bin/python tools/segments/segment.py record --state work/states/x.ss --keys keys.json --frames 3000 --name ch3_a
# a segment cut out of a playthrough recording (rebuilds the emulator state by replaying its keys)
.venv/bin/python tools/segments/segment.py cut --from data/story_playthrough/opening.json --at area:1-13#2 --frames 2000 --name ow_1_13b
.venv/bin/python tools/segments/segment.py cuts --from data/story_playthrough/opening.json      # the area-entry cuts
.venv/bin/python tools/segments/segment.py save --from data/story_playthrough/opening.json --at area:1-15 --out work/states/x.ss
# replay + first divergence (all segments, or a list)
cd engine && SEGMENT=ow_1_13b cargo test -p wf-import --test segments -- --nocapture
```

Output `data/segments/NAME.json` (one frame per line; ~250 KB per 600 frames incl. the start RAM) and
`work/segments/NAME.ss` (the start save state, local only). Rows = the playthrough's (`tools/story_playthrough/
record.py`). `init` = the start snapshot (`capture.snapshot`) + `keys` (input manager held keys) + `ram`
(`{iwram, ewram}`: zlib + base64). A segment with `"expect": {"matched": N}` in its file is a **gate** (the test fails below N continuously
matched video frames); one without it only reports, so chapter work in progress never turns the suite red.
`SEGMENT_STRICT=all` holds every segment to its whole recording, `SEGMENT_STRICT=0` never fails, `SEGMENT_DIR=dir`
runs another directory (the recorder writes there too when it is set).

**Where to start a segment:** at an **area-entry cut** — the emulator state right after the logic frame in which
the leader appears in a freshly loaded area (fade-in not stepped yet). `segment.py cut --at area:W-A[#n]` picks it in
a recording; for your own states: run the game to an area transition and save the state on the first video frame
whose logic counter (`0x03000F94`) changed after the player object (`[0x03001C34]`) is non-null in the new area (or
`record` from any earlier state and use `cut --from data/segments/NAME.json --at area:...` on that recording: `cut` / `save` rebuild the
emulator state from a playthrough's soft reset or from a segment's start state `work/segments/NAME.ss`). `wf_import::inspect(&ram).kind == CutKind::AreaEntry` says whether a RAM is one.

## 2. API (`engine/crates/wf-import`)

```rust
use wf_import::{GbaRam, inspect, import_story, import_emulator_state, StoryHost, CutKind};

let ram = GbaRam::from_json(&doc["init"]["ram"])?;            // or GbaRam::new(iwram, ewram)
let info = inspect(&ram);                                     // area, lf, leader, mode, kind (AreaEntry | LoadPending | MidArea)
let story: Story = import_story(&data, &ram);                 // the persistent state only
let mut host = StoryHost::new(&data, &files)?;                // files = wf_script::export::export(rom, &ScriptSet::load_all(rom)).0
let imp = import_emulator_state(&data, &ram, &mut host)?;     // Imported { world, prev_keys, lf, kind, held_frames, draws, notes }
host.prev_held = imp.prev_keys;
let mut sess = StoryHost::session(&data, imp.world);
for keys in logic_frames { host.step(&mut sess, keys); }      // one logic frame, the playthrough's host order
```

* `StoryHost` = the playthrough's per-frame script loop as a library (one Rhai runtime on the shared host, the
  `FieldEvent::Run` queue, `Cause::Ai` / `Cause::AreaEnter` completion). Keep the **same host** from the import on: a
  script the load started can still be running (`host.running()`). `snapshot()` / `restore()` copy its state.
* `segment::{load, replay, compare, initial_map}` and `segment::Report { frames, logic, compared, first, counts,
  actors, notes, errors, import, started }`: the harness the test uses (`replay(&data, &files, &doc)`).
* `rewind(rng, n)`: the game's RNG run backwards n calls (the generator is invertible).
* `import_emulator_state` returns `Err(Unsupported)` for cuts that are not area entries (MidArea / LoadPending) and
  `Err(Mismatch)` if the rebuilt frame cannot reproduce the RAM's RNG.

## 3. What is imported (coverage)

From RAM (addresses `wf_import::ram`):
* **Story** (`import_story`): current character, game bits (GD+1), 83 supply counts, 143 flag bytes (incl. map-visited
  bits), the equipment list (unequipped entries; equipped ones are the characters' slots), zenie, play time
  (counter + GD+0x4D0), the 9 records (HP / EP / max, XP -> level, base stats, stat points, skills, learned and chosen
  skill levels, selected skill, form, equipment slots, SS cooldown, party / char flags), script words L0..3 / G0..3,
  `variant_byte`.
* **Leader carry-over** (what the game's load keeps): SS / SS3 gauges (record +0x1E / +0x20), SS cooldown, the form
  controller's accumulators (EP regen, story-form countdown, fusion countdown, SS drain; player +0x22C object), Holy
  Water timer + bonuses (GD+0x4DC..), hittable flag (+0x1CC); position (dest) and facing.
* **Frame context**: RNG, logic counter (`World.frame` = the game's counter, `g.fc0` = 0), the input manager's held
  keys (first frame's edges) and its 64-entry press ring (double taps, "presses within N frames" AI tests), the
  playing song, the area-name banner (shown iff the RAM has the banner object), area id / variant.
* **Rebuilt by wf-core itself** (exactly the way it plays a transition): the leader at the last fade-out frame, the
  fade-out's end (old player's idle draw, EnterArea: map, collision, camera snap, map sprites, breakables, items,
  triggers, song, actors with their spawn draws, on-leave / on-enter scripts through the host), the leader's arrival
  (2 draws), fade-in + autowalk, the frame's camera ease; a **blocking on-enter script** (1-15, 1-17 ...) is played
  through its held frames until the leader arrives. The load's starting RNG = the RAM's RNG rewound by the draws the
  rebuild made (two builds).
* **Checked against RAM after the rebuild** (differences = `notes`, the replay still runs): RNG (must match, else
  Err), the leader's 24.8 position / facing, the camera, every RAM actor (player / enemy / NPC objects) has an entity
  of the same position and facing.

Not imported (not needed at an area entry, or not modelled by wf-core): HP 1/1024 damage remainder and the equipment
HP-regen accumulator (start at 0), the countdown / stopwatch timers (GD+0x4D4 / +0x4D8: wf-core has no such timer
yet), the boss bar / HUD hidden state / script camera / screen effects of a cutscene (an area entry has none), the
menu state, the partner record of a cutscene fusion.

**Mid-area cuts (arbitrary frames)** are not importable yet: they need every live object (enemy AI stacks and node
runtimes, patrols, projectiles, pickups, breakables, NPC behaviour runners, the trigger list's touch state) and a
running script's VM thread from RAM. Use area-entry cuts; a chapter has many (every door / edge exit).

## 4. Proof (2026-10-02)

The story playthrough's 26,651 exact frames (New Game -> Cave of the Ancients 1-17), cut at **every** area-entry
cut from 1-2 on and recorded again from the rebuilt emulator state to frame 26,650 (`work/proof_cuts.sh`, segments in
`work/segments_proof/`, every re-recorded row identical to the playthrough's), imported and replayed:
**17 / 17 segments exact to the end** (every actor, camera, fade, RNG, sounds, text, zenie), with no import note
(RNG, leader, camera and every actor equal to the RAM right after the rebuild).

| cut | start frame | frames replayed | load: RNG draws / held frames |
|---|---|---|---|
| 1-2 | 6,811 | 19,839 | 3 / 0 (on-enter `block_area`) |
| 1-5 | 7,215 | 19,435 | 3 / 0 |
| 1-3 | 7,388 | 19,262 | 11 / 0 (Yemma's office NPCs) |
| 1-4 | 9,252 | 17,398 | 7 / 0 |
| 1-11 | 11,999 | 14,651 | 12 / 0 |
| 1-12 | 14,407 | 12,243 | 5 / 0 |
| 1-13 #1 / #2 / #3 | 15,058 / 20,553 / 22,342 | 11,592 / 6,097 / 4,308 | 6 / 0 |
| 1-14 #1 / #2 / #3 / #4 | 16,609 / 19,717 / 21,658 / 24,223 | 10,041 / 6,933 / 4,992 / 2,427 | 7 / 0 |
| 1-15 #1 / #2 | 18,148 / 24,976 | 8,502 / 1,674 | 4, 3 / **2** (on-enter `prop_show` blocks: played by wf-core) |
| 1-16 | 25,654 | 996 | 4 / 0 |
| 1-17 | 26,069 | 581 | 3 / 2 (blocking on-enter) |

Not importable in that span: 1-1 and 1-10 (their loads run inside an event: the prologue's and s061's warps) —
`segment.py cuts` flags such cuts, and
loads that are not an exit's arrival (a script warp / save load: leader idle, another fade object) as not importable
yet. One rule found on the way: the path followers' re-plan phase (`0x030032E5`,
`World.g.path_stagger`) survives loads; without it the Grand Kai's planet fighters drift 1/256 px within 300 frames.

Regression set in `data/segments/` (`cargo test -p wf-import --test segments`, ~0.1 s each): `ow_1_12` (plain exit),
`ow_1_15` (blocking on-enter script + the boss), `ow_1_17` (blocking on-enter). They carry `"expect"` (gates); add it to a
chapter segment once it matches.

## 5. Next (mid-area import)

Importing at an arbitrary frame needs, from RAM: every object's state (enemy AI stacks and node runtimes, patrol
runners, path followers, projectiles / effects, pickups and their timers, breakables' HP, NPC behaviour runners), the
trigger list's touch / once state, a running event's script thread (VM frames, waits, the director's queues), and the
camera controller / screen-effect objects. The current area-entry import is the base for it: rebuild the area, then
overwrite the objects from RAM and check the next frames.

## 6. Chapter: Super Buu arc (start state, route, segments)

**Start state** (`tools/superbuu/mksave.py superbuu work/superbuu/sb_start.ss`, ~1 s): the Super Buu Saga begins when
Janemba's defeat scene s452 sets story flag 125 (event #6's chapter label "Super Buu Saga" = flag 125 set, 137 clear).
No playthrough reaches it yet, so the state is a **save game loaded by the real game**:
1. base = `work/earth/ow_end.ss` (copied to `work/superbuu/base_ow_end.ss`): the story playthrough route played by the
   real game through Other World (flags 1-13, 157 set by play);
2. the mainline's party / form / skill changes after that are applied by **running the events' own natives through the
   game's script interpreter** (0x08004098), event by event in story order (`STORY` in mksave.py: flags 15 .. 125;
   conditional helpers s501 / s503 resolved for the branch the story takes, e.g. s452 -> s503(1, 5, ..): leave 7,
   join 0 / 4, current char 0);
3. flags 1..125 as the mainline sets them (not 65 / 66 (one switch of three), 75 / 95 (set and cleared again), 122
   (never set)) + the HFIL puzzle / gate flags on the way to Janemba (181, 183 (18-12 Vegeta Lv 100 gate), 284);
   optional collectibles, side quests and map-visited bits stay clear;
4. levels with the game's level-up (`level_up_to` 0xF3: random growth), points spent like the Status screen does
   (35 / 25 / 25 / 15 %; the stat-point pool is a u16 at +0x2C), RecalcStats 0x08013C66: Goku / Goten 180,
   Vegeta 160, Gohan / Trunks 165; zenie 18,000 (lower levels die in the later fights);
5. GameData +0x8A0 = the 18-13 save point (HFIL, next to Janemba's arena), +1 bit 0; written to EEPROM slot 0 by the
   game's own slot writer 0x080049F2 (checksum, EEPROM protocol); then soft reset -> title -> Game 1 -> the game's
   load. Every RAM byte of the resulting state comes from the boot + load path (`work/superbuu/libemu_sb.dylib`:
   tools/emu/shim.c + savedata access, `tools/superbuu/build_sb.sh`).

Validation: the title's Select Game shows the slot as "Super Buu Saga"; the game plays the whole chapter from it with
no intervention (King Yemma s059 -> s067 -> the Z-Sword s464 -> Kami's Lookout -> HTC sparring -> the fusion (Watch
This!) -> Gotenks vs Super Buu -> 6-20), so every flag / party bit these scripts read holds what the story needs.
Known liberties: the levels / zenie / empty inventory are a plausible player's, not a recorded one's.

**Route** `tools/superbuu/sbroute.py [--from MARK] [--to MARK]` (driver goals + `tools/superbuu/sbdriver.py`): the
whole main path, **190,593 frames** from the start state to the title after the credits (Z-Sword, Lookout / HTC,
Gotenks, Ultimate Gohan, Vegito, inside Buu, Kid Buu's three phases, Uub, ending + credits); checkpoints
`work/superbuu/m_<mark>.ss` (start, zsword, lookout, gotenks, ult_gohan, wasteland, fusion_saga, goku_vs_buu, vegito,
inside_buu, intestines, brain, kid_buu_saga, kid_buu, after_kid_buu, final, the_end), keys in
`work/superbuu/sb_keys.json`. Goals: `walke`, `areaw`, `fastmash`, `prog`, `node` (timed presses after a scene node),
`eventsw` (Watch This! sequence from the mode object; the power struggle 0x0804B5D8: A every other frame), `boss`
(+ `timer`: survive the countdown), `fly`, `exitto` (BFS over the exit graph), `evade` (Vegeta's countdown, GD+1 bit 2),
`itbuu` (Super Buu 0x77: dodge the ball, Instant Transmission onto Buu while he glides - the phase advances only on a
hit taken while the player's anim is IT, sprite anim 0x13), `beam`, `bat`, `zap`. Master
`work/superbuu/seg/sb_master.json` (82 MB), cut by `tools/superbuu/mkcuts.py` into `work/superbuu/segs/` (47 cuts at
the area entries outside events).

| segment | from | frames | matched | notes |
|---|---|---|---|---|
| `sb_18_12` | 18-12 (HFIL exit) | 178 | 178 | |
| `sb_18_01` | 18-1 | 600 | 600 | Goku's pad -> Other World travel map 0-51 -> Yemma's Office -> 1-5 |
| `sb_01_03` | 1-3 | 7,735 | 7,553 (gate) | King Yemma s059 + the Janemba flashback scenes (0-17 / 0-16 / 0-96) |
| `sb_01_05b` | 1-5 #2 | 11,000 | 6,541 (gate) | s067 -> 19-3 / 19-7 (Gohan), the Z-Sword power struggle and boulder |

From the full route (`sb_0NN_<world>_<area>`, all gated; the four above come from an older start state, still valid):

| cuts | where | result |
|---|---|---|
| sb_000, sb_001 | 18-12, 18-1 | exact (479, 315) |
| sb_008 - sb_010 | 20-2 .. 20-4 (Wasteland, Gohan vs Buu) | exact (2,632 / 302 / 442) |
| sb_013 - sb_028, sb_031 - sb_033, sb_036 - sb_040 | inside Buu 21-2 .. 21-24 (worms, Blisters / Enzymes, the pods, brain) | exact, 24 cuts, 15,861 frames |
| sb_029 / sb_030 | 21-13 / 21-14 (save point: Switch Characters) | 703 / 490 |
| sb_034 / sb_035 | 21-18 / 21-19 (Buu absorbs: s485 -> s502) | 619 / 367 |
| prefixes (the cut's first matched + 120 frames; full cuts in work/superbuu/segs) | sb_004 1-3 7,623 · sb_005 1-5 7,350 · sb_006 6-6 3,443 · sb_011 20-5 3,025 · sb_012 20-6 975 · sb_041 21-25 -> world map -> Kid Buu 11,442 · sb_044 4-20 (ending) 1,974 | gated |

Rules found (all in wf-core / wf-script-core / wf-import, regressions green):
* **Travel map** 0-51 (`show_destination_label` 0xD9, label entity vt 0x0804BAF4, update 0x0803669E): the selection
  is one of the area's triggers (label index = trigger index, start = the native's argument); LEFT / RIGHT / UP / DOWN
  (in that order) move to the trigger whose centre ((x0 + x1 + 15) >> 1, (y0 + y1 + 7) >> 1) lies that way
  (along >= |across|, least along² + 9 across²); A runs its activation (`field::dest_menu`).
* **Nested on-enter scripts**: `warp` 0x77 and `load_scene` 0x73 run the new area's on-enter script inside the load
  (EnterArea); the script that warped continues when it ends (`Pending::Arrival`, `HostState.nest_area_enter`,
  StoryHost runs it on top of the suspended thread). 0-51's (non-blocking) lets the player arrive in the load frame;
  1-5's (`prop_show` step) holds the arrival 2 frames; 0-16's two `prop_hide`s delay s025's actors 4 frames. Both
  natives are "conditional blocking" in script_natives.json (the Rhai step splitter cuts after them).
* A player a script removed (`remove_actor(0)`, s511) is rebuilt by the next load (`Field.player_removed`).
* `set_character` / `set_char_form` of the active character re-enter idle when the sprite changes (0x08010692 ends with
  vt+0x28(0): one rand(255)).
* Cutscene ki blast (0x41 / 0x42): sound 20 + rand(2). `fire_wave_beam`: 8 frames per cel, held on cel 3 (or the
  sprite's last). An idle reached by a one-shot state's own end or a beam command's end is restarted by a following
  `play_anim(a, 0)` (`ActAnim.self_idle`). `prop_set_pos` takes no frame of its own (`done_at_start`).
* A black fade-out darkens from the current palette (an already black screen stays black: return_to_field after a
  scene that ended faded out).
* The power struggle (0xC6) keeps the object world and the cameras running under the gauge (mode vt 0x0804B5D8 entry 3
  calls the world update 0x080115B0), players get no input; the script continues in the frame the mode ends; a new
  state replaces a frame-range animation the player was playing. 0xC6 always opens the exact screen (menu/minigame.rs).

Rules found inside Buu (VERIFIED by the cuts above; battle code mirrored in the TS reference, cross_ts green):
* **Worm** (0x8E head / 0x8F segments): the head's constructor (its first state's enter) appends the 5 segments to the
  world list before the head itself is appended (`battle::push_initial`: the enemy moves after objects its enter
  made); the 12-slot ring moves a segment only when a command arrives (prefilled 12, replays the 13th back); segment
  graphics 0x129 (spawn template 0x08756A2C); the last segment sets Worm Tail 0x12A + SetAnim(1) every frame; head and
  segments stay idle (no walk anim while moving); contact = message kind 1 (player footprint), 21-frame counter.
* **Blister**: an Enzyme spawns only where its footprint (sprite 0x57) is free on the enemy grid (else retry next
  frame, 0x0802F458).
* A load waiting for its on-enter script holds the enemies like an event (only their anim ticks).
* A broken breakable's callback starts its event before the object pass: the player is not updated that frame
  (`Field.busy_pre_pass`), and the enemies run their update in the event's exit frame (`queued_event`).
* A `load_scene` backdrop's objects are held by the scene's event (`Field.scene_load`).
* A text node whose last page lists `-` options is a choice (s011's "-Save / -Switch Characters").
* Switch Character builds its figures (idle rand(255) each) when the fade-out reaches black; the field's cameras keep
  easing during a menu's fade-out; the screen is black during a page's leave frames.

* A script's `warp_fade` inside an event nests the new area's on-enter script in the load (`Field.nest_fade`): the
  players arrive when it ends (s479 -> the world map 0-50, its `prop_show` steps hold 2 frames); the 3D map camera
  exists from the load; the load's own objects stop running with their event (`load_ids` cleared when it ends); an
  event over the world map holds the flyer (its animation ticks) except a warp's fade-in and the frame after.
* Switch Character's chosen leader is rebuilt (one idle rand(255)) when the screen closes (its 2nd leave frame).

**Blocker: OBJ VRAM purge** (`0x08011732(world, need)`): a text box (`0x08012030`: 0x1000 / 0x1000 / (portrait)
0x1000 / 0x800 bytes) needs room in the object tile allocator (best-fit blocks at world+0x694, 512 units of 64
bytes; room = largest free block << 6, `0x080116FC`). Short of room it suspends objects from the end of the list by
level (vfunc +0x18: actors 13 off screen / 14 on screen, the camera's object 15, effects lower) until it fits, and
brings them back later. It fires in crowded scenes: the world map's question boxes (sb_041 f11454: 6 patrols gone)
and every box of the ending gathering 4-24 (sb_044 f1985: 1-4 NPCs). Needs a model of every object's tile block
(sizes 1 / 4 / 8 / 16 / 64 units, allocation order) — not started.

Open: `sb_029` f703 / `sb_030` f490 — Switch Character's leave lasts 16 frames here vs 25 (the world restore's CPU
time; lag frames). `sb_034` f619 —
s502's sprite swap (`change_sprite` Buu 123 -> 126) is 1 frame late and the white fade timing. `sb_041` / `sb_044`: the OBJ VRAM purge above.

## 7. Side content (owner: the side-content agent, from 2026-10-02)

**Ownership change (coordinator):** HFIL / Janemba (incl. the Janemba -> Super Janemba handover in the field), Broly
and all post-game / optional areas and quests are the **side-content agent's**, no longer the Super Buu agent's
"after the main chapters" work. The Super Buu agent keeps the Super Buu arc main line (§6) from its start state on.
Shared tools are reused, not forked: `tools/superbuu/mksave.py` (start states as real save games), `tools/segments/
segment.py`, `wf-import`. Side-content segments are named `sc_*` (`data/segments/sc_*.json`), start states live in
`work/side/`, routes in `tools/side/`. Rules found go into wf-core / wf-script-core like any chapter's; additive.
Status and the side-content catalogue follow below as they are recorded.

## 8. Babidi's ship chapter (owner: the ship / Majin Buu chapter agent, 2026-10-02)

**Real route** (replaces the dev state): `re/buu/ship/run_route.py work/earth/earth_end.ss work/ship/real4 --real --log
--to c14_s342` plays the real game from the Earth agent's end state (Goku in 7-2 after the tournament) for 378,903
frames, every input from the driver (`ship_route.goals_real`, `ship_driver.ShipDriver`), no RAM writes:
Yajirobe's Senzu (s230 x2) -> the rest of the tournament (s261 .. s314) -> Goku to 70 in East District 439 (the 8-2
gate) -> Gohan to 100, Vegeta to 92 in Southwest Forest 8-3 / 8-6 (Vegeta fights Buu alone later) -> Onigiri at
Papaya Island's stall (s258) -> Babidi's ship (3 switches + Pui Pui, 3 switches + Yakon, 3 Switch items / 3 panels)
-> Gohan rests in reserve at 8-66's save point (reserve characters regain 5% HP / ~596 frames) -> Dabura (8-68) ->
Goku vs Majin Vegeta (8-70, four rounds) -> Vegeta's capsule back to 8-16 (s342). Marks `r00..r05`, `c04..c14` are
saved states (`run_route.py --from MARK`). `real4b`: c14 -> Vegeta vs Buu in the Giant Crater 8-71 (s343 / s344) ->
Goku in Rocky Canyon (`c15_after_buu`, 19,292 frames); `real4c` = both logs joined (398,195 frames).

Driver tactics that made it survivable without Senzu stocks: `flame_guard` (a Destroyer's FlameBreath 0x86 fans
along its facing ~100+ px: step out of the line, else flee, never through a door), `block_guard` (R toward a soldier
lining up a wave: 0x156 / 0x1E2 / 0x1E8, a front block takes 1/16), `rest` (stand still until a reserve character
is full), the `buu` fight: Buu takes damage only while **pacified** (0x0300302C != 0: Babidi's state 0xC9 enter
0x0802AEDA sets it for ~770 frames, his gesture enter 0x0802AD88 clears it; Buu wanders 0xA2 and does not fight back)
- block (R) turned to Buu otherwise, punch him while pacified.

**Segments**: `rec_segments.py work/ship/real4 work/ship/rsegs --prefix shipr` -> 338 segments (370,281 frames):
285 exact (208,315 frames); `--from-frame 378600 --first 338` on `real4c` re-cuts the last one through the Buu fight
(`work/ship/rsegs_c/shipr_338_8-16`, 19,591 frames, exact to 2,869). Gated in data (`shipr_*`, 23 segments, 18,988
frames): Babidi's ship stages 1-3 (switch rooms, the 8-63 hub buttons, the Destroyer rooms 8-45 / 8-55, the panels),
Vegeta's 8-13 Destroyer room; the 7 dev-state gates (`ship_*`) are retired.

Rules found (wf-core / wf-script-core), round 2:
* the nodes' face-the-player enters (FlameBreath 0x08023FA8, TurretFire 0x0802B684, SoldierThrow 0x080266D8,
  CyborgShot 0x0802F6A8, FacePlayer) all call 0x0801599C (24.8 difference, ties horizontal): `enemy::face_player` uses
  it in the field (battles keep the footprint rule cross_ts pins).
* drop scripts queued in one frame drop where *their* enemy died (`Cause::Defeat.loot`, set at the run's start by
  `field::start_cause`): two fighters dying together no longer share the last loot position (8-62).
* a nested `load_scene` (0x73 with a nesting host) spawns the area's objects after its on-enter script, as
  0x08009030 does (load scripts, then the object loop): 8-60's prop_show(1) blocks 3 frames, its fighters appear then
  (`Field.defer_objects` / `pending_objects`, `finish_arrival`).
* boss handovers (s327 / s328 / s336), motion controllers ticking while an event holds the field, shared soldier-throw
  wave, Destroyer flames during holds: round 1 notes below still hold.

Open: the pause-menu heal / stat screens' sound 40 timing (`snd: play 40` in ~25 grind segments, menu code under
concurrent edits), the Switch Character swap at save points (missing / cam a frame off), 8-16 s319's NPC idle cel,
s342's 8-17 camera_scroll_to one frame early (f2911 of the Buu-fight segment), RNG in a few 8-3 / 8-31 / 8-33 rooms,
Papaya 7-1 NPC facing. Next: the fat Buu arc from Goku in 8-70 (c15) to the Super Buu chapter's start (§6).

Round 1 (dev state, kept for reference): placed field enemies stand exactly at the def position; AlignWithPlayer keeps
the facing on a blocked step; shield graphics swap on node enter; loot x = position + foot_w / 2; breakable crit /
shake rules; energy waves collide head-on; `save_field_on_next_scene` + `load_scene*` keep the field
(`field::SavedWorld`), `return_to_field` restores it; `SfxStopMatch` counts as a stop. Test content caches are stamped
with the extractor hash (`wf_extract::cache`) and re-extract on mismatch.

## 9. Earth chapters: Gohan, Great Saiyaman, Videl, Goten & Trunks, the World Tournament (owner: the Earth agent)

**Start state** `work/earth/chapter_start.ss` (= `ow_end.ss`): the story playthrough route (route.py intro + goals)
**played by the real game** from the soft reset to Earth 2-1 (Gohan controllable, lf 40,415); end state
`work/earth/earth_end.ss`: Goku in 7-2 after the tournament (Spopovich / Yamu take Gohan's energy, s306 -> s260,
flag 50, lf 192,179) — the real start for the Babidi's ship chapter. Both rebuild from the committed key log
`data/story_playthrough/earth_route_keys.json` (`start`: 42,275 frames after the soft reset; `chapter`: 160,503).

**Route** (all played by the real game, closed-loop goals, `tools/story_playthrough/earth_route.py` +
`earth_driver.py`; `earth_play.py` plays it and saves `work/earth/seg_<name>.ss` per segment, `earth_keys.py` checks
the chain and writes the key log, `earth_savekeys.py` the data file): house -> lunch (s110) -> Nimbus (s111 / s124)
-> the bank robbers (s125) -> the lecture (s159) -> s129 -> world map circle (s130 -> #515, the 3D map flown to West
City: `fly` goal) -> Bulma (s193) -> the 2402 capsule (map item, s197) -> the Great Saiyaman (s194) -> s131 ->
late for class (s141 / s160) -> the circus clowns (s142) -> training in East District 439 (Energy Blasts from range,
level-ups refill HP: `levelup` goal) -> the circus fight (s143, five fighters) -> Musuka (s169) -> the tank (s168,
sniped from 90 px, waiting out of its row for EP) -> Videl (s132 / s133) -> s134 -> Chi-Chi (s106 / s113) -> the
level-65 gate -> Goten's rock drill (s119: dodge 10 thrown rocks) -> Goten SS fight -> Trunks vs Vegeta in the
gravity chamber (dash in and punch: he keeps 60 px away) -> Bulma (s184) -> Papaya Island: register Trunks and
Gohan (s239), s262, the punching machine (0xA7), the Junior Division, s288 -> Videl vs Spopovich (melee) -> s304 ..
s306 -> 7-2. Stat points: STR / POW through the Status page (`alloc`), a Rice Ball through the Inventory (`menu`).

**Segments**: master recording `work/earth/rec/earth_all.json` (segment.py record from the chapter start with the
key log), cut at every importable area entry by `earth_cuts.py` into `work/earth/segs/e_W_A_n.json` (local; run with
`SEGMENT_DIR=work/earth/segs`).

Rules found (wf-core / wf-script-core / wf-import; playthrough, lockstep, cross_ts, field_lockstep, data segments green):
* a map trigger's inline script that blocks (prop_hide / prop_show at Capsule Corp's doors) holds the field in the
  VM's nested loop: no object thinks, animations tick, a walking player keeps its walk state (`field::hold_nested`,
  set by the hosts when a `Cause::Trigger` script blocks).
* `prop_hide` / `prop_show` of a prop the area does not have is dropped at once (3-10's on-enter: no hold).
* a script's immediate `warp` inside an event defers the arrival to the new area's on-enter script when the host
  nests (`Field.nest_load`; 3-1's on-enter prop_hide: the player appears 2 frames after s124 / s129's warps).
* `set_character` always re-enters idle (one rand(255)), also with the same sprite (s124).
* on-defeat callbacks of SelfDestruct enemies run from the death effect after its delay, after the drop script
  (`Field.defeat_later`; 3-1's robbers: s126 8 frames after the KO); the trivial event releases the held update.
* an event a script starts (run_event from a trigger's code: the world map circles -> s515) stops the field like an
  event trigger (`fieldnpc::start_event` in `begin_event` at depth 0 when nothing holds the field yet).
* an item-get started in the player's update holds the rest of the object pass; map NPC behaviours pause while it
  shows (4-30's scientist).
* `warp_fade` (0x76): the player goes idle at the call, the script camera stays until the load, a 64-frame fade-out
  (brightness by 4, starting a frame after the call), no destructor idle draw; objects an area load makes inside an
  event are not held by it (`Field.load_ids`: the world map's patrolling encounters), the world map flyer reads the
  keys during an event; the 3D fade-in is 64 frames (brightness by 4).
* the Ki Blast state is not "attacking" for IfPlayerAttackingMe (TS reference updated, cross_ts green).
* the HUD's low-HP beep: HP <= max / 5 (not 0), sound 41 at volume 32, more than 60 logic frames after the last one
  (stamp = the area load's frame), after the actors' update.
* a held `face_actor` keeps turning toward its target until the wait mode it finished in ends (`Track.live`; s160's
  class follows the announcer's walk in the same batch, s084 still holds).
* harness: the 3D world map compares `Flight.cam`; actors are not compared while a pause-menu page replaces the world;
  `SEGMENT_RNG=1` with `SEGMENT_TRACE=a..b` prints every RNG call's source location.
* content: `fieldcontent` also takes a map with `mode7.json` as the 3D world map (re-extracted content lists no
  unsupported panorama layer any more: field_lockstep world_map was red).

Second pass (2026-10-03) rules, each from these recordings:
* Goten's rock drill (2-90): TauntAnim is the throw controller 0x0804E030 (tick 8: two rand(64), a rock 0x08033F28
  from the facing offset toward the player's footprint centre, 22 updates of two gravity substeps, then a kind-1
  energy message (mult 0) - accepted queues s120 - and a puff); tick 32 -> SetAnim(0).
* RunScript (and the rock's hit) append an event object to the world list: it starts in the next frame's list pass
  (`field::queue_run`); in the frame such an event ends the enemies and the players' form controllers still run.
* An event holding the field holds the players' passive update (regen, Super Saiyan drain).
* `play_anim` on an enemy uses the enemy SetAnim table's rule (raw 11 once-stop for Goten's power-up).
* The player's / Gohan enemy's Ki blast box follows its record: 8x16 / 16x8 from tick 16 (2-9 #9, 2-10 #1).
* A bomber hurt mid-throw throws nothing; the tank's TankShoot is a controller (shell at anim frame 2 from
  0x08757354, record box, 48 + rand(2) on a hit); the shell's handler destroys it on any kind-0 message and accepts
  (Gohan's blast shoots shells down); IfPlayerInLineOfFire needs IsAnim(0) and > 16 px along the facing.
  (Battle reference updated too: site `ai.ts` / `enemy.ts` / `projectiles.ts`, TS dumps regenerated, cross_ts green.)
* A map item's event after the item-get completes the interrupted object pass when it ends (4-30).
* An NPC left in a `set_pose` keeps it frozen after the event (3-3).
* Menus: a script-opened menu that closes without handing the frame back holds the world that frame
  (`Menu.closed_this_frame`); lockstep steps menu screens per video frame and counts a menu's black page as black -
  `shop` is exact with `ui/` content. The punch machine: its wait mode updates the world, and a wait ending on the
  hidden (blinking) marker starts another (the marker blinks on the boot counter, `Story.lf_bias`).

Third pass (2026-10-03):
* Talk events (`HostState.talk_event`, set by the hosts for `Cause::Talk`) exit at the top of the frame: a Director
  wait followed only by story / HUD natives, a closing text box, or a modal scroll (the frame's world update then
  runs, `from_scroll` -> `held_event`) all give the field its update in the exit frame (4-27 #1 / #2, 4-20 #5).
  Map-trigger cutscenes (s073, s159, s130) keep the old rule (no catch-up after a native).
* The 3D map's view rect (`fieldworld::view_rect`, 0x0802ED98, VERIFIED against RAM) and the draw-time facing of
  enemies inside it (0x08010010; heading obj+0x1AC from `cutscene_move`'s float atan2). 3-1 / 3-11 pass the old
  airship-facing frames; their current first divergences (a zero 3D camera in the load frame, the flyer's cel) come
  from the in-progress flyer-hold change in `fieldworld::flyer_update` (another agent's, uncommitted).

Results (90 segments, 104,026 video frames): **79 exact to their end**, 88,169 frames continuously matched; all 79
gated in `data/segments/earth_*.json`.

Open, with what was found:
* Status-page sound 40 / 39 (2-9 #13, 2-11 #4, 3-15 #4; also the ship chapter): a page redraw takes 619..668
  scanlines; whether the next iteration slips 3 or 4 frames depends on it, and the variation (up to ~16k
  instructions) is the software sound mixer (IWRAM 0x03000880.., resampling loops per active voice) running inside
  the redraw window. Exact timing needs the mixer's CPU load per frame (active voices / pitches) - not modelled.
* 7-2 f1200 and 7-3 f2995: NPCs parked off the world list (0x08011744: objects give up memory, vt+0x18 -> 2, moved
  to the list at +0x40C) when a portrait text box allocates - the OBJ / heap purge the Super Buu agent is modelling.
* 2-8 #1 f5646: the camera's 3-frame wobble is one step ahead after a long load frame; 3-3 #4 f1479: a soldier's
  AlignWithPlayer stalls in the core while the game's keeps cycling states; 2-8 #15 / 4-1 #2: 3D map (see above).

**Side-content catalogue** (data survey 2026-10-02; flags = story flags): Broly turns out to be **mainline** (Dragon
Ball saga: his defeat s436 sets flag 111, which Dragon Ball #4 s439 needs) — shared with the Super Buu agent, whose main line
reaches it: **whoever reaches Broly first takes it** and notes it here (not taken yet, 2026-10-03). The optional
content: the dungeon level-gate tiers (s517: tier 2 after flag 128, tier 3 after 149 at 8-2 / 10-1 / 11-1 / 14-1 /
15-1, 9-1), per-visit gates (s518: 2-33 Gohan L140, 8-7 Vegeta L120, 13-1), the East District hidden dungeon
(2-33..2-47), Hercule Exhibits (25, s170 at 3-20) and Z-Fighter Exhibits (15, s189 at 4-23) -> Gogeta's Fusion Dance,
the Lighthouse Keeper trade chain (s347), the ninja bandana bounty (s391), fishing -> Korin, the Capsule Corp NPCs
(s185 / s199), Yajirobe, world-map ambushes (s509), Heaven / Dabura (22-1, s490..493), and the post-game (flag 149 ->
s190): the Airship "XJ-6" (12-x, s385 / s389), the tier-3 sets, the Barren gate (20-4, Goku L175), exhibit
replacements. Optional HFIL chests: s440 / s441 / s442 (flags 213 / 214 / 215).

**Start states** (`tools/side/mksave_side.py CHAPTER out.ss`, the §6 builder with extra chapters):
* `janemba` — flags through 120, a save at 1-5's arrival point (s066 on the load), current char Goku; route
  `tools/side/jroute.py` (1-5 pad -> 0-51 -> HFIL 18-1). The HFIL corridor 18-1 .. 18-11 (Poltergeists, Living Dead,
  Lich, Nosferatu: levels 116..125) is not routed yet (the driver's melee loses to them).
* `janemba_arena` — flags through 123 + 181 / 183 / 284, a save at the 18-13 save point (s448 on the load), Goku /
  Vegeta level 200 (liberty: the route's melee bot needs it for the two Super Janemba fights).
Route `jroute.py --route arena --wait 1` (driver goals + `tools/side/sidedriver.py`: `bossx` = boss with the area's
exits blocked and their push keys masked, `bosst` = fight given enemy types in priority order, rotate the approach
side when the target's HP stops falling, dodge the given types): 41,967 frames from the load, real game: s448, s449,
Janemba (minis first), s450 the handover to Super Janemba, s451 the fusion (Watch This!), Gogeta vs Super Janemba,
s452 (flag 125). Master `work/side/seg/sc_arena_master.json` (local), segments cut at the 18-14 entry:
`data/segments/sc_janemba` (6,000 frames, gate) and `work/side/seg/sc_janemba_full` (40,688 frames, local).

| segment | from | frames | matched |
|---|---|---|---|
| `sc_janemba` | 18-14 entry | 6,000 | 6,000 (gate) |
| `sc_janemba_full` (local, 22 MB) | 18-14 entry | 40,688 | 35,925: Janemba, the handover, Super Janemba, the fusion |

Rules found (field only where battle is pinned by the TypeScript cross-check):
* Janemba's body parts: created at body + (-16, -16) / (48, -16) / (16, -40); then part 0x37's AI node (0x0803D0C4)
  places the chain every frame its AI runs at (16, -40) / (-12, -21) / (44, -21) + a y delta by the body's cel:
  raw 10 [-1, -2, -1], raw 20 [15, -3, -5], else [0, 1, 0, -1] (0x0803CF46 / 0x0803D046 / 0x0803CFC2; read before
  the body's tick; a switch into raw 20 shows a frame late, and under it the previous frame's cel). Battle keeps the
  measured fixed offsets (-12, -22) ... until the TypeScript reference is updated.
* Hand strikes: the node's first update gives the arm part (A 0x34 / B 0x35) the strike controller: raw 19 (8 frames
  a cel, no start draw), the hand (gfx 0x96, spawn record 0x0877B050, at the player's frame-start position + 4 / -20)
  on its 16th tick - before the player's update, so a punch that hurts Janemba in that frame meets it - and on its
  56th the hand's SetAnim(0) + its tiles freed, then the arm's SetAnim(0). JanembaPartFlash cuts a running strike:
  the controller's exit idles (rand(255)) and destroys its hand; the node ends with it. A hand takes the player's hits
  (crit chance + roll, nothing else) only until its controller ends.
* Janemba's parts are enemy objects for hits too: a hit under one computes its damage (crit chance + roll) only.
* Field hit tests (the player's melee on enemies, MiniRollAttack's hit, IfPlayerNearAndHittable's centres) use the
  body box 0x0800FE78 at the ground position: the height does not lift it (Janemba 66 px up in the ascent, hopping
  minis).
* Ascend = raw 20 one pass (cel 0 shows 9 frames); descend rises from its 2nd frame, lands after 67, the animation
  plays back (cel 1 for 8 frames, then cel 0), 16 frames later the shake (0x46, 0x1E) and a puff per missing mini
  (rate 0x1000 + rand(0x1000)), 16 more: that many minis (a mini killed in between is not replaced) and SetAnim(0).
* A mini's hop counter is its controller's: it counts on while an event holds the field (no moves).
* A Janemba part's hurt end (SetAnim(0)) also runs while an event holds the field (the defeat scene).
* Super Janemba: TeleportNearPlayer / TeleportChain4 take hits off from the vanish until 18 frames after the jump; the
  jump lands at the player's whole-pixel position + the side offset, moved to the first walkable tile-aligned spot of
  0x08009816's square spiral, keeps the facing, and leaves the grid alone (the old footprint stays marked, the new one
  is taken as its own); TeleportNearPlayer ends with the landing animation (36 frames, SetAnim(0)); the chain's
  controller counts from the frame after the enter, its first update steps the side counter 0x03003228.
* The field's shake slot holds one shake at a time: a newer one suspends the older until it ends.
* Mini-Janemba: the roll's dash is a collision-checked step without sliding; a hopping enemy meets the walls.
* Remove / become_map_object / glide lift an NPC's walk mark (s101); the field enemies' AI does not run in an event's
  exit frame (the game's next frame does).
* Enemy grid (field, checked by dumping the live grid, map object [0x080726AC + 8] + 0xB0, against ours with
  `SEGMENT_GRID` / `SEGMENT_ORDER` in the segment trace): a hopping enemy keeps its marks (height does not detach);
  every enemy constructor marks its footprint - the body parts once where they are created (the mirror node moves them
  without marks), the strike hands too. Unmarking clears whatever shares the tiles, which is what later steps collide
  with.
* JanembaPartFlash (0x0803999D, Janemba's on-hit tree): every part plays the hurt animation from the hit; each part's
  own end SetAnim(0)s it (idle draw). ShockStun (state 0x0A's reaction) lasts the hurt animation, then PopState
  (0x104 resumes: WaitMinisDefeated -> the strikes).
* Scenes: `screen_rect_x/y_at_actor` (0xCB / 0xCC) centre on the footprint centre in the field (Gogeta's split
  places the walk from it); frames passing while a batch is open (0x38 .. 0x3A, here flash_white + fade_to_white)
  do not run the world update; Watch This!'s closing frame runs the scene's tracks, the script and the cameras but not
  the objects.
* Debug: `SEGMENT_PUPPETS=1` in the segment trace prints the scene puppets' animations and the director's tracks.

Open at 35,925 (full run): after the batch commit an instant command pair (face, set_pos) finishes a frame later in
ours than in the game (the director's wait ends one frame late; shared scene code, s101 / s073 rules pin the current
order).
