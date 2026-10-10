//! Dev tool: replay a driver log from the emulator (work/earth/timing/sndlog.py) through the port, starting from
//! the logged driver RAM, and report the first update after which the voices differ from the game's RAM.
//!     cargo run --release -p wf-audio --example sndcheck -- /tmp/snd.json
use wf_audio::bank::SoundBank;
use wf_audio::driver::{Driver, N_TRACKS, N_VOICES};
use wf_rom::testutil;

const SFX_TABLE: u64 = 0x0831_92A8;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// (pos, step, gain, flags) of every voice: 15 tracks then 16 SFX voices
fn voices_ram(ram: &[u8]) -> Vec<(i32, i32, u32, u8)> {
    let u32_ = |o: usize| u32::from_le_bytes([ram[o], ram[o + 1], ram[o + 2], ram[o + 3]]);
    let mut v = Vec::new();
    for t in 0..N_TRACKS {
        let o = 0x18 + 0x60 * t;
        v.push((u32_(o) as i32, u32_(o + 4) as i32, u32_(o + 0xC), ram[o + 0x10]));
    }
    for i in 0..N_VOICES {
        let o = 0x18 + 0x60 * N_TRACKS + 0x40 * i;
        v.push((u32_(o) as i32, u32_(o + 4) as i32, u32_(o + 0xC), ram[o + 0x10]));
    }
    v
}

fn main() {
    let path = std::env::args().nth(1).expect("log");
    let rom = testutil::rom().expect("rom");
    let bank = SoundBank::from_rom(&rom);
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut d = Driver::new(&bank);
    d.seed_ram(&unhex(j["ram0"].as_str().unwrap()));
    let mut out = Vec::new();
    let mut updates = 0;
    let mut last_n = 0;
    let mut prof = wf_audio::driver::Profile::default();
    let mut mode = 0;
    let dump = std::env::args().nth(2);
    let mut rows = Vec::new();
    for e in j["events"].as_array().unwrap() {
        let k = e[0].as_str().unwrap();
        match k {
            "upd" => {
                last_n = e[4].as_u64().unwrap() as usize;
                // chunks of <= 512 like 0x0803F296
                // fewer than 64 pending samples: the update returns without mixing
                let mut left = if last_n < 64 { 0 } else { last_n };
                wf_audio::driver::profile_begin();
                while left > 0 {
                    let c = left.min(512);
                    d.render_into(&mut out, c);
                    left -= c;
                }
                updates += 1;
                prof = wf_audio::driver::profile_end();
                mode = e[5].as_u64().unwrap();
            }
            "ram" => {
                if let Some(c) = e.get(3).and_then(|c| c.as_u64()) {
                    let p = &prof;
                    rows.push(format!(
                        "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                        e[1], last_n, mode, c, p.inner, p.runs, p.silent_runs, p.track_mix, p.sfx_mix, p.splits, p.upd_vol, p.upd_entry, p.upd_restart, p.upd_pitch, p.upd_live, p.ticks, p.rows, p.cells, p.tick_fns, p.chunks, p.segments
                    ));
                }
            }
            "sfx" => {
                let id = ((e[3].as_u64().unwrap() - SFX_TABLE) / 12) as u8;
                d.play_sfx(id, e[4].as_u64().unwrap() as u8, e[5].as_u64().unwrap() as u8 as i8);
            }
            "stop" => d.stop_sfx(((e[3].as_u64().unwrap() - SFX_TABLE) / 12) as u8),
            "stopall" => d.stop_all_sfx(),
            "songstop" => d.stop_song(),
            "song" => {
                let p = e[3].as_u64().unwrap() as u32;
                if let Some(i) = bank.songs.iter().position(|s| s.rom == p) {
                    d.play_song(i);
                } else {
                    println!("song ptr {p:#x} unknown");
                }
            }
            "pre" => {
                let g = voices_ram(&unhex(e[2].as_str().unwrap()));
                let st = d.state();
                let ours: Vec<(i32, i32, u32, u8)> = st.player.tracks.iter().map(|t| &t.v).chain(st.voices.iter()).map(|v| (v.pos, v.step, v.gain, v.flags)).collect();
                if let Some(i) = (0..g.len()).find(|&i| ours[i] != g[i]) {
                    println!("update {updates} (frame {}, n {last_n}): voice {i} ours {:?} game {:?}", e[1], ours[i], g[i]);
                    return;
                }
            }
            _ => {}
        }
    }
    println!("{updates} updates match");
    if let Some(d) = dump {
        std::fs::write(d, rows.join("\n")).unwrap();
    }
}
