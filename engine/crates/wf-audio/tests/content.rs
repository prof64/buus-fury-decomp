//! Content export: files exist, parse, are consistent with the bank, and the decode raised no warnings.

use wf_audio::bank::SoundBank;
use wf_rom::{content::Content, testutil};

#[test]
fn export_layout() {
    let Some(rom) = testutil::rom() else { return };
    let bank = SoundBank::from_rom(&rom);
    assert!(bank.warnings.is_empty(), "{:?}", bank.warnings);
    assert_eq!(bank.songs.len(), 53);
    assert_eq!(bank.sfx.len(), 64);
    assert_eq!(bank.instruments.len(), 79);

    let t0 = std::time::Instant::now();
    let mut c = Content::new();
    wf_audio::extract(&rom, &mut c).unwrap();
    eprintln!("audio extract: {} files, {:.1} MB, {:.0} ms", c.files.len(), c.total_bytes() as f64 / 1e6, t0.elapsed().as_secs_f64() * 1000.0);

    let idx = c.json("audio/index.json").unwrap();
    assert_eq!(idx["songs"].as_array().unwrap().len(), 53);
    assert_eq!(idx["sfx"].as_array().unwrap().len(), 64);
    for s in idx["songs"].as_array().unwrap() {
        let song = c.json(s["file"].as_str().unwrap()).unwrap();
        assert!(song["patterns"].as_array().unwrap().len() > 0);
        assert!(song["orders"].as_array().unwrap().len() > 0);
    }
    assert_eq!(idx["songs"][1]["name"], "Babidi's Ship");
    for s in idx["samples"].as_array().unwrap() {
        let wav = c.get(&format!("audio/samples/{}.wav", s["id"].as_str().unwrap())).unwrap();
        let w = wf_audio::wav::read(wav).unwrap();
        assert_eq!(w.samples.len(), s["length"].as_u64().unwrap() as usize);
    }
    // WAV round trip of the PCM
    let w = wf_audio::wav::read(c.get("audio/samples/sample_000.wav").unwrap()).unwrap();
    let back: Vec<i8> = w.samples.iter().map(|&s| (s / 256) as i8).collect();
    assert_eq!(back, bank.samples[0].pcm);
    let drv = c.json("audio/driver.json").unwrap();
    assert_eq!(drv["pitch_table"][60], 0x4000);
}

#[test]
fn rendered_wavs_opt_in() {
    let Some(rom) = testutil::rom() else { return };
    let mut c = Content::new();
    wf_audio::extract_rendered(&rom, &mut c, 1.0).unwrap();
    let w = wf_audio::wav::read(c.get("audio/rendered/song_01.wav").unwrap()).unwrap();
    assert_eq!((w.channels, w.rate), (2, 15978));
    assert!(c.get("audio/rendered/sfx_63.wav").is_some());
}

#[test]
fn hq_renderer_follows_the_exact_driver() {
    let Some(rom) = testutil::rom() else { return };
    let bank = SoundBank::from_rom(&rom);
    // render song 1 both ways at the native rate and compare envelopes (hq has no 8-bit truncation / clipping)
    let secs = 8.0;
    let exact = wf_audio::render_song(&bank, 1, secs);
    let rate = wf_audio::driver::MIX_RATE.round() as u32;
    for (linear, min) in [(false, 0.98), (true, 0.9)] {
    let hq = wf_audio::hq::render_song(&bank, 1, secs, rate, linear);
    let n = exact.len().min(hq.len() / 2);
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for i in 0..n {
        let k = (i as f64 * wf_audio::driver::MIX_RATE / rate as f64) as usize;
        let Some(&(l, _)) = exact.get(k) else { break };
        let a = l as f64 / 128.0;
        let b = hq[2 * i] as f64;
        ab += a * b;
        aa += a * a;
        bb += b * b;
    }
    let c = ab / (aa * bb).sqrt();
    eprintln!("hq (linear {linear}) vs exact (song 1, {secs} s, left): correlation {c:.4}");
    assert!(c > min, "{c}");
    }
}
