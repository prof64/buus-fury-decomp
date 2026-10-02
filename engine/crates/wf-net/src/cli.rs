//! Native shell integration: the `--net-*` options of `buus-fury`, the blocking lobby before the window opens, and
//! the headless netplay runner (scripted / random inputs, artificial latency / loss, JSON result) used by the
//! two-process tests.

use std::sync::Arc;

use wf_core::battle::BattleConfig;
use wf_core::content::GameData;

use crate::core_game::MatchConfig;
use crate::lobby::{room_code, LobbySettings, Role};
use crate::transport::{Conditions, Rng64};

pub const HELP: &str = "\
online: --net-host [--room CODE] [--net-players N] [--pvp [--pvp-enemies]] | --net-join CODE | --net-spectate CODE
        --signal ws://127.0.0.1:3536  --net-delay 2  --net-prediction 12  --net-name NAME  --net-stun URL
        --net-latency MS --net-jitter MS --net-loss 0.05 --net-seed N (artificial network, added on send)
        --net-report DIR (desync reports + .wfr)  --net-record DIR (match replays)  --net-random-input SEED  --net-result FILE (headless JSON)
local:  --net-synctest N [--net-synctest-bytes] (roll back N frames every frame and compare hashes)";

#[derive(Clone, Debug, Default, PartialEq)]
pub enum NetMode {
    #[default]
    Off,
    Host,
    Join(String),
    Spectate(String),
    SyncTest(u32),
}

#[derive(Clone, Debug)]
pub struct NetArgs {
    pub mode: NetMode,
    pub signal: String,
    pub room: Option<String>,
    pub players: usize,
    pub pvp: bool,
    pub pvp_enemies: bool,
    pub delay: u32,
    pub prediction: u32,
    pub cond: Conditions,
    pub seed: u64,
    pub report_dir: Option<String>,
    pub random_input: Option<u64>,
    pub name: String,
    pub stun: Vec<String>,
    pub result: Option<String>,
    pub synctest_bytes: bool,
    /// save every match as a .wfr replay into this directory (the native shell defaults it to `<data>/replays`)
    pub record_dir: Option<String>,
    /// give up waiting in the lobby after this many seconds
    pub lobby_timeout_s: f64,
}

impl Default for NetArgs {
    fn default() -> Self {
        NetArgs {
            mode: NetMode::Off,
            signal: "ws://127.0.0.1:3536".into(),
            room: None,
            players: 2,
            pvp: false,
            pvp_enemies: false,
            delay: 2,
            prediction: 12,
            cond: Conditions::default(),
            seed: 1,
            report_dir: None,
            random_input: None,
            name: String::new(),
            stun: Vec::new(),
            result: None,
            synctest_bytes: false,
            record_dir: None,
            lobby_timeout_s: 120.0,
        }
    }
}

impl NetArgs {
    pub fn active(&self) -> bool {
        self.mode != NetMode::Off
    }
    /// Consume one option (`next` yields its value). False = not a net option.
    pub fn flag(&mut self, f: &str, next: &mut dyn FnMut() -> String) -> bool {
        match f {
            "--net-host" => self.mode = NetMode::Host,
            "--net-join" => self.mode = NetMode::Join(next().to_ascii_uppercase()),
            "--net-spectate" => self.mode = NetMode::Spectate(next().to_ascii_uppercase()),
            "--net-synctest" => self.mode = NetMode::SyncTest(next().parse().unwrap_or(8)),
            "--net-synctest-bytes" => self.synctest_bytes = true,
            "--room" => self.room = Some(next().to_ascii_uppercase()),
            "--signal" => self.signal = next(),
            "--net-players" => self.players = next().parse().unwrap_or(2).clamp(1, 4),
            "--pvp" => self.pvp = true,
            "--pvp-enemies" => self.pvp_enemies = true,
            "--net-delay" => self.delay = next().parse().unwrap_or(2).min(10),
            "--net-prediction" => self.prediction = next().parse().unwrap_or(12).clamp(1, 60),
            "--net-latency" => self.cond.latency_ms = next().parse().unwrap_or(0.0),
            "--net-jitter" => self.cond.jitter_ms = next().parse().unwrap_or(0.0),
            "--net-loss" => self.cond.loss = next().parse().unwrap_or(0.0),
            "--net-seed" => self.seed = next().parse().unwrap_or(1),
            "--net-report" => self.report_dir = Some(next()),
            "--net-random-input" => self.random_input = Some(next().parse().unwrap_or(1)),
            "--net-name" => self.name = next(),
            "--net-stun" => self.stun.push(next()),
            "--net-result" => self.result = Some(next()),
            "--net-record" => self.record_dir = Some(next()),
            "--net-lobby-timeout" => self.lobby_timeout_s = next().parse().unwrap_or(120.0),
            _ => return false,
        }
        true
    }

