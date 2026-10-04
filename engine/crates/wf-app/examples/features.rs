//! The product features on top of the exact core, story mode, no lag (headless, muted): the whole real run from
//! power-on (`tools/wholerun/record.py`'s master: the app is stepped once per game logic iteration with that row's
//! keys, as `wholerun --lag none`) and, along it:
//!
//! * replays: the run is recorded through `wf_replay::Recorder` (a state-hash checkpoint every `--interval` steps),
//!   serialized, parsed back and verified with `wf_replay::verify` on a fresh power-on sim;
//! * save states: every `--save-every` steps the sim's snapshot bytes are restored into a fresh sim, which continues
//!   `--continue` steps and must hit the uninterrupted run's checkpoint hashes;
//! * rewind: every `--rewind-every` steps (at a pseudo-random offset) the sim is rewound `--rewind` steps to a kept
//!   snapshot, re-run with the same inputs and must reach the same state hash.
//!
//!   cargo run --release -p wf-app --example features -- MASTER.jsonl --power-on POWER_ON.json [--steps N]
//!       [--interval 600] [--save-every 50000] [--continue 3000] [--rewind-every 20000] [--rewind 300]

use std::io::{BufRead, BufReader};
use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::{Sim, SimEvent};

fn power_on(packs: &Packs, power: &Option<serde_json::Value>) -> StorySim {
    let data = wf_app::battle::load_data(packs).expect("data");
    let cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::State(0, 0), ..Default::default() };
    let mut session = wf_core::session::Session::new_field(data, cfg);
    session.world.rng = wf_core::rng::Rng::new(0, 0);
    session.world.opts.no_lag = true;
    wf_core::menu::title::open_title_boot(&mut session.world);
    if let Some(v) = power {
        let hex = v["driver"].as_str().unwrap_or("");
        let dram: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(0)).collect();
        let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
        if session.world.data.sound.is_some() {
            wf_core::sndsim::import(&mut session.world, &dram, n("pos"), n("last"), Some(n("phase")));
        }
    }
    let mut app = StorySim::with_session(packs, session).expect("story sim");
    app.skip_area_banner();
    app.session.world.opts.no_lag = true;
    app
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let master = args.first().expect("MASTER.jsonl").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let num = |name: &str, d: u64| opt(name).and_then(|v| v.parse::<u64>().ok()).unwrap_or(d);
    let limit = num("--steps", u64::MAX);
    let interval = num("--interval", 600) as u32;
    let save_every = num("--save-every", 50_000);
    let cont = num("--continue", 3_000);
    let rewind_every = num("--rewind-every", 20_000);
    let rewind = num("--rewind", 300);
    let power: Option<serde_json::Value> = opt("--power-on").map(|p| serde_json::from_str(&std::fs::read_to_string(p).expect("power-on")).expect("json"));

    // the app's inputs: the keys of the recording's logic rows (no lag: one step per game iteration)
    let mut inputs: Vec<u16> = vec![];
    let mut prev_lf = 0i64;
    for l in BufReader::new(std::fs::File::open(&master).expect("master")).lines() {
        let v: serde_json::Value = serde_json::from_str(&l.expect("line")).expect("row");
        let lf = v["lf"].as_i64().unwrap_or(0);
        if lf != prev_lf {
            inputs.push(v["keys"].as_u64().unwrap_or(0) as u16);
        }
        prev_lf = lf;
        if inputs.len() as u64 >= limit {
            break;
        }
    }
    println!("{} logic steps", inputs.len());

    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();

    let mut sim = power_on(&packs, &power);
    // --probe K: a restored copy taken at step K beside the original, stepped together: the first step where they
    // differ and which world parts do (diagnosis of an incomplete snapshot)
    if let Some(pk) = opt("--probe").and_then(|v| v.parse::<usize>().ok()) {
        let mut ev = vec![];
        for &keys in &inputs[..pk] {
            ev.clear();
            sim.step(&[keys], &mut ev);
        }
        let mut b = power_on(&packs, &power);
        if args.iter().any(|a| a == "--self") {
            // B: a second power-on sim stepped the same way, then restored from its own snapshot (a rewind of 0)
            for &keys in &inputs[..pk] {
                ev.clear();
                b.step(&[keys], &mut ev);
            }
            let s = b.snapshot().unwrap();
            assert!(b.restore(&s), "self restore");
        } else {
            assert!(b.restore(&sim.snapshot().unwrap()), "restore");
        }
        println!("probe at {pk}: hashes {:016x} / {:016x}", sim.state_hash(), b.state_hash());
        {
            let (x, y) = (wf_core::snapshot::to_bytes(&sim.session.world), wf_core::snapshot::to_bytes(&b.session.world));
            println!("  world bytes equal: {} ({} / {})", x == y, x.len(), y.len());
            let d: Vec<usize> = x.iter().zip(y.iter()).enumerate().filter(|(_, (p, q))| p != q).map(|(i, _)| i).take(20).collect();
            println!("  differing offsets {d:?}");
            if let Some(&i) = d.first() {
                println!("  around: {:02x?}\n      vs {:02x?}", &x[i.saturating_sub(24)..(i + 24).min(x.len())], &y[i.saturating_sub(24)..(i + 24).min(y.len())]);
            }
            // the serialized parts one by one (the snapshot's own order)
            let w2 = wf_core::snapshot::from_bytes(&sim.session.world.data, &x).expect("re-read");
            println!("  re-read of A's bytes equal to B's restore: {}", wf_core::snapshot::to_bytes(&w2) == y);
            let (wa, wb) = (&sim.session.world, &b.session.world);
            for (n, x, y) in [
                ("col", format!("{:?}", wa.col.as_ref().map(|c| (c.enemy.len(), c.walk.as_ref().map(|v| v.len())))), format!("{:?}", wb.col.as_ref().map(|c| (c.enemy.len(), c.walk.as_ref().map(|v| v.len()))))),
                ("opts", format!("{:?}", wa.opts), format!("{:?}", wb.opts)),
                ("stats", format!("{:?}", wa.stats), format!("{:?}", wb.stats)),
                ("saves", format!("{:?}", wa.saves), format!("{:?}", wb.saves)),
                ("input", format!("{:?}", wa.input), format!("{:?}", wb.input)),
                ("field", format!("{:?}", wa.field), format!("{:?}", wb.field)),
                ("ents", format!("{:?}", wa.ents), format!("{:?}", wb.ents)),
            ] {
                if x != y {
                    let i = x.bytes().zip(y.bytes()).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
                    let lo = i.saturating_sub(150);
                    println!("  {n} differs: ...{}...\n   vs ...{}...", &x[lo..(i + 150).min(x.len())], &y[lo..(i + 150).min(y.len())]);
                }
            }
        }
        for (j, &keys) in inputs[pk..].iter().enumerate().take(30000) {
            ev.clear();
            sim.step(&[keys], &mut ev);
            ev.clear();
            b.step(&[keys], &mut ev);
            if wf_core::snapshot::to_bytes(&sim.session.world) != wf_core::snapshot::to_bytes(&b.session.world) {
                let (wa, wb) = (&sim.session.world, &b.session.world);
                let parts: Vec<(&str, String, String)> = vec![
                    ("rng", format!("{:?}", wa.rng.state()), format!("{:?}", wb.rng.state())),
                    ("frame", format!("{}", wa.frame), format!("{}", wb.frame)),
                    ("ents", format!("{:?}", wa.ents), format!("{:?}", wb.ents)),
                    ("field", format!("{:?}", wa.field), format!("{:?}", wb.field)),
                    ("scene", format!("{:?}", wa.scene), format!("{:?}", wb.scene)),
                    ("story", format!("{:?}", wa.story), format!("{:?}", wb.story)),
                    ("menu", format!("{:?}", wa.menu), format!("{:?}", wb.menu)),
                    ("g", format!("{:?}", wa.g), format!("{:?}", wb.g)),
                    ("cameras", format!("{:?}", wa.cameras), format!("{:?}", wb.cameras)),
                    ("sound", format!("{:?}", wa.sound), format!("{:?}", wb.sound)),
                ];
                println!("probe: differ after {} steps (step {})", j + 1, pk + j + 1);
                for (n, x, y) in parts {
                    if x != y {
                        let i = x.bytes().zip(y.bytes()).position(|(p, q)| p != q).unwrap_or(0);
                        let lo = i.saturating_sub(200);
                        println!("  {n}: ...{}...\n   vs ...{}...", &x[lo..(i + 200).min(x.len())], &y[lo..(i + 200).min(y.len())]);
                    }
                }
                return;
            }
        }
        println!("probe: equal for 30000 steps");
        return;
    }
    let header = wf_replay::Header {
        sim: "story".into(),
        content: Default::default(),
        config: serde_json::json!({"power_on": true}),
        seed: 0,
        players: 1,
        hash_interval: interval,
        title: "whole run".into(),
        created: 0,
        branch: None,
        outcome: String::new(),
        frames: 0,
        final_hash: String::new(),
    };
    let mut rec = wf_replay::Recorder::new(header, sim.frame(), sim.state_hash(), None);
    let mut ev: Vec<SimEvent> = vec![];
    let mut saves: Vec<(u64, Vec<u8>)> = vec![];
    // rewind: a snapshot kept `rewind` steps before each rewind point
    let mut back: Option<(u64, Vec<u8>)> = None;
    let (mut rewinds, mut rewind_bad) = (0usize, 0usize);
    let mut lcg: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next_rewind = rewind_every / 2;
    let t0 = std::time::Instant::now();
    for (k, &keys) in inputs.iter().enumerate() {
        let k = k as u64;
        if k > 0 && k % save_every == 0 {
            saves.push((k, sim.snapshot().expect("snapshot")));
        }
        if k + rewind == next_rewind {
            back = Some((k, sim.snapshot().expect("snapshot")));
        }
        ev.clear();
        sim.step(&[keys], &mut ev);
        rec.record(&[keys], sim.frame(), || sim.state_hash());
        if k + 1 == next_rewind {
            if let Some((at, s)) = back.take() {
                let want = sim.state_hash();
                let now = sim.snapshot().expect("snapshot");
                assert!(sim.restore(&s), "rewind restore");
                for &kk in &inputs[at as usize..=k as usize] {
                    ev.clear();
                    sim.step(&[kk], &mut ev);
                }
                rewinds += 1;
                let got = sim.state_hash();
                if got != want {
                    rewind_bad += 1;
                    println!("REWIND {} steps at step {}: state {got:016x} vs {want:016x}", k + 1 - at, k + 1);
                    // continue the reference run from its own state
                    assert!(sim.restore(&now));
                }
            }
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            next_rewind += rewind_every / 2 + (lcg >> 33) % rewind_every;
        }
        if (k + 1) % 200_000 == 0 {
            println!("  {} steps ({:.0}s)", k + 1, t0.elapsed().as_secs_f64());
        }
    }
    let replay = rec.finish(sim.state_hash()).clone();
    println!("uninterrupted run: {} steps, final state {:016x}, {} checkpoints ({:.0}s)", sim.frame(), sim.state_hash(), replay.checkpoints.len(), t0.elapsed().as_secs_f64());
    println!("  where: area {:?}, current {}, zenie {}, world frame {}, rng {:08x?}", sim.session.world.field.as_ref().map(|f| (f.area.world, f.area.area)), sim.session.world.story.current, sim.session.world.story.zenie, sim.session.world.frame, sim.session.world.rng.state());
    println!("rewind: {rewinds} rewinds of {rewind} steps, {rewind_bad} differ");

    // save states into fresh sims, continued against the uninterrupted run's checkpoints
    let mut save_bad = 0;
    for (k, s) in &saves {
        let mut fresh = power_on(&packs, &power);
        if !fresh.restore(s) {
            println!("SAVE at step {k}: restore refused");
            save_bad += 1;
            continue;
        }
        let mut c = wf_replay::Checker::default();
        let end = (*k + cont).min(inputs.len() as u64);
        for &kk in &inputs[*k as usize..end as usize] {
            ev.clear();
            fresh.step(&[kk], &mut ev);
            if c.check(&replay, fresh.frame(), || fresh.state_hash()).is_some() {
                break;
            }
        }
        if let Some(d) = c.divergence {
            save_bad += 1;
            println!("SAVE at step {k}: {d}");
        }
    }
    println!("save states: {} restored into fresh sims ({} bytes each, last), {save_bad} differ within {cont} steps", saves.len(), saves.last().map(|s| s.1.len()).unwrap_or(0));

    // the replay file round trip and its verification on a fresh power-on sim
    let bytes = replay.to_bytes();
    let back = wf_replay::Replay::from_bytes(&bytes).expect("replay parse");
    let mut fresh = power_on(&packs, &power);
    let rep = wf_replay::verify(&back, &mut fresh);
    println!("replay: {} bytes, {} frames, {} checkpoints checked, ok {}{}", bytes.len(), rep.frames, rep.checkpoints, rep.ok, rep.divergence.map(|d| format!(" - {d}")).unwrap_or_default());
}
