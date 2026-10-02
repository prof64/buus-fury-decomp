//! Compare the synthesizer with the emulator rips the site uses (`work/audio/cache/<rom>_v7/{music,sfx}/NN.wav`:
//! mGBA's 32 768 Hz stereo capture of the game's mixer, leading silence trimmed, `NN.json` loop metadata).
//!
//! The rips are a resampled view of the same 8-bit FIFO stream (mGBA output = FIFO value * 256, held at
//! 32 768 Hz), and start at an arbitrary tick phase, so this measures: best alignment, waveform correlation,
//! gain, residual, and the loop length against the ripper's loop detection. Skips without ROM / rips.

use wf_audio::bank::SoundBank;
use wf_audio::driver::{Driver, MIX_RATE};
use wf_audio::wav;
use wf_rom::testutil;

const RIP_RATE: f64 = 32768.0;

fn rip_dir() -> std::path::PathBuf {
    testutil::repo_root().join("work/audio/cache/e65738e9d676_v7")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// Mono (L+R)/2 of the rip as f32.
fn load_rip(kind: &str, i: usize) -> Option<(Vec<f32>, serde_json::Value)> {
    let d = rip_dir().join(kind);
    let w = wav::read(&std::fs::read(d.join(format!("{i:02}.wav"))).ok()?)?;
    assert_eq!(w.rate, 32768);
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join(format!("{i:02}.json"))).ok()?).ok()?;
    let ch = w.channels as usize;
    Some((w.samples.chunks_exact(ch).map(|c| c.iter().map(|&s| s as f32).sum::<f32>() / ch as f32).collect(), m))
}

/// Mine, sample-and-hold resampled to 32 768 Hz, scaled like mGBA (x256), mono.
fn upsample(mine: &[(i8, i8)], n: usize) -> Vec<f32> {
    let r = MIX_RATE / RIP_RATE;
    let m = |k: usize| mine.get(k).map(|&(l, rr)| (l as f32 + rr as f32) * 128.0).unwrap_or(0.0);
    let lin = std::env::var("WF_UPS").map(|v| v == "lin").unwrap_or(false);
    (0..n)
        .map(|j| {
            let x = j as f64 * r;
            let k = x as usize;
            if lin {
                let f = (x - k as f64) as f32;
                m(k) * (1.0 - f) + m(k + 1) * f
            } else {
                m(k)
            }
        })
        .collect()
}

/// mGBA mixes the FIFO through blip_buf (band-limited steps + a DC-blocking high-pass), so compare both signals
/// through the same band filter: box(5) low-pass (~6.5 kHz) minus a box(1024) (~32 Hz) high-pass.
fn band(x: &[f32]) -> Vec<f32> {
    let mut pre = vec![0f64; x.len() + 1];
    for i in 0..x.len() {
        pre[i + 1] = pre[i] + x[i] as f64;
    }
    let boxm = |i: usize, h: usize| -> f64 {
        let a = i.saturating_sub(h);
        let b = (i + h + 1).min(x.len());
        (pre[b] - pre[a]) / (b - a) as f64
    };
    (0..x.len()).map(|i| (boxm(i, 2) - boxm(i, 512)) as f32).collect()
}

fn corr_band(a: &[f32], b: &[f32]) -> (f64, f64, f64) {
    corr(&band(a), &band(b))
}

fn corr(a: &[f32], b: &[f32]) -> (f64, f64, f64) {
    let n = a.len().min(b.len());
    let (mut sab, mut saa, mut sbb) = (0f64, 0f64, 0f64);
    for i in 0..n {
        sab += a[i] as f64 * b[i] as f64;
        saa += a[i] as f64 * a[i] as f64;
        sbb += b[i] as f64 * b[i] as f64;
    }
    let c = sab / (saa.sqrt() * sbb.sqrt() + 1e-9);
    let gain = sab / (saa + 1e-9); // b ≈ gain * a
    let mut res = 0f64;
    for i in 0..n {
        let d = b[i] as f64 - gain * a[i] as f64;
        res += d * d;
    }
    (c, gain, (res / (sbb + 1e-9)).sqrt())
}

/// Best lag L (rip[j] ≈ mine[j + L]) in [-max, max]: coarse on 16x box-decimated signals, then exact.
fn align(mine: &[f32], rip: &[f32], start: usize, len: usize, max: i64) -> i64 {
    if max <= 128 {
        // short searches: exhaustive on the band-filtered signals
        let (bm, br) = (band(mine), band(rip));
        let mut best = (f64::MIN, 0i64);
        for lag in -max..=max {
            let a0 = start as i64 + lag;
            if a0 < 0 || a0 as usize + len > bm.len() || start + len > br.len() {
                continue;
            }
            let (c, _, _) = corr(&bm[a0 as usize..a0 as usize + len], &br[start..start + len]);
            if c > best.0 {
                best = (c, lag);
            }
        }
        return best.1;
    }
    let dec = |x: &[f32]| -> Vec<f32> { x.chunks(16).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect() };
    let (dm, dr) = (dec(mine), dec(rip));
    let (s, l) = (start / 16, len / 16);
    let mut best = (f64::MIN, 0i64);
    for lag in -(max / 16)..=(max / 16) {
        let a0 = s as i64 + lag;
        if a0 < 0 || a0 as usize + l > dm.len() || s + l > dr.len() {
            continue;
        }
        let (c, _, _) = corr(&dm[a0 as usize..a0 as usize + l], &dr[s..s + l]);
        if c > best.0 {
            best = (c, lag * 16);
        }
    }
    let mut fine = (f64::MIN, best.1);
    for lag in best.1 - 40..=best.1 + 40 {
        let a0 = start as i64 + lag;
        if a0 < 0 || a0 as usize + len > mine.len() || start + len > rip.len() {
            continue;
        }
        let (c, _, _) = corr(&mine[a0 as usize..a0 as usize + len], &rip[start..start + len]);
        if c > fine.0 {
            fine = (c, lag);
        }
    }
    fine.1
}

