# Segments: start wf-core from any real-game emulator state (crate `wf-import`)

STATUS: in progress (2026-10-02). API below is the contract; additive changes only.

Goal: match campaign chapters against the real game **in parallel**. Each chapter agent takes an emulator save
state somewhere in the campaign, records the real game from there with its own input (a *segment*), imports the
start state into wf-core, replays the same input and reads the first divergence. The full New Game playthrough
(`wf-script-core/tests/playthrough.rs`) stays as it is.

## 1. API (`engine/crates/wf-import`)

```rust
use wf_import::{GbaRam, import_emulator_state, import_story, StoryHost, segment};

// the GBA work RAM at the cut (IWRAM 0x03000000 32 KiB, EWRAM 0x02000000 256 KiB)
let ram = GbaRam::from_json(&doc["init"]["ram"])?;          // segment files embed it (zlib + base64)
let story: Story = import_story(&data, &ram);                // flags, words, inventory, zenie, records, party...

// a story host = the playthrough's per-frame script loop as a library (one Rhai runtime on the shared host)
let mut host = StoryHost::new(&data, &files)?;               // files = wf_script::export(rom, set)
let imp = import_emulator_state(&data, &ram, &mut host)?;    // -> Imported { world, prev_keys, lf, kind, notes }
let mut sess = host.session(imp.world, imp.prev_keys);
for keys in route { host.step(&mut sess, keys); }            // one logic frame (the playthrough's host order)
```

* `import_emulator_state` works at **area-entry cuts**: the end of the logic frame whose EnterArea ran (the
  segment recorder finds them). wf-core reloads the area itself from the imported story state; the load's scripts
  (on-enter) run through the host passed in (`ScriptRunner` trait; `StoryHost` implements it).
* `segment::load(path)`, `segment::replay(&data, &files, &doc) -> Report { compared, first, counts }`: the
  segment harness (same comparison as the playthrough: area, every actor's position / facing / animation, camera,
  fade, RNG, sounds, text boxes, zenie).

## 2. Making a segment (one command)

```
.venv/bin/python tools/segments/segment.py cut --from data/story_playthrough/opening.json --at area:1-12 --frames 3000 --name ow_1_12
.venv/bin/python tools/segments/segment.py record --state work/states/foo.ss --keys keys.json --frames 3000 --name foo
cd engine && SEGMENT=ow_1_12 cargo test -p wf-import --test segments -- --nocapture
```

(details below as they land)
