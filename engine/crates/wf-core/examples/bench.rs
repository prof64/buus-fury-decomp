//! Step / snapshot / restore / hash timings for typical battles (headless, no window, no audio).
//!   cargo run --release -p wf-core --example bench [-- <content/base dir>]
//! Without a content dir the ROM (roms/buu.gba) is extracted in memory first.
use std::sync::Arc;
use std::time::Instant;
use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::{DirSource, GameData, MapSource};
use wf_core::session::Session;

fn load() -> Arc<GameData> {
    if let Some(dir) = std::env::args().nth(1) {
        return Arc::new(GameData::load(&DirSource { root: dir.into() }).expect("content"));
    }
    let rom = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../roms/buu.gba");
    let opts = wf_extract::Options {
        only: ["tables", "sprites", "effects", "maps"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let (c, _) =
        wf_extract::extract(std::fs::read(rom).expect("roms/buu.gba"), &opts).expect("extract");
    Arc::new(GameData::load(&MapSource::new(c.files)).expect("content"))
}

fn keys(f: u32) -> u16 {
    let x = (f / 9).wrapping_mul(2654435761) >> 7;
    [0u16, 0x20, 0x10, 0x40, 0x80, 1, 1, 2, 0x100, 0x21, 0x12][(x % 11) as usize]
}

fn main() {
    let t = Instant::now();
    let data = load();
    println!("content load: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    for (name, enemy, count, arena) in [
        ("4 Majin Fighters", "t68", 4, "forest"),
        ("Janemba + parts", "t51", 1, "crater"),
        ("Kid Buu", "t60", 1, "wasteland"),
        ("12 Majin Fighters", "t68", 12, "plateau"),
    ] {
        let cfg = BattleConfig {
            arena: arena.into(),
            players: vec![FighterSetup::default()],
            phases: vec![Phase {
                enemy: enemy.into(),
                count,
            }],
            rng: RngInit::Seed(9),
            xp: false,
        };
        let mut s = Session::new(Arc::clone(&data), cfg);
        let n = 3600u32;
        let t = Instant::now();
        let (mut ents, mut snap_ns, mut bytes_ns, mut restore_ns, mut hash_ns, mut size) =
            (0usize, 0u128, 0u128, 0u128, 0u128, 0usize);
        let mut step_ns = 0u128;
        for f in 0..n {
            let t0 = Instant::now();
            s.step(&[keys(f)]);
            step_ns += t0.elapsed().as_nanos();
            ents = ents.max(s.world.ents.len());
            let t1 = Instant::now();
            let sn = s.snapshot();
            snap_ns += t1.elapsed().as_nanos();
            let t2 = Instant::now();
            s.restore(&sn);
            restore_ns += t2.elapsed().as_nanos();
            let t3 = Instant::now();
            std::hint::black_box(s.hash());
            hash_ns += t3.elapsed().as_nanos();
            if f % 60 == 0 {
                let t4 = Instant::now();
                let b = s.snapshot_bytes();
                bytes_ns += t4.elapsed().as_nanos() * 60;
                size = size.max(b.len());
            }
        }
        let us = |ns: u128| ns as f64 / n as f64 / 1e3;
        println!(
            "{name:18} {n} frames in {:.0} ms | step {:.1} us | snapshot (clone) {:.1} us | restore {:.1} us | hash {:.1} us | bytes {:.1} us ({} KB) | max entities {ents}",
            t.elapsed().as_secs_f64() * 1e3,
            us(step_ns),
            us(snap_ns),
            us(restore_ns),
            us(hash_ns),
            us(bytes_ns),
            size / 1024
        );
    }
}
