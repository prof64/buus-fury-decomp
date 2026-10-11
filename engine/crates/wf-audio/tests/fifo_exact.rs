//! Sample-exact check of the driver against the game's own FIFO output, captured in the emulator by
//! `engine/crates/wf-audio/re/fifo_ref.py` into `work/audio/fifo_ref/` (research data, not shipped).
//! Skips when the ROM or the captures are missing.

use wf_audio::bank::SoundBank;
use wf_audio::driver::Driver;
use wf_rom::testutil;

struct Ref {
    pcm: Vec<(i8, i8)>,
    pans: Vec<i8>,
    ram: Vec<Vec<u8>>,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn load(kind: &str, i: usize) -> Option<Ref> {
    let dir = testutil::repo_root().join("work/audio/fifo_ref");
    let b = std::fs::read(dir.join(format!("{kind}_{i:02}.bin"))).ok()?;
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{kind}_{i:02}.json"))).ok()?).ok()?;
    let pans = j["tracks"].as_array()?.iter().map(|t| t["pan"].as_u64().unwrap_or(0) as u8 as i8).collect();
    let ram = j["track_ram"].as_array().map(|a| a.iter().map(|h| unhex(h.as_str().unwrap())).collect()).unwrap_or_default();
    Some(Ref { pcm: b.chunks_exact(2).map(|c| (c[0] as i8, c[1] as i8)).collect(), pans, ram })
}

/// Returns (samples compared, first mismatch index).
fn compare(bank: &SoundBank, kind: &str, i: usize) -> Option<(usize, Option<usize>)> {
    let r = load(kind, i)?;
    let mut d = Driver::new(bank);
    let n = r.pcm.len();
    let out = if kind == "song" {
        d.set_track_pans(&r.pans);
        for (t, ram) in r.ram.iter().enumerate() {
            d.seed_track_ram(t, ram);
        }
        d.play_song(i);
        d.render(n, 267)
    } else {
        d.play_sfx(i as u8, 0x40, 0);
        d.render(n, 267)
    };
    let bad = out.iter().zip(&r.pcm).position(|(a, b)| a != b);
    Some((n, bad))
}

/// The capture harness (tools/audio/buu_sound.py) patches a Thumb stub into what it took for ROM padding at
/// 0x082ED300 — that is inside SFX 29's sample (0x082EBFEE + 7635 bytes, a silent tail). Reproduce the patch so
/// the comparison stays exact (the existing rips of SFX 29 carry this click as well; the real game data is silence).
fn apply_harness_patch(bank: &mut SoundBank) {
    const STUB: u32 = 0x082E_D300;
    const CODE: [u16; 16] = [
        0xB590, 0x4F07, 0x683C, 0x2C00, 0xD008, 0x6878, 0x68B9, 0x68FA, 0x693B, 0xF000, 0xF804, 0x6178, 0x2100, 0x6039,
        0xBD90, 0x4720,
    ];
    let mut patch = vec![0u8; 0x24];
    for (i, h) in CODE.iter().enumerate() {
        patch[2 * i..2 * i + 2].copy_from_slice(&h.to_le_bytes());
    }
    patch[0x20..0x24].copy_from_slice(&0x0203_FFD0u32.to_le_bytes());
    for s in bank.samples.iter_mut() {
        for (k, &b) in patch.iter().enumerate() {
            let a = STUB + k as u32;
            if a >= s.rom && ((a - s.rom) as usize) < s.pcm.len() {
                s.pcm[(a - s.rom) as usize] = b as i8;
            }
        }
    }
}

#[test]
fn songs_and_sfx_match_fifo_capture() {
    let Some(rom) = testutil::rom() else { return };
    let mut bank = SoundBank::from_rom(&rom);
    apply_harness_patch(&mut bank);
    let mut checked = 0;
    let mut failures = Vec::new();
    for (kind, n) in [("song", 53usize), ("sfx", 64)] {
        for i in 0..n {
            let Some((len, bad)) = compare(&bank, kind, i) else { continue };
            checked += 1;
            if let Some(b) = bad {
                failures.push(format!("{kind} {i}: first mismatch at sample {b} of {len}"));
            }
        }
    }
    eprintln!("fifo_exact: {checked} captures compared, {} mismatching", failures.len());
    for f in &failures {
        eprintln!("  {f}");
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// replays the captures `PREFIX_NAME.json` / `.bin` of work/audio/fifo_ref on one `Driver` each (the game's own mix
/// chunks, each call before the chunk it preceded); the failures
fn replay_captures(bank: &SoundBank, prefix: &str) -> Vec<String> {
    let dir = testutil::repo_root().join("work/audio/fifo_ref");
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_prefix(prefix).and_then(|n| n.strip_suffix(".json")).map(String::from))
        .collect();
    names.sort();
    let mut failures = Vec::new();
    for name in &names {
        let j: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{prefix}{name}.json"))).unwrap()).unwrap();
        let pcm: Vec<(i8, i8)> = std::fs::read(dir.join(format!("{prefix}{name}.bin"))).unwrap().chunks_exact(2).map(|c| (c[0] as i8, c[1] as i8)).collect();
        let mut d = Driver::new(bank);
        let pans: Vec<i8> = j["tracks"].as_array().unwrap().iter().map(|t| t["pan"].as_u64().unwrap_or(0) as u8 as i8).collect();
        d.set_track_pans(&pans);
        if let Some(a) = j["track_ram"].as_array() {
            for (t, h) in a.iter().enumerate() {
                d.seed_track_ram(t, &unhex(h.as_str().unwrap()));
            }
        }
        // (the running game: the whole driver's RAM as it stood when the capture began)
        if let Some(r) = j["ram"].as_str() {
            d.seed_ram(&unhex(r));
        }
        let ops = j["ops"].as_array().unwrap();
        let mut out = Vec::new();
        for (c, n) in j["chunks"].as_array().unwrap().iter().enumerate() {
            for op in ops.iter().filter(|o| o[0].as_u64() == Some(c as u64)) {
                let i = |k: usize| op[k].as_i64().unwrap_or(0);
                match op[1].as_str().unwrap() {
                    "song" => d.play_song(i(2) as usize),
                    "sfx" => {
                        d.play_sfx(i(2) as u8, i(3) as u8, i(4) as i8);
                    }
                    "stop_sfx" => d.stop_all_sfx(),
                    "stop_one" => d.stop_sfx(i(2) as u8),
                    "stop_match" => d.stop_matching_sfx(i(2) as u8),
                    "clear" => d.clear_tracks(),
                    "stop" => d.stop_song(),
                    "volm" => d.set_music_volume(i(2) as u8),
                    "vols" => d.set_sfx_volume(i(2) as u8),
                    o => panic!("op {o}"),
                }
            }
            d.render_into(&mut out, n.as_u64().unwrap() as usize);
        }
        let bad = out.iter().zip(&pcm).position(|(a, b)| a != b);
        eprintln!("{prefix}{name}: {} samples in {} updates (largest {}), first mismatch {bad:?}", pcm.len(), j["chunks"].as_array().unwrap().len(), j["chunks"].as_array().unwrap().iter().filter_map(|n| n.as_u64()).max().unwrap_or(0));
        if let Some(b) = bad {
            failures.push(format!("{name}: sample {b} (chunk {})", {
                let mut s = 0;
                j["chunks"].as_array().unwrap().iter().take_while(|n| {
                    s += n.as_u64().unwrap() as usize;
                    s <= b
                }).count()
            }));
        }
    }
    failures
}

/// Sound transitions (re/fifo_trans.py): a song replaced mid-play, SFX over music, stop all SFX, the tracks cleared,
/// songs after songs (the driver state carried over) - replayed on one `Driver` with the game's own mix chunks and
/// each call before the chunk it preceded; every sample must equal the game's FIFO bytes.
#[test]
fn transitions_match_fifo_capture() {
    let Some(rom) = testutil::rom() else { return };
    let mut bank = SoundBank::from_rom(&rom);
    apply_harness_patch(&mut bank);
    let failures = replay_captures(&bank, "trans_");
    assert!(failures.is_empty(), "transitions not exact: {failures:?}");
}

fn fnv(b: impl Iterator<Item = u8>) -> u64 {
    b.fold(0xcbf2_9ce4_8422_2325, |h, x| (h ^ x as u64).wrapping_mul(0x100_0000_01b3))
}

/// The running game (re/fifo_live.py; not the frozen harness): the pause menu opened over music, moved in, its pages
/// (an item's Use, the Options' volumes), closed; items picked up in the field and in a link fight (the item-get
/// presentation) over the area's song. The main loop's long iterations there are refilled from the timer interrupt
/// (928 samples at a time) and end with the iteration's own update; the calls take effect at the update after them.
/// References: `data/audio/live_NAME.json` (the driver's RAM at the start, every update's size, every call, the
/// stream's FNV-1a 64 - no sound data). Replayed on a `Driver` seeded from that RAM: the same stream, every sample
/// (with `work/audio/fifo_ref/live_NAME.bin` present, the first differing sample is named).
#[test]
fn live_game_matches_fifo_capture() {
    let Some(rom) = testutil::rom() else { return };
    let bank = SoundBank::from_rom(&rom);
    let dir = testutil::repo_root().join("data/audio");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("data/audio")
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_prefix("live_").and_then(|n| n.strip_suffix(".json")).map(String::from))
        .collect();
    names.sort();
    assert!(names.len() >= 7, "the live captures' references: {names:?}");
    let mut failures = Vec::new();
    for name in &names {
        let j: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("live_{name}.json"))).unwrap()).unwrap();
        let mut d = Driver::new(&bank);
        d.seed_ram(&unhex(j["ram"].as_str().unwrap()));
        let ops = j["ops"].as_array().unwrap();
        let mut out = Vec::new();
        for (c, n) in j["chunks"].as_array().unwrap().iter().enumerate() {
            for op in ops.iter().filter(|o| o[0].as_u64() == Some(c as u64)) {
                let i = |k: usize| op[k].as_i64().unwrap_or(0);
                match op[1].as_str().unwrap() {
                    "song" => d.play_song(i(2) as usize),
                    "sfx" => {
                        d.play_sfx(i(2) as u8, i(3) as u8, i(4) as i8);
                    }
                    "stop_sfx" => d.stop_all_sfx(),
                    "stop_one" => d.stop_sfx(i(2) as u8),
                    "stop_match" => d.stop_matching_sfx(i(2) as u8),
                    "clear" => d.clear_tracks(),
                    "stop" => d.stop_song(),
                    "volm" => d.set_music_volume(i(2) as u8),
                    "vols" => d.set_sfx_volume(i(2) as u8),
                    o => panic!("op {o}"),
                }
            }
            d.render_into(&mut out, n.as_u64().unwrap() as usize);
        }
        let h = format!("{:016x}", fnv(out.iter().flat_map(|s| [s.0 as u8, s.1 as u8])));
        let ok = out.len() as u64 == j["samples"].as_u64().unwrap() && Some(h.as_str()) == j["fnv"].as_str();
        eprintln!(
            "live {name}: {} samples in {} updates (largest {}, {} from the interrupt), {} calls: {}",
            out.len(),
            j["chunks"].as_array().unwrap().len(),
            j["long"],
            j["irq"],
            ops.len(),
            if ok { "exact" } else { "DIFFERS" }
        );
        if !ok {
            let bin = testutil::repo_root().join(format!("work/audio/fifo_ref/live_{name}.bin"));
            let at = std::fs::read(bin).ok().and_then(|b| out.iter().zip(b.chunks_exact(2)).position(|(a, b)| (a.0 as u8, a.1 as u8) != (b[0], b[1])));
            failures.push(format!("{name}: first differing sample {at:?}"));
        }
    }
    assert!(failures.is_empty(), "the running game's sound not exact: {failures:?}");
}