    /// The match the host proposes (battle from the shell's options; its players are replaced by the seated ones).
    pub fn match_config(&self, battle: &BattleConfig) -> MatchConfig {
        if self.pvp {
            MatchConfig::pvp(battle.clone(), self.pvp_enemies)
        } else {
            MatchConfig::coop(battle.clone())
        }
    }

    pub fn lobby_settings(&self, battle: &BattleConfig, content: String) -> LobbySettings {
        let role = match self.mode {
            NetMode::Host => Role::Host,
            NetMode::Spectate(_) => Role::Spectator,
            _ => Role::Player,
        };
        let fighter = battle.players.first().cloned().unwrap_or_default();
        // (no process id on wasm32-unknown-unknown: it panics there)
        let tag = if cfg!(target_arch = "wasm32") { "web".to_string() } else { (std::process::id() % 1000).to_string() };
        let name = if self.name.is_empty() { format!("{}-{tag}", fighter.fighter) } else { self.name.clone() };
        LobbySettings { role, name, fighter, content, want_players: self.players, base: self.match_config(battle), input_delay: self.delay, max_prediction: self.prediction }
    }

    pub fn room_code(&self, seed: u64) -> String {
        match &self.mode {
            NetMode::Join(c) | NetMode::Spectate(c) => c.clone(),
            _ => self.room.clone().unwrap_or_else(|| room_code(seed)),
        }
    }

    /// ICE servers: as given; none for a signaling server on this machine (host candidates suffice), else a
    /// public STUN server so peers behind NAT find each other.
    pub fn ice(&self) -> Vec<String> {
        if !self.stun.is_empty() {
            return self.stun.iter().filter(|s| *s != "none").cloned().collect();
        }
        let local = ["ws://127.0.0.1", "ws://localhost", "ws://[::1]"].iter().any(|p| self.signal.starts_with(p));
        if local {
            Vec::new()
        } else {
            vec!["stun:stun.l.google.com:19302".into()]
        }
    }
}

/// The shell's input script: `R60,D40,RA10` (letters R L U D A B S(tart) E(select) + frames).
pub fn parse_script(s: &str) -> Vec<u16> {
    use wf_core::input as b;
    let mut out = Vec::new();
    for item in s.split(',').filter(|s| !s.is_empty()) {
        let k = item.trim_end_matches(|c: char| c.is_ascii_digit());
        let n: usize = item[k.len()..].parse().unwrap_or(1);
        let bits = k.chars().fold(0, |acc, c| {
            acc | match c {
                'R' => b::RIGHT,
                'L' => b::LEFT,
                'U' => b::UP,
                'D' => b::DOWN,
                'A' => b::A,
                'B' => b::B,
                'S' => b::START,
                'E' => b::SELECT,
                _ => 0,
            }
        });
        out.extend(std::iter::repeat_n(bits, n));
    }
    out
}

/// Deterministic button mashing for tests: random held buttons for random spans (directions, A, B, L, R).
pub struct RandomInput {
    rng: Rng64,
    cur: u16,
    left: u32,
}
impl RandomInput {
    pub fn new(seed: u64) -> Self {
        RandomInput { rng: Rng64::new(seed), cur: 0, left: 0 }
    }
    pub fn next(&mut self) -> u16 {
        use wf_core::input as b;
        if self.left == 0 {
            let r = self.rng.next();
            let dirs = [0, b::RIGHT, b::LEFT, b::UP, b::DOWN, b::RIGHT | b::UP, b::LEFT | b::DOWN, b::RIGHT, b::LEFT];
            let mut v = dirs[(r % dirs.len() as u64) as usize];
            let k = (r >> 8) % 10;
            v |= match k {
                0..=3 => b::A,
                4 => b::B,
                5 => b::R,
                6 => b::L | b::A,
                _ => 0,
            };
            self.cur = v;
            self.left = 4 + ((r >> 20) % 40) as u32;
        }
        self.left -= 1;
        self.cur
    }
}

