//! Dev tool: print {song: seconds needed to cover intro + one full loop + 2 s} as JSON (for re/fifo_ref.py).
use wf_audio::bank::SoundBank;
use wf_audio::driver::{Driver, MIX_RATE};
use wf_rom::testutil;

fn main() {
    let bank = SoundBank::from_rom(&testutil::rom().unwrap());
    let mut parts = Vec::new();
    for i in 0..bank.songs.len() {
        let (log, lp) = Driver::song_structure(&bank, i, 20000);
        let secs = match lp {
            Some((_, j)) => log[j].sample as f64 / MIX_RATE + 2.0,
            None => 60.0,
        };
        parts.push(format!("\"{i}\": {:.1}", secs.max(10.0)));
    }
    println!("{{{}}}", parts.join(", "));
}
