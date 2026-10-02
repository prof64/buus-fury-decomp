//! Dev tool: check a song's loop period on the synthesized output: correlation of [t, t+5 s] with the same
//! window one period later, for our period and an alternative.   cargo run -p wf-audio --example loopcheck -- 41 25.8175
use wf_audio::bank::SoundBank;
use wf_audio::driver::{Driver, MIX_RATE};
use wf_rom::testutil;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let id: usize = a[0].parse().unwrap();
    let alt: f64 = a.get(1).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let bank = SoundBank::from_rom(&testutil::rom().unwrap());
    let (log, lp) = Driver::song_structure(&bank, id, 20000);
    let (i, j) = lp.expect("loop");
    let (s, e) = (log[i].sample as usize, log[j].sample as usize);
    println!("loop rows {} start {:.4}s len {:.4}s", j - i, s as f64 / MIX_RATE, (e - s) as f64 / MIX_RATE);
    let mut d = Driver::new(&bank);
    d.play_song(id);
    let total = e + (e - s) + (10.0 * MIX_RATE) as usize;
    let out = d.render(total, 267);
    let x: Vec<f64> = out.iter().map(|&(l, r)| l as f64 + r as f64).collect();
    let w = (5.0 * MIX_RATE) as usize;
    let t = s + (2.0 * MIX_RATE) as usize;
    for (name, per) in [("ours", e - s), ("alt", (alt * MIX_RATE) as usize)] {
        if per == 0 {
            continue;
        }
        let (mut ab, mut aa, mut bb, mut same) = (0.0, 0.0, 0.0, 0usize);
        for k in 0..w {
            let (p, q) = (x[t + k], x[t + per + k]);
            ab += p * q;
            aa += p * p;
            bb += q * q;
            if out[t + k] == out[t + per + k] {
                same += 1;
            }
        }
        println!("{name}: period {per} samples, corr {:.5}, identical samples {:.2}%", ab / (aa * bb).sqrt(), 100.0 * same as f64 / w as f64);
    }
}
