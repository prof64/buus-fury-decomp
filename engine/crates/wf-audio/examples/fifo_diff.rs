//! Dev tool: show where the driver output departs from a FIFO capture (work/audio/fifo_ref).
//!     cargo run -p wf-audio --example fifo_diff -- song 4
use wf_audio::bank::SoundBank;
use wf_audio::driver::Driver;
use wf_rom::testutil;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let kind = a[0].as_str();
    let i: usize = a[1].parse().unwrap();
    let rom = testutil::rom().expect("rom");
    let bank = SoundBank::from_rom(&rom);
    let dir = testutil::repo_root().join("work/audio/fifo_ref");
    let b = std::fs::read(dir.join(format!("{kind}_{i:02}.bin"))).unwrap();
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{kind}_{i:02}.json"))).unwrap()).unwrap();
    let pans: Vec<i8> = j["tracks"].as_array().unwrap().iter().map(|t| t["pan"].as_u64().unwrap() as u8 as i8).collect();
    let r: Vec<(i8, i8)> = b.chunks_exact(2).map(|c| (c[0] as i8, c[1] as i8)).collect();
    let mut d = Driver::new(&bank);
    d.log_rows = true;
    if kind == "song" {
        d.set_track_pans(&pans);
        d.play_song(i);
    } else {
        d.play_sfx(i as u8, 0x40, 0);
    }
    let out = d.render(r.len(), 267);
    let bad = out.iter().zip(&r).position(|(x, y)| x != y);
    println!("len {} first mismatch {:?}", r.len(), bad);
    if let Some(k) = bad {
        let s = k.saturating_sub(8);
        for t in s..(k + 24).min(r.len()) {
            println!("{t:8} mine {:4} {:4}  ref {:4} {:4}", out[t].0, out[t].1, r[t].0, r[t].1);
        }
        if let Some(e) = d.rows_log.iter().rev().find(|e| e.sample as usize <= k) {
            println!("row at mismatch: {e:?}");
            if kind == "song" {
                let s = &bank.songs[i];
                let p = &s.patterns[s.orders[e.order as usize] as usize];
                for r in (e.row as usize).saturating_sub(3)..=(e.row as usize) {
                    println!("  row {r}: {:?}", p.rows[r]);
                }
            }
        }
        if kind == "song" {
            for t in 0..15 {
                let mut d3 = Driver::new(&bank);
                d3.mute = !(1u16 << t);
                d3.set_track_pans(&pans);
                d3.play_song(i);
                let o = d3.render(k + 24, 267);
                if o[k - 8..k + 24].iter().any(|&(l, r)| l != 0 || r != 0) {
                    let s: Vec<String> = o[k - 2..k + 20].iter().map(|x| format!("{},{}", x.0, x.1)).collect();
                    println!("solo {t}: {}", s.join(" "));
                }
            }
            let mut d2 = Driver::new(&bank);
            d2.set_track_pans(&pans);
            d2.play_song(i);
            d2.render(k.saturating_sub(16), 267);
            for (ti, t) in d2.player.tracks.iter().enumerate() {
                let v = &t.v;
                if v.flags & 1 != 0 {
                    println!(
                        "track {ti}: entry {:?} pos {} ({}) step {} loop {} ls {} end {} gain {:08x} pitch {:x} rate {}",
                        v.entry, v.pos, v.pos >> 14, v.step, v.loop_mode, v.loop_start >> 14, v.end >> 14, v.gain, v.pitch, v.rate
                    );
                }
            }
        }
    }
    if kind == "sfx" {
        let e = &bank.sfx[i];
        println!("entry {:?} len {}", e, bank.samples[e.sample].pcm.len());
    }
}
