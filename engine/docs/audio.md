# Audio: the Webfoot sound driver in Rust (`wf-audio`)

Buu's Fury does not use Nintendo's MP2K: it runs Webfoot's own **tracker sequencer + software mixer**. `wf-audio`
re-implements it from the disassembly and reads all song / sample data straight from the ROM — no emulator.
The exact path reproduces the game's 8-bit FIFO output **bit for bit** (see Validation).

Addresses are GBA bus addresses (Thumb routines without the +1). Research tools: `crates/wf-audio/re/`
(`wfdis.py` disassembler, `tables.py`, `blscan.py`, `fifo_ref.py` capture harness).

## 1. API

```rust
let bank = wf_audio::bank::SoundBank::from_rom(&rom);       // samples, instruments, SFX, songs, tables
let mut d = wf_audio::driver::Driver::new(&bank);           // the game's sound object (15 tracks + 16 SFX voices)
d.play_song(1); d.play_sfx(20, 64, 0); d.stop_sfx(20); d.stop_all_sfx(); d.set_master_volumes(4, 6);
let pcm: Vec<(i8, i8)> = d.render(n, 267);                  // exact FIFO A/B output, 15 978.3 Hz
wf_audio::render_song(&bank, id, secs);  wf_audio::render_sfx(&bank, id, vol, max_secs);
wf_audio::hq::render_song(&bank, id, secs, 48000, true);    // float, interpolated, any rate (not bit-exact)
wf_audio::extract(&rom, &mut content);                      // content files (default extraction, ~0.1 s)
wf_audio::extract_rendered(&rom, &mut content, secs);       // + audio/rendered/*.wav (opt-in, slower)
```

`Driver::song_structure(bank, id, max_rows)` walks the sequencer without mixing and returns the row log and the
loop (first repeat of the state *(order, row, speed, samples/tick)* at a row start).

## 2. ROM data

| what | where | format |
|---|---|---|
| song table | `0x083BB78C`, 53 × 20 B | `u32 pattern_table, u32 order_list, u32 instruments (0x0836D18C), u32 flag_presets (0x0836D540), u8 tempo, u8 speed, u16 0` |
| order list | per song | `u8` pattern indices, `0xFF` = end → back to order 0 |
| pattern | `pattern_table[i]` | `u8 rows`, then per row: cells until a `0x00` byte |
| instruments | `0x0836D18C`, 79 × 12 B | sample entry (below) |
| SFX table | `0x083192A8`, 64 × 12 B | sample entry |
| sample entry | 12 B | `u32 pcm (signed 8-bit), u8 loop_mode (bit0 loop, bit1 ping-pong), u8 0, u16 loop_start, u16 length, u16 rate_hz` |
| pitch table | `0x087B6724` | `u32` per note, 0x4000 = 1.0 (note 60), 12-TET |
| slide tables | `0x087B6A04` (up, x/16384), `0x087B6C84` (down, x/65536); the 64 `u16` before each = fine tables (x/32768, x/65536) |
| vibrato sine | `0x087B6E84` | 64 × s8, amplitude 64 |
| effect handlers | `0x087B6904` | 32 Thumb pointers |

Note: header byte +0x11 is the **speed** (ticks per row), not a channel count (`tools/audio/AUDIO.md` calls it
channels). All songs share one instrument table and one preset table.

**Cell encoding** (row reader `0x0803EEE4`). Lead byte `b`: channel = `(b & 0xF) − 1` (0..14). Flag byte:
`b >> 4 == 0` → next byte (also remembered per channel), `== 1` → the channel's remembered flag byte, `≥ 2` →
`presets[(b >> 4) − 2]` (`04 61 70 80 40 25 08 34 07 E1 0F 69 16 2D`). Then, by flag bits: `0x01` note byte,
`0x02` instrument, `0x04` volume (0..64), `0x08` effect + param. Bits `0x10/0x20/0x40/0x80` reuse the channel's
buffer instead: `0x10` re-trigger the last note, `0x20` keep the voice's current sample (no instrument reload),
`0x40` re-apply the last volume, `0x80` re-apply the last effect. Note `0xFF` = note off. The cell buffer persists
across rows, patterns and **songs**.

## 3. Sequencer

