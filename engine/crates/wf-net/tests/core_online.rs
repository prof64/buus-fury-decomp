//! The internet-play features on wf-core battles over the virtual network: rejoin through the lobby (after a crash
//! with the rejoin ticket, and in place after a long network outage), the shared pause, rollback-aware sounds, PvP
//! homing. Needs the ROM (roms/buu.gba) or WF_CONTENT, like wf-core's tests.
mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::GameData;
use wf_core::shots::ShotKind;
use wf_core::world::Event;
use wf_net::cli::RandomInput;
use wf_net::core_game::{CoreGame, MatchConfig, SfxAudit, SfxMode, PAUSE_BIT};
use wf_net::game::Game;
use wf_net::lobby::{LobbySettings, Role};
use wf_net::netsim::{content_id, offer_transport, Clock, NetLobby, NetSim};
use wf_net::session::NetConfig;
use wf_net::testnet;
use wf_net::transport::{Conditions, SimNet};
use wf_view::{AudioEvent, Sim};

const FIGHTERS: [(&str, i32); 4] = [("goku", 2), ("vegeta", 26), ("gohan", 10), ("trunks", 21)];

fn battle(n: usize, enemy: &str, count: i32, arena: &str, seed: u32) -> BattleConfig {
    BattleConfig {
        arena: arena.into(),
        players: (0..n).map(|k| FighterSetup { fighter: FIGHTERS[k].0.into(), form: FIGHTERS[k].1, ..Default::default() }).collect(),
        phases: vec![Phase { enemy: enemy.into(), count }],
        rng: RngInit::Seed(seed),
        xp: false,
        spawn_near: None,
    }
}

fn settings(d: &GameData, role: Role, k: usize, base: &MatchConfig, players: usize, pause: &str) -> LobbySettings {
    LobbySettings {
        role,
        name: format!("p{k}"),
        fighter: FighterSetup { fighter: FIGHTERS[k].0.into(), form: FIGHTERS[k].1, ..Default::default() },
        content: content_id(d),
        want_players: players,
        base: base.clone(),
        pause: pause.into(),
        secret: format!("seat-secret-{k}"),
        ..Default::default()
    }
}

/// Peers on one virtual network driven through `NetLobby` -> `NetSim` like the frontends do.
struct Room {
    net: SimNet,
    t: Rc<Cell<f64>>,
    clock: Clock,
    data: Arc<GameData>,
    lobbies: Vec<Option<NetLobby>>,
    sims: Vec<Option<NetSim>>,
    inputs: Vec<RandomInput>,
    /// per peer: (from frame, n frames) the menu is open
    menu: Vec<Option<(i32, i32)>>,
    step: f64,
    k: u64,
}

impl Room {
    fn new(cond: Conditions, seed: u64, base: &MatchConfig, n: usize, pause: &str) -> Room {
        let data = common::data();
        let net = SimNet::new(cond, seed);
        let t = Rc::new(Cell::new(0.0f64));
        let tc = t.clone();
        let clock: Clock = Rc::new(move || tc.get());
        let lobbies = (0..n)
            .map(|k| {
                let role = if k == 0 { Role::Host } else { Role::Player };
                Some(NetLobby::new(Box::new(net.endpoint(&format!("p{k}"))), settings(&data, role, k, base, n, pause), "ROOMX", clock.clone()))
            })
            .collect();
        Room { net, t, clock, data, lobbies, sims: (0..n).map(|_| None).collect(), inputs: (0..n).map(|k| RandomInput::new(seed * 31 + k as u64)).collect(), menu: vec![None; n], step: 1000.0 / 59.7275, k: 0 }
    }
    fn now(&self) -> f64 {
        self.k as f64 * self.step
    }
    /// one tick of every peer
    fn tick(&mut self) {
        self.t.set(self.now());
        let mut ev = Vec::new();
        for i in 0..self.lobbies.len() {
            if let Some(l) = &mut self.lobbies[i] {
                if l.poll() {
                    let l = self.lobbies[i].take().unwrap();
                    self.sims[i] = Some(l.into_sim(self.data.clone()).expect("start"));
                }
            }
            if let Some(s) = &mut self.sims[i] {
                let f = s.status.borrow().frame;
                let mut v = self.inputs[i].next();
                if self.menu[i].is_some_and(|(a, n)| f >= a && f < a + n) {
                    v = s.paused_input().unwrap();
                }
                s.step(&[v], &mut ev);
                ev.clear();
            }
        }
        self.k += 1;
    }
    fn run_until(&mut self, max_ticks: u64, mut done: impl FnMut(&Room) -> bool) -> bool {
        for _ in 0..max_ticks {
            self.tick();
            if done(self) {
                return true;
            }
        }
        false
    }
    fn live(&self) -> Vec<&NetSim> {
        self.sims.iter().flatten().collect()
    }
    fn all_hashed(&self, target: i32, n: usize) -> bool {
        let l = self.live();
        l.len() == n && l.iter().all(|s| s.hashed() >= target)
    }
}

