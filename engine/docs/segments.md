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
(`{iwram, ewram}`: zlib + base64). Optional `"expect": {"matched": N}` in the file makes the test hold the segment to
N continuously matched video frames instead of all of them (for segments that are work in progress).

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

See the table at the end (filled by the proof run).
