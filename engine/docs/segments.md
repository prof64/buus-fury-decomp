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
   (35 / 25 / 25 / 15 %), RecalcStats 0x08013C66: Goku / Vegeta / Goten / Trunks 125, Gohan 115; zenie 18,000;
5. GameData +0x8A0 = the 18-13 save point (HFIL, next to Janemba's arena), +1 bit 0; written to EEPROM slot 0 by the
   game's own slot writer 0x080049F2 (checksum, EEPROM protocol); then soft reset -> title -> Game 1 -> the game's
   load. Every RAM byte of the resulting state comes from the boot + load path (`work/superbuu/libemu_sb.dylib`:
   tools/emu/shim.c + savedata access, `tools/superbuu/build_sb.sh`).

Validation: the title's Select Game shows the slot as "Super Buu Saga"; the game plays the whole chapter from it with
no intervention (King Yemma s059 -> s067 -> the Z-Sword s464 -> Kami's Lookout -> HTC sparring -> the fusion (Watch
This!) -> Gotenks vs Super Buu -> 6-20), so every flag / party bit these scripts read holds what the story needs.
Known liberties: the levels / zenie / empty inventory are a plausible player's, not a recorded one's.

**Route** `tools/superbuu/sbroute.py` (driver goals + `tools/superbuu/sbdriver.py`: `walke`, `areaw`, `fastmash`,
`prog`, `node` (breakpoint on the scene step: timed presses, e.g. the Z-Sword swings 47 frames after node 19 / 33
start), `eventsw` (Watch This!: reads the drawn sequence from the mode object), `boss`): 40,046 frames, keys in
`work/superbuu/sb_keys.json`; master recording `SEGMENT_DIR=work/superbuu/seg segment.py record --keys ...
--name sb_master`, segments cut at importable area entries.

| segment | from | frames | matched | notes |
|---|---|---|---|---|
| `sb_18_12` | 18-12 (HFIL exit) | 178 | 178 | |
| `sb_18_01` | 18-1 | 600 | 600 | Goku's pad -> Other World travel map 0-51 -> Yemma's Office -> 1-5 |
| `sb_01_03` | 1-3 | 7,735 | 7,553 (gate) | King Yemma s059 + the Janemba flashback scenes (0-17 / 0-16 / 0-96) |
| `sb_01_05b` | 1-5 #2 | 11,000 | 6,541 (gate) | s067 -> 19-3 / 19-7 (Gohan), the Z-Sword power struggle and boulder |

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

Open: `sb_01_03` f7569 — `return_to_field` restores the field's saved objects (NPC positions / animation states), the
remake reloads the area fresh (the ogres at Yemma's door are where they stood before the scene in the game).
`sb_01_05b` f6541 — `play_anim(1, 0)` on Goku (spawned `spawn_actor(1, ..)`, holding `set_pose(1, 12, 0)`) leaves the
pose in the game; the remake's rule restarts idle on any non-idle actor (needs the NPC SetAnim / vt+0x2C rule).
Not recorded yet: the HTC fights, Gotenks, Ultimate Gohan, Vegito, inside Buu, Kid Buu, ending (route continues from
`m_after_gotenks`).