fn agree(sims: &[&NetSim], target: i32) -> u64 {
    let h = sims[0].hash_at(target).expect("hashed");
    for s in sims {
        assert_eq!(s.hash_at(target).unwrap(), h, "{} disagrees at {target}", s.me);
        assert!(s.status.borrow().desync.is_none(), "{}: {:?}", s.me, s.status.borrow().desync);
    }
    h.0
}

/// the replay a peer saves verifies on a fresh session (pause frames left out, a rejoin starts from its snapshot)
fn replay_verifies(s: &NetSim) -> wf_replay::VerifyReport {
    let r = s.replay(None);
    let r = wf_replay::Replay::from_bytes(&r.to_bytes()).unwrap();
    // the match as the host seated it (fighters in arrival order)
    let mut sess = s.start.config.session(common::data());
    wf_replay::verify(&r, &mut sess)
}

/// P3 crashes (vanishes without a goodbye); its restarted game rejoins with the ticket it saved, gets a snapshot
/// from the host, catches up and plays on. Everyone (and both replays) agree.
#[test]
fn rejoin_after_crash_with_ticket() {
    let m = MatchConfig::coop(battle(3, "t68", 4, "forest", 17));
    let mut room = Room::new(Conditions { latency_ms: 50.0, jitter_ms: 10.0, loss: 0.05 }, 3, &m, 3, "local");
    assert!(room.run_until(4000, |r| r.all_hashed(400, 3)), "match did not start");
    // crash: the endpoint leaves the network, the sim is gone without its Drop (no goodbye)
    let ticket = room.sims[2].as_ref().unwrap().ticket.clone().expect("ticket");
    let slot = ticket.slot;
    room.net.leave("p2");
    std::mem::forget(room.sims[2].take());
    let crash_frame = room.live()[0].status.borrow().frame;
    // back after 7 s (the others dropped it after 5)
    let back = room.k + (7000.0 / room.step) as u64;
    assert!(!room.run_until(back - room.k, |_| false));
    for s in room.live() {
        assert_eq!(s.status.borrow().disconnected, vec!["p2".to_string()], "{}", s.me);
    }
    let mut st = settings(&room.data, Role::Player, 2, &m, 3, "local");
    st.rejoin = Some(ticket);
    room.lobbies[2] = Some(NetLobby::new(Box::new(room.net.endpoint("p2-again")), st, "ROOMX", room.clock.clone()));
    let target = crash_frame + 1500;
    assert!(room.run_until(20_000, |r| r.all_hashed(target, 3)), "rejoined match did not reach {target}");
    let sims = room.live();
    let h = agree(&sims, target);
    let rj = sims[2];
    assert!(rj.rejoined());
    assert_eq!(rj.local_slot(), Some(slot));
    for s in &sims[..2] {
        assert_eq!(s.status.borrow().rejoined, vec!["p2".to_string()]);
        assert!(s.status.borrow().disconnected.is_empty());
    }
    // the rejoined player really plays again (its inputs reach the others)
    let wf_net::netsim::Mode::Player(p) = &sims[0].mode else { panic!() };
    let f = p.frame();
    assert!((f - 300..f - 10).any(|x| p.input_at(slot, x) != 0), "the rejoined slot stays neutral");
    for s in &sims {
        let v = replay_verifies(s);
        assert!(v.ok, "{}: replay {:?}", s.me, v.divergence);
    }
    eprintln!("rejoin after crash: crash at f{crash_frame}, agreed at {target} ({h:016x}), rejoiner stats {:?}", rj.status.borrow().stats);
}