Player (`0x030038A4`): `+0` ticks left, `+1` row, `+2` rows, `+3` speed, `+4` tempo, `+6` samples per tick
= `40000 / tempo` (integer), `+8` countdown, `+0x10` order. Mixing `0x0803F07C` runs the countdown in output
samples, so ticks fall on exact sample positions independent of the frame chunking. Tick `0x0803EFD0`: when the
tick counter reaches 0 → next row (load cells), row handler per track, counter = speed; otherwise each track's
per-tick handler (mode bits 0-1 of track `+0x4A`: 1 = only while the note is on, 2 = always). After every tick
each voice applies its pending changes (`0x0803E0B6`). PlaySong (`0x0803F110`) clears only the track mode, cell
flags and voice flags — effect memories, vibrato phase, pans and the cell buffer carry over from the previous song.

**Effects** (row handler → per-tick handler). Memories: volume-slide param `+0x4C`, pitch param `+0x4D`,
retrigger `+0x4E` (a 0 param reuses the memory).

| id | name | behaviour |
|---|---|---|
| 0, 3, 9, 10, 13, 14, 16, 18, 19, 22, 23, 25, 26, 28–31 | none | trigger the note / set the volume (`0x0803E4E4`) |
| 1 | set_speed | trigger; ticks per row = param |
| 2 | pattern_jump | trigger; jump to order `param` after this row |
| 4 | volume_slide | `x0` up x / tick, `0y` down y / tick (`F0`/`0F` also once now), `xF` fine up, `Fy` fine down; clamp 0..64 |
| 5 / 6 | pitch_slide_down / up | per tick pitch × `down[p]`/65536 or × `up[p]`/16384; `Fx` fine (once), `Ex` extra fine (fine tables) |
| 7 | tone_portamento | no trigger; slide toward the note's pitch by the slide tables, stop at the target |
| 8 | vibrato | trigger; depth = (p & 15) × 4, speed = p >> 4; pitch = base × fine-up[d]/32768 (d ≥ 0) or × fine-down[−d]/65536, d = depth × sine[phase] >> 6 |
| 11 | vibrato_volume_slide | vibrato + volume slide (param = slide) |
| 12 | portamento_volume_slide | tone portamento + volume slide; **game bug kept**: the up+volume-up combination snaps to the target while still below it |
| 15 | sample_offset | trigger; position = param × 256 samples |
| 17 | retrigger | trigger; restart the sample every (p & 15) ticks |
| 20 | set_tempo | trigger; samples per tick = 40000 / param |
| 21 | fine_vibrato | like 8 with depth = p & 15, no "note on" check |
| 24 | set_pan | trigger; pan = param (s8, −64..64) |
| 27 | note_delay | trigger after `param` ticks |

## 4. Voices and mixer

Voice (0x40 B; tracks add 0x20 B of effect state): `+0` position (18.14 fixed), `+4` step (negative while a
ping-pong loop runs backwards), `+0xC` packed gain `L | R << 16`, `+0x10` pending flags (0x10 volume, 0x08 load
entry, 0x04 restart, 0x02 pitch), `+0x12` volume, `+0x13` pan, `+0x30` pitch, `+0x34` master volume.

* gain: `L = (master × volume × (64 − pan)) >> 10`, `R = (master × volume × (64 + pan)) >> 10` (u16).
  Master = Options setting: music **4**, SFX **6** by default (`0x08006B36`; range 0..8). Gain 0 skips mixing.
* step: `(((pitch × rate) >> 14) × 0x1063 + 0x7FF) >> 12` (0x1063/4096 ≈ 16384 / 15978.3).
* inner loop (ARM `0x03000868`): `buf[i] += gain_packed × pcm[(pos + i·step) >> 14]` — **one 32-bit
  multiply-accumulate for both channels**, so the right half carries the left half's borrow. Drop-sample, no
  interpolation. Loop handling `0x0803E152` splits the run at the end with BIOS `Div` (truncating):
  forward → `pos += step·k − (end − loop_start)`, ping-pong → reflect, one-shot → stop (clears the track's
  "note on").
* output (`0x03000A4C`): `left = clamp(i16(w) >> 6)`, `right = clamp(i32(w) >> 22)` to s8; DMA1 → FIFO A (left),
  DMA2 → FIFO B (right), timer 0 reload 0xFBE6 → **16 777 216 / 1050 = 15 978.3 Hz**.
* order per update: SFX voices apply pending changes at the start of each frame update (`0x0803E3F6`), music
  mixes (with its ticks), then the 16 SFX voices; PlaySfx (`0x0803E378`) takes the first free voice or drops.

## 5. Content files (`audio/`)