/// Local sync-test `Sim` from the shell's battle (all slots read from the local inputs).
pub fn synctest_sim(args: &NetArgs, data: Arc<GameData>, battle: &BattleConfig, distance: u32) -> crate::netsim::SyncSim {
    crate::netsim::SyncSim::new(&args.match_config(battle), data, distance, args.synctest_bytes)
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::netsim::{content_id, Clock, NetLobby, NetSim};
    use crate::transport::Transport;
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use wf_view::Sim;

    pub fn clock() -> Clock {
        let t0 = Instant::now();
        Rc::new(move || t0.elapsed().as_secs_f64() * 1000.0)
    }

    /// The WebRTC transport to the room (+ the artificial network conditions, if any).
    pub fn transport(args: &NetArgs, url: &str) -> Result<Box<dyn Transport>, String> {
        #[cfg(feature = "webrtc")]
        {
            let t = crate::matchbox::MatchboxTransport::connect(url, args.ice())?;
            if args.cond != Conditions::default() {
                return Ok(Box::new(crate::transport::Conditioner::new(t, args.cond, args.seed)));
            }
            Ok(Box::new(t))
        }
        #[cfg(not(feature = "webrtc"))]
        {
            let _ = (args, url);
            Err("this build has no WebRTC transport (wf-net feature `webrtc`)".into())
        }
    }

    /// Join / host a room and wait (blocking, progress on stderr) until the match starts.
    pub fn connect_blocking(args: &NetArgs, data: Arc<GameData>, battle: &BattleConfig) -> Result<NetSim, String> {
        let clock = clock();
        let code = args.room_code(Instant::now().elapsed().as_nanos() as u64 ^ std::process::id() as u64 ^ (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7)));
        let url = crate::lobby::room_url(&args.signal, &code);
        let settings = args.lobby_settings(battle, content_id(&data));
        let role = settings.role;
        eprintln!("wf-net: room {code} ({url}) as {role:?}; waiting for {} player(s)", args.players);
        let t = transport(args, &url)?;
        let mut lobby = NetLobby::new(t, settings, &code, clock.clone());
        let t0 = Instant::now();
        let mut shown = 0;
        loop {
            if lobby.poll() {
                break;
            }
            let n = lobby.lobby.players_present();
            if n != shown {
                shown = n;
                eprintln!("wf-net: {n} player(s) in room {code}");
            }
            if t0.elapsed().as_secs_f64() > args.lobby_timeout_s {
                return Err(format!("nobody joined room {code} within {} s", args.lobby_timeout_s));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(r) = &lobby.lobby.rejected {
            return Err(format!("the host refused: {r}"));
        }
        let mut sim = lobby.into_sim(data)?;
        sim.report_dir = args.report_dir.as_ref().map(std::path::PathBuf::from);
        sim.record_dir = args.record_dir.as_ref().map(std::path::PathBuf::from);
        sim.created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let s = sim.status.borrow().clone();
        eprintln!("wf-net: match started as {} ({} players{}), session {:08x}", s.local_slot.map(|k| format!("P{}", k + 1)).unwrap_or("spectator".into()), sim.start.players.len(), if sim.start.config.pvp { ", PvP" } else { "" }, sim.start.session);
        Ok(sim)
    }


    /// Headless netplay (no window, no audio): play `frames` frames with the `script` (or random) inputs at the
    /// game's real rate, wait until state `frames` is confirmed by every peer, linger so the others finish too, and
    /// return / print a JSON result (state hash + chain at `frames`, rollback / network stats, desyncs).
    pub fn run_headless(args: &NetArgs, data: Arc<GameData>, battle: &BattleConfig, frames: u64, script: &[u16], content: wf_replay::ContentId) -> Result<serde_json::Value, String> {
        let frames = frames as i32;
        let mut rnd = args.random_input.map(RandomInput::new);
        let input = |f: i32, rnd: &mut Option<RandomInput>| -> u16 {
            match rnd {
                Some(r) => r.next(),
                None => script.get(f as usize).copied().unwrap_or(0),
            }
        };
        if let NetMode::SyncTest(d) = args.mode {
            let mut sim = synctest_sim(args, data, battle, d);
            let t0 = Instant::now();
            let mut ev = Vec::new();
            for f in 0..frames {
                let v = input(f, &mut rnd);
                sim.step(&[v], &mut ev);
                ev.clear();
            }
            let r = serde_json::json!({
                "mode": "synctest", "frames": frames, "check_distance": d, "bytes": args.synctest_bytes,
                "error": sim.error, "resim_frames": sim.st.resim_frames, "ms": t0.elapsed().as_secs_f64() * 1e3,
                "hash": format!("{:016x}", sim.state_hash()),
            });
            return finish(args, r);
        }
        let mut sim = connect_blocking(args, data, battle)?;
        sim.content = content;
        let period = Duration::from_secs_f64(1.0 / wf_view::GBA_FPS);
        let mut next = Instant::now();
        let t0 = Instant::now();
        let mut done_at: Option<Instant> = None;
        let mut ev = Vec::new();
        let mut ticks = 0u64;
        loop {
            let f = sim.status.borrow().frame;
            let v = input(f, &mut rnd);
            sim.step(&[v], &mut ev);
            ev.clear();
            ticks += 1;
            if done_at.is_none() && sim.hashed() >= frames {
                done_at = Some(Instant::now());
            }
            if done_at.is_some_and(|d| d.elapsed() > Duration::from_millis(2500)) {
                break;
            }
            if t0.elapsed().as_secs_f64() > frames as f64 / 30.0 + 60.0 {
                eprintln!("wf-net: giving up (state {frames} never confirmed)");
                break;
            }
            next += period;
            match next.checked_duration_since(Instant::now()) {
                Some(d) => std::thread::sleep(d),
                None => next = Instant::now(),
            }
        }
        let replay = sim.save_replay().map(|p| p.display().to_string());
        sim.bye();
        let s = sim.status.borrow().clone();
        let (h, c) = sim.hash_at(frames).unwrap_or((0, 0));
        let st = &s.stats;
        let r = serde_json::json!({
            "mode": format!("{:?}", args.mode),
            "me": s.me, "slot": s.local_slot, "players": sim.start.players.len(), "pvp": sim.start.config.pvp,
            "frames": frames, "hash": format!("{h:016x}"), "chain": format!("{c:016x}"),
            "confirmed_ok": sim.hashed() >= frames,
            "frame_now": s.frame, "ticks": ticks, "seconds": t0.elapsed().as_secs_f64(),
            "desync": s.desync, "disconnected": s.disconnected, "outcome": s.outcome, "winner": s.winner,
            "rollbacks": st.rollbacks, "resim_frames": st.resim_frames, "max_rollback": st.max_rollback,
            "rollback_us_avg": if st.rollbacks > 0 { st.rollback_us / st.rollbacks as f64 } else { 0.0 },
            "rollback_us_max": st.rollback_us_max,
            "resim_us_per_frame": if st.resim_frames > 0 { st.rollback_us / st.resim_frames as f64 } else { 0.0 },
            "step_us_avg": if st.frames > 0 { st.step_us / st.frames as f64 } else { 0.0 },
            "stalls": st.stalls, "skips": st.skips, "hash_checks": st.hash_checks,
            "packets_sent": st.packets_sent, "packets_recv": st.packets_recv, "bytes_sent": st.bytes_sent, "bytes_recv": st.bytes_recv,
            "rtt_ms": s.rtt, "conditions": {"latency_ms": args.cond.latency_ms, "jitter_ms": args.cond.jitter_ms, "loss": args.cond.loss},
            "spect_behind": s.spect_behind,
            "replay": replay,
        });
        finish(args, r)
    }

    fn finish(args: &NetArgs, r: serde_json::Value) -> Result<serde_json::Value, String> {
        println!("NETRESULT {r}");
        if let Some(p) = &args.result {
            std::fs::write(p, serde_json::to_string_pretty(&r).unwrap()).map_err(|e| e.to_string())?;
        }
        Ok(r)
    }
}