/// 2 players; P2's network dies (no traffic, its signaling connection closes, P1 is not told). P1 notices the
/// silence but keeps its session (it cannot tell whose network died, and the other may come back to it); P2's game
/// asks for a new connection (`need_transport` -> `offer_transport`, what the browser's JS does), probes the room and
/// rejoins in place; the match goes on.
#[test]
fn rejoin_in_place_after_outage() {
    let m = MatchConfig::pvp(battle(2, "t68", 2, "forest", 5), true);
    let mut room = Room::new(Conditions { latency_ms: 40.0, jitter_ms: 10.0, loss: 0.03 }, 8, &m, 2, "local");
    assert!(room.run_until(4000, |r| r.all_hashed(300, 2)));
    let me = room.sims[1].as_ref().unwrap().me.clone();
    room.net.kill(&me);
    let cut_frame = room.live()[0].status.borrow().frame;
    let mut offered = false;
    let target = cut_frame + 1500;
    let ok = room.run_until(30_000, |r| {
        let s = r.sims[1].as_ref().unwrap();
        if !offered && s.status.borrow().need_transport {
            offered = true;
            offer_transport(Box::new(r.net.endpoint("p1-new")));
        }
        // the survivor never asks for a new connection: its signaling is fine
        assert!(!r.sims[0].as_ref().unwrap().status.borrow().need_transport);
        r.all_hashed(target, 2)
    });
    assert!(offered, "p1 never asked for a new connection");
    assert!(ok, "the match did not continue to {target}");
    let sims = room.live();
    agree(&sims, target);
    assert!(sims[1].rejoined());
    assert!(!sims[0].rejoined());
    assert_eq!(sims[1].me, "p1-new");
    for s in &sims {
        let v = replay_verifies(s);
        assert!(v.ok, "{}: replay {:?}", s.me, v.divergence);
    }
}

/// the room's shared pause: P2 opens its menu for 120 frames, the world stands still for everyone (deterministically,
/// no stall, no drop), then goes on; the local pause (default) only makes the player stand still
#[test]
fn shared_and_local_pause() {
    for mode in ["shared", "local"] {
        let m = MatchConfig::coop(battle(2, "t60", 1, "wasteland", 13));
        let mut room = Room::new(Conditions { latency_ms: 60.0, jitter_ms: 10.0, loss: 0.05 }, 4, &m, 2, mode);
        room.menu[1] = Some((400, 120));
        let mut seen_paused = false;
        let ok = room.run_until(20_000, |r| {
            if let Some(s) = &r.sims[0] {
                if s.status.borrow().paused_by == vec![1] {
                    seen_paused = true;
                }
            }
            r.all_hashed(1200, 2)
        });
        assert!(ok, "{mode}");
        let sims = room.live();
        agree(&sims, 1200);
        let wf_net::netsim::Mode::Player(p) = &sims[0].mode else { panic!() };
        let paused_frames = (0..1200).filter(|&f| p.used_inputs()[f as usize].iter().any(|v| v & PAUSE_BIT != 0)).count();
        if mode == "shared" {
            assert!(seen_paused, "the overlay never showed the pause");
            assert!((110..=130).contains(&paused_frames), "{paused_frames}");
        } else {
            assert_eq!(paused_frames, 0);
        }
        for s in &sims {
            assert_eq!(s.status.borrow().stats.stalls, 0, "{mode}: the menu must not stall the match");
            let v = replay_verifies(s);
            assert!(v.ok, "{mode} {}: replay {:?}", s.me, v.divergence);
            let r = s.replay(None);
            assert_eq!(r.inputs.len() as usize + paused_frames, s.hashed() as usize, "{mode}: paused frames are not in the replay");
        }
        eprintln!("{mode} pause: {paused_frames} paused frames, world frame {} at session frame {}", sims[0].frame(), sims[0].status.borrow().frame);
    }
}

