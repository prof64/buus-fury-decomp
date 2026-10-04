//! Story mode through the rollback netcode (headless, muted): two `wf_net::P2PSession` peers in process on the
//! virtual network (latency, jitter, loss), each running a co-op story session (`StorySim`, two players), random
//! buttons per slot. Afterwards: every peer's state hash and hash chain at the last frame agree, no desync event,
//! and the networked result equals a straight (no-rollback) run of the confirmed inputs.
//!
//!   cargo run --release -p wf-app --example netstory -- [--frames N] [--area W-A] [--latency MS] [--jitter MS]
//!       [--loss P] [--seed S]

use wf_app::story::{StorySim, StoryState};
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_net::session::{NetConfig, NetEvent};
use wf_net::testnet;
use wf_net::transport::{Conditions, SimNet};
use wf_view::{Sim, SimEvent};

struct StoryGame(StorySim);

impl wf_net::Game for StoryGame {
    type State = StoryState;
    fn save(&self) -> StoryState {
        self.0.save_mem()
    }
    fn load(&mut self, s: &StoryState) {
        self.0.load_mem(s);
    }
    fn advance(&mut self, inputs: &[u16], _first: bool) {
        let mut ev: Vec<SimEvent> = vec![];
        self.0.step(inputs, &mut ev);
    }
    fn checksum(&self, s: &StoryState) -> u64 {
        s.session.world.state_hash()
    }
    fn save_bytes(&self) -> Option<Vec<u8>> {
        self.0.snapshot()
    }
    fn load_bytes(&mut self, b: &[u8]) -> bool {
        self.0.restore(b)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let frames: i32 = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(6000);
    let area = opt("--area").and_then(|v| wf_app::story::parse_area(&v)).unwrap_or((1, 1));
    let f = |n: &str, d: f64| opt(n).and_then(|v| v.parse().ok()).unwrap_or(d);
    let cond = Conditions { latency_ms: f("--latency", 80.0), jitter_ms: f("--jitter", 20.0), loss: f("--loss", 0.03) };
    let seed: u64 = opt("--seed").and_then(|v| v.parse().ok()).unwrap_or(7);

    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    // --master MASTER.jsonl --power-on P [--distance D]: the rollback sync test (every frame: roll back D frames,
    // re-simulate, compare the states) over the whole real run from power-on (one player, the recording's logic rows)
    if let Some(master) = opt("--master") {
        use std::io::BufRead;
        let power: Option<serde_json::Value> = opt("--power-on").map(|p| serde_json::from_str(&std::fs::read_to_string(p).expect("power-on")).expect("json"));
        let data = wf_app::battle::load_data(&packs).expect("data");
        let cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::State(0, 0), ..Default::default() };
        let mut session = wf_core::session::Session::new_field(data, cfg);
        session.world.rng = wf_core::rng::Rng::new(0, 0);
        session.world.opts.no_lag = true;
        wf_core::menu::title::open_title_boot(&mut session.world);
        if let Some(v) = &power {
            let hex = v["driver"].as_str().unwrap_or("");
            let dram: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(0)).collect();
            let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
            if session.world.data.sound.is_some() {
                wf_core::sndsim::import(&mut session.world, &dram, n("pos"), n("last"), Some(n("phase")));
            }
        }
        let mut app = StorySim::with_session(&packs, session).expect("story sim");
        app.skip_area_banner();
        let mut st = wf_net::synctest::SyncTestSession::new(StoryGame(app), opt("--distance").and_then(|v| v.parse().ok()).unwrap_or(2));
        st.bytes = args.iter().any(|a| a == "--bytes");
        let mut prev_lf = 0i64;
        let (mut n, t0) = (0u64, std::time::Instant::now());
        for l in std::io::BufReader::new(std::fs::File::open(&master).expect("master")).lines() {
            let v: serde_json::Value = serde_json::from_str(&l.expect("line")).expect("row");
            let lf = v["lf"].as_i64().unwrap_or(0);
            if lf != prev_lf {
                if let Err(e) = st.advance(&[v["keys"].as_u64().unwrap_or(0) as u16]) {
                    println!("sync test: DESYNC at step {n}: {e:?}");
                    return;
                }
                n += 1;
                if n >= frames as u64 {
                    break;
                }
            }
            prev_lf = lf;
        }
        println!("sync test (rollback {} every step{}): {n} steps, {} re-simulated, no desync ({:.0}s)", st.check_distance, if st.bytes { ", via bytes" } else { "" }, st.resim_frames, t0.elapsed().as_secs_f64());
        return;
    }
    let make = || StoryGame(StorySim::new(&packs, area, vec![0, 3], 1234).expect("story"));

    let net = SimNet::new(cond, seed);
    let cfg = NetConfig { session: 4321, max_prediction: 16, ..Default::default() };
    let mut ps = testnet::peers(&net, vec![make(), make()], &cfg, 0, seed);
    ps[1].start_tick = 5;
    let t0 = std::time::Instant::now();
    let r = testnet::run(&net, &mut ps, &mut [], frames, 3_600_000.0);
    println!("run: done {} ({:.0} virtual s, {:.0} s real)", r.done, r.ms / 1000.0, t0.elapsed().as_secs_f64());
    let h = ps[0].sess.hashes[frames as usize];
    let mut ok = r.done;
    for p in &ps {
        let same = p.sess.hashes[frames as usize] == h && p.sess.chain[frames as usize] == ps[0].sess.chain[frames as usize];
        let desyncs = p.sess.events.iter().filter(|e| matches!(e, NetEvent::Desync { .. })).count();
        println!("  {}: hash {:016x} chain {:016x} desyncs {desyncs} rollbacks {} (max {}) re-simulated {}", p.id, p.sess.hashes[frames as usize], p.sess.chain[frames as usize], p.sess.stats.rollbacks, p.sess.stats.max_rollback, p.sess.stats.resim_frames);
        ok &= same && desyncs == 0;
    }
    // the straight run with the confirmed inputs
    let mut straight = make();
    let inputs = ps[0].sess.known_inputs().to_vec();
    for k in 0..frames as usize {
        let inp: Vec<u16> = inputs.iter().map(|slot| slot.get(k).copied().unwrap_or(0)).collect();
        wf_net::Game::advance(&mut straight, &inp, true);
    }
    let sh = straight.0.state_hash();
    println!("straight replay of the confirmed inputs: {:016x} ({})", sh, if sh == h { "equal" } else { "DIFFERENT" });
    ok &= sh == h;
    println!("netcode story mode: {}", if ok { "OK" } else { "FAILED" });
}