fn seed(d: &mut Driver, i: usize) {
    // track state the game had when the ripper started the song (same play order as fifo_ref.py); optional
    let p = testutil::repo_root().join(format!("work/audio/fifo_ref/song_{i:02}.json"));
    if let Ok(b) = std::fs::read(p) {
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        if let Some(a) = j["track_ram"].as_array() {
            for (t, h) in a.iter().enumerate() {
                d.seed_track_ram(t, &unhex(h.as_str().unwrap()));
            }
        }
    }
}

#[test]
fn music_matches_rips() {
    let Some(rom) = testutil::rom() else { return };
    let bank = SoundBank::from_rom(&rom);
    let mut n = 0;
    let mut worst = 1.0f64;
    let mut loop_err_max = 0f64;
    let mut lines = Vec::new();
    for i in 0..bank.songs.len() {
        let Some((rip, meta)) = load_rip("music", i) else { continue };
        let secs = 24.0f64.min(rip.len() as f64 / RIP_RATE + 2.0);
        let mut d = Driver::new(&bank);
        seed(&mut d, i);
        d.play_song(i);
        let mine8 = d.render((secs * MIX_RATE) as usize, 267);
        let mine = upsample(&mine8, (secs * RIP_RATE) as usize);
        // window: 2 s .. 14 s of the rip (or what exists)
        let start = (2.0 * RIP_RATE) as usize;
        let len = ((rip.len().saturating_sub(start)).min((12.0 * RIP_RATE) as usize)).min(mine.len() / 2);
        if len < 4096 {
            continue;
        }
        let lag = align(&mine, &rip, start, len, (1.0 * RIP_RATE) as i64);
        let a0 = (start as i64 + lag) as usize;
        let (c, g, res) = corr_band(&mine[a0..a0 + len], &rip[start..start + len]);
        // loop length vs the ripper
        let (log, lp) = Driver::song_structure(&bank, i, 20000);
        let mut loop_txt = String::from("-");
        if let (Some((a, b)), Some(l)) = (lp, meta.get("loop").filter(|l| !l.is_null())) {
            let mine_len = (log[b].sample - log[a].sample) as f64 / MIX_RATE;
            let rip_len = l["end"].as_f64().unwrap() - l["start"].as_f64().unwrap();
            // the ripper's loop is the shortest period it found; ours may be a multiple when e.g. a speed change
            // makes the state repeat later — compare modulo
            let k = (mine_len / rip_len).round().max(1.0);
            let err = (mine_len - k * rip_len).abs();
            // the ripper refines its loop by waveform correlation; when its own seam match is poor (song 41: 0.77) its
            // length is off (examples/loopcheck.rs shows ours repeats, its does not) -- report, do not count
            if l["correlation"].as_f64().unwrap_or(1.0) >= 0.9 { loop_err_max = loop_err_max.max(err); }
            loop_txt = format!("{mine_len:.4}s vs {rip_len:.4}s (x{k}) err {:.2} ms", err * 1000.0);
        }
        worst = worst.min(c);
        n += 1;
        lines.push(format!("song {i:02}: corr {c:.5} gain {g:.3} resid {res:.4} lag {lag} | loop {loop_txt}"));
    }
    for l in &lines {
        eprintln!("{l}");
    }
    eprintln!("music vs rips: {n} songs, worst correlation {worst:.5}, worst loop length error {:.3} ms", loop_err_max * 1000.0);
    if n > 0 {
        assert!(worst > 0.85, "worst correlation {worst}");
        assert!(loop_err_max < 0.0001, "loop length error {loop_err_max}");
    }
}

#[test]
fn sfx_match_rips() {
    let Some(rom) = testutil::rom() else { return };
    let bank = SoundBank::from_rom(&rom);
    let mut n = 0;
    let mut worst = 1.0f64;
    let mut lines = Vec::new();
    for i in 0..bank.sfx.len() {
        let Some((rip, _meta)) = load_rip("sfx", i) else { continue };
        if rip.len() < 512 {
            continue;
        }
        let secs = rip.len() as f64 / RIP_RATE + 0.5;
        let mut d = Driver::new(&bank);
        d.set_master_volumes(4, 6);
        d.play_sfx(i as u8, 64, 0);
        let mine8 = d.render((secs * MIX_RATE) as usize, 267);
        let mine = upsample(&mine8, (secs * RIP_RATE) as usize);
        let len = rip.len().min(mine.len()).min((3.0 * RIP_RATE) as usize) - 300;
        let lag = align(&mine, &rip, 0, len, 100);
        let a0 = lag.max(0) as usize;
        let (c, g, res) = corr_band(&mine[a0..a0 + len], &rip[0..len]);
        worst = worst.min(c);
        n += 1;
        lines.push(format!("sfx {i:02}: corr {c:.5} gain {g:.3} resid {res:.4} lag {lag}"));
    }
    for l in &lines {
        eprintln!("{l}");
    }
    eprintln!("sfx vs rips: {n} effects, worst correlation {worst:.5}");
    if n > 0 {
        assert!(worst > 0.93, "worst correlation {worst}");
    }
}