/// Sounds under rollback: compare what each peer finally plays per frame with the sounds of a straight replay of
/// the confirmed inputs. Reconcile (cancel + late) is much closer to the truth than playing first simulations only.
#[test]
fn rollback_aware_sounds() {
    let m = MatchConfig::coop(battle(2, "t68", 3, "forest", 29));
    let mut results = Vec::new();
    for mode in [SfxMode::Predicted, SfxMode::Reconcile, SfxMode::Confirmed] {
        let net = SimNet::new(Conditions { latency_ms: 90.0, jitter_ms: 20.0, loss: 0.05 }, 6);
        let cfg = NetConfig { session: 9, max_prediction: 16, ..Default::default() };
        let games: Vec<CoreGame> = (0..2)
            .map(|_| {
                let mut g = CoreGame::new(m.session(common::data()));
                g.sfx = mode;
                g.audit = Some(Vec::new());
                g
            })
            .collect();
        let mut ps = testnet::peers(&net, games, &cfg, 0, 12);
        // Confirmed releases sounds from the confirmed frame: the driver does what NetSim does
        let r = testnet::run(&net, &mut ps, &mut [], 2400, 900_000.0);
        assert!(r.done);
        for p in ps.iter_mut() {
            let c = p.sess.confirmed();
            p.sess.game.release_confirmed(c);
        }
        // the truth: a straight replay of the confirmed inputs, every frame's sounds
        let mut truth = CoreGame::new(m.session(common::data()));
        truth.sfx = SfxMode::Predicted;
        truth.audit = Some(Vec::new());
        let known = ps[0].sess.known_inputs().to_vec();
        for f in 0..2000 {
            let inp: Vec<u16> = known.iter().map(|k| k[f]).collect();
            truth.advance(&inp, true);
        }
        let per_frame = |a: &[SfxAudit]| {
            let mut v: std::collections::BTreeMap<i32, Vec<u8>> = Default::default();
            for e in a {
                match e {
                    SfxAudit::Play(f, AudioEvent::PlaySfx { id, .. }) if *f < 2000 => v.entry(*f).or_default().push(*id),
                    SfxAudit::Stop(f, AudioEvent::PlaySfx { id, .. }) if *f < 2000 => {
                        let e = v.entry(*f).or_default();
                        if let Some(k) = e.iter().position(|x| x == id) {
                            e.remove(k);
                        }
                    }
                    _ => {}
                }
            }
            v
        };
        let t = per_frame(truth.audit.as_ref().unwrap());
        let total: usize = t.values().map(|v| v.len()).sum();
        let (mut wrong, mut missing) = (0, 0);
        for p in &ps {
            let h = per_frame(p.sess.game.audit.as_ref().unwrap());
            for f in 0..2000 {
                let mut a = h.get(&f).cloned().unwrap_or_default();
                let b = t.get(&f).cloned().unwrap_or_default();
                for id in &b {
                    match a.iter().position(|x| x == id) {
                        Some(k) => {
                            a.remove(k);
                        }
                        None => missing += 1,
                    }
                }
                wrong += a.len();
            }
        }
        let rb: u64 = ps.iter().map(|p| p.sess.stats.rollbacks).sum();
        let st: Vec<_> = ps.iter().map(|p| p.sess.game.sfx_stats.clone()).collect();
        eprintln!("sounds {mode:?}: {total} true sounds per peer, over 2 peers {wrong} wrong (played, did not happen), {missing} missing; {rb} rollbacks; {st:?}");
        results.push((mode, wrong, missing, total));
    }
    let (_, pw, pm, _) = results[0];
    let (_, rw, rm, _) = results[1];
    let (_, cw, cm, _) = results[2];
    assert!(pw + pm > 0, "no misprediction happened: the test proves nothing");
    assert!(rw + rm < (pw + pm) / 2, "reconcile {rw}+{rm} vs predicted {pw}+{pm}");
    assert_eq!((cw, cm), (0, 0), "confirmed sounds are exact");
}

/// PvP: a Kamikaze Ghost (homing) chases the opposing player when there are no enemies, and hits it; without the
/// PvP rule set it finds nothing and explodes
#[test]
fn pvp_ghost_homes_on_the_opponent() {
    let d = common::data();
    let mut out = Vec::new();
    for pvp in [true, false] {
        let mut s = MatchConfig::pvp(battle(2, "t68", 1, "htc", 3), false).session(d.clone());
        wf_core::battle::set_pvp(&mut s.world, pvp);
        for _ in 0..40 {
            s.step(&[0, 0]);
        }
        let (p1, p2) = (s.world.players[0], s.world.players[1]);
        // released far from P2 (below and beside it): it has to fly there
        let (x, y) = s.world.actor_by_id(p2).unwrap().center();
        let gid = wf_core::shots::ghost(&mut s.world, p1, 3, (x - 90) << 8, (y + 80) << 8, 0);
        let gi = s.world.find(gid).unwrap();
        if let ShotKind::Ghost(g) = &mut s.world.shot(gi).kind {
            g.free = true;
        }
        let dist = |s: &wf_core::session::Session| {
            let gi = s.world.find(gid)?;
            if s.world.ents[gi].removed {
                return None;
            }
            let (px, py) = (s.world.shot_ref(gi).px(), s.world.shot_ref(gi).py());
            let (tx, ty) = s.world.actor_by_id(p2).unwrap().center();
            Some((((tx - px) as f64).powi(2) + ((ty - py) as f64).powi(2)).sqrt())
        };
        let d0 = dist(&s);
        let (mut hit, mut alive) = (false, 0);
        for _ in 0..400 {
            let ev = s.step(&[0, 0]).to_vec();
            if dist(&s).is_some() {
                alive += 1;
            }
            hit |= ev.iter().any(|e| matches!(e, Event::Damage { target, attacker, .. } if *target == p2 && *attacker == p1));
        }
        eprintln!("ghost pvp={pvp}: start distance {d0:?}, flew {alive} frames, hit P2: {hit}");
        out.push((pvp, hit, alive));
    }
    assert!(out[0].1 && out[0].2 > 20, "PvP: the ghost must fly to P2 and hit it: {out:?}");
    assert!(!out[1].1 && out[1].2 < 30, "no PvP: no target, it explodes");
}