| file | content |
|---|---|
| `index.json` | `songs[]`, `sfx[]` (`id`, `index`, `name`, `file`), `samples[]`, `arena_music`, `boss_music`, `events` (simulator triggers) |
| `driver.json` | mix rate, track / voice counts, default master volumes, formulas (step, gain, output stage), `cell_flags`, `effects` (id → name), `pitch_table`, `slide_up(_fine)`, `slide_down(_fine)`, `vibrato_sine` |
| `instruments.json` | `instruments[]`: `id` `inst_NN`, `sample`, `file`, `length` (samples), `rate_hz`, `loop` {`mode` forward/pingpong, `start`, `end` (samples)} or null, `loop_mode_raw`, `rom` |
| `sfx/sfx_NN.json` | same sample-entry fields + `name`, `trigger`, `default_volume` (0..64, the volume the game passes), `duration_s` (null if looping) |
| `songs/song_NN.json` | `tempo`, `samples_per_tick`, `tick_hz`, `speed` (ticks/row), `channels`, `orders`, `loop` {`start_order`, `start_row`, `start_sample`, `end_sample` (15 978 Hz samples), `start_s`, `end_s`} (null = no repeat found), `effect_names`, `patterns[]` {`rows`: [[cell…]…]} |
| `samples/sample_NNN.wav` | the unique PCM blocks, 8-bit mono WAV at the first user's rate, `smpl` loop chunk |

Cell object: `ch` (track 0..14), `flags` (raw flag byte, §2), and only the fields present in the cell: `note` +
`n` (name, note 60 = "C5" = the sample's own pitch), `inst`, `vol`, `fx` + `param`. Modding: edit cells / orders /
samples; a player built on `Driver` semantics needs only these files. Names (`name`, `context`, `trigger`) are our
curation (`crates/wf-audio/data/names.json`, from `data/buu/audio.json`), not ROM data. Rendered WAVs are not part of
the default extraction (`extract_rendered`).

## 6. Validation

* **Bit-exact vs the game** (`tests/fifo_exact.rs`): `re/fifo_ref.py` captures the exact bytes the game writes to the
  FIFO rings (`0x02000000` / `0x02000400`) through the frozen-game harness, for every song over intro + one full
  loop (10–201 s, 34 min total) and every SFX (4 s). With the track RAM the game had at the song start loaded into
  the driver (`Driver::seed_track_ram`, see the carry-over note in §3): **53/53 songs and 64/64 SFX identical, every
  sample**. A fresh `Driver` (all memories zero) is what a freshly booted game would play.
* **vs the site's emulator rips** (`tests/rips.rs`, `work/audio/cache/…_v7`): these are mGBA's 32 768 Hz output
  (blip_buf resampling, DC high-pass, gain ≈ 0.75 of FIFO×256). After sample-and-hold upsampling, band filtering
  both sides (~32 Hz–6.5 kHz) and alignment: songs correlation median 0.9990, worst 0.904 (song 16), 0.950 (song
  10) — those two use vibrato whose phase carries over from whatever played before in the ripper's run (state we
  seed from our own capture order, which differs); SFX median 0.992, worst 0.943 (SFX 62/30, 16 kHz samples whose
  images above 8 kHz mGBA's resampler treats differently). **Loop lengths** from the sequencer state equal the
  ripper's to ≤ 0.02 ms on 51 of the 52 looping rips (song 41: see below).
* `tests/content.rs`: export layout, WAV round trip, decode warnings (none), hq renderer tracks the exact one
  (correlation 0.986 drop-sample, 0.972 interpolated). Extraction: 263 files, 14.6 MB, ~0.1 s (release).

**Findings about the existing rips** (not changed here): song 41's loop in `data/buu/audio.json` is 54.6 ms too long
(the ripper's own seam correlation was 0.77; `examples/loopcheck.rs`: our 25.7629 s period repeats with correlation
0.984, the ripper's 25.8175 s with −0.01). SFX 29's rip contains a short click: the harness patches its Thumb stub at
`0x082ED300`, which is inside SFX 29's sample (silent tail at `0x082EBFEE`+4882) — the real game plays silence there.

## 7. Deviations / limits

* `hq` (float, interpolated) is intentionally not bit-exact; use `Driver::render` for the reference sound.
* State carry-over between songs is reproduced only if the caller keeps one `Driver` across songs (as the game
  does); a new `Driver` per song = fresh boot.
* Not modelled: the GBA's analogue output stage (the 8-bit FIFO stream *is* the digital output), the IRQ-driven
  catch-up mixing (it only changes chunk sizes, which do not affect music; SFX start on the next update like the game).
* Effect ids ≥ 32 would read past the handler table in the game; none occur.
