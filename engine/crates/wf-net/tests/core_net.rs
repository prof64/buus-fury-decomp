//! wf-core battles through the rollback session on the virtual network: co-op 2–4 players vs bosses / groups and
//! PvP with latency, jitter and loss; the lobby flow end to end; PvP hit rules; sync test over multiplayer battles.
//! Needs the ROM (roms/buu.gba) or WF_CONTENT, like wf-core's tests.
mod common;

use std::cell::Cell;
use std::rc::Rc;

use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::session::Session;
use wf_core::world::{Body, Event};
use wf_net::cli::RandomInput;
use wf_net::core_game::{CoreGame, MatchConfig};
use wf_net::lobby::{LobbySettings, Role};
use wf_net::netsim::{content_id, Clock, NetLobby, NetSim};
use wf_net::session::{NetConfig, NetEvent};
use wf_net::synctest::SyncTestSession;
use wf_net::testnet;
use wf_net::transport::{Conditions, SimNet, Transport};

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

fn games(m: &MatchConfig) -> Vec<CoreGame> {
    let d = common::data();
    (0..m.battle.players.len())
        .map(|_| {
            let mut g = CoreGame::new(m.session(d.clone()));
            g.collect_events = false;
            g
        })
        .collect()
}

fn run_match(name: &str, m: &MatchConfig, cond: Conditions, frames: i32, seed: u64) -> Vec<testnet::PeerRun<CoreGame>> {
    let net = SimNet::new(cond, seed);
    let cfg = NetConfig { session: 1234, max_prediction: 16, ..Default::default() };
    let mut ps = testnet::peers(&net, games(m), &cfg, 0, seed);
    ps[1].start_tick = 5;
    let r = testnet::run(&net, &mut ps, &mut [], frames, 900_000.0);
    assert!(r.done, "{name}: not finished");
    let h = ps[0].sess.hashes[frames as usize];
    for p in &ps {
        assert_eq!(p.sess.hashes[frames as usize], h, "{name}: {} disagrees", p.id);
        assert_eq!(p.sess.chain[frames as usize], ps[0].sess.chain[frames as usize], "{name}");
        assert!(!p.sess.events.iter().any(|e| matches!(e, NetEvent::Desync { .. })), "{name}: {:?}", p.sess.events);
    }
    // and the networked result is exactly the plain session with the confirmed inputs
    let replay = wf_net::netsim::replay_hashes(common::data(), m, ps[0].sess.known_inputs(), frames as usize);
    assert_eq!(replay[frames as usize], h, "{name}: rollback result != straight replay");
    let rb: u64 = ps.iter().map(|p| p.sess.stats.rollbacks).sum();
    let rf: u64 = ps.iter().map(|p| p.sess.stats.resim_frames).sum();
    let us: f64 = ps.iter().map(|p| p.sess.stats.rollback_us).sum();
    let mx = ps.iter().map(|p| p.sess.stats.rollback_us_max).fold(0.0, f64::max);
    let step: f64 = ps.iter().map(|p| p.sess.stats.step_us).sum::<f64>() / ps.iter().map(|p| p.sess.stats.frames).sum::<u64>() as f64;
    eprintln!(
        "{name}: {} peers, {frames} frames, lat {} jit {} loss {}: 0 desyncs, rollbacks {rb} ({rf} frames re-simulated, max depth {}), {:.0} us/rollback avg, {:.0} us max, {:.1} us/re-simulated frame, first-time frame {:.1} us (incl. save), stalls {:?}, skips {:?}, rtt {:?}",
        ps.len(),
        cond.latency_ms,
        cond.jitter_ms,
        cond.loss,
        ps.iter().map(|p| p.sess.stats.max_rollback).max().unwrap(),
        us / rb.max(1) as f64,
        mx,
        us / rf.max(1) as f64,
        step,
        ps.iter().map(|p| p.sess.stats.stalls).collect::<Vec<_>>(),
        ps.iter().map(|p| p.sess.stats.skips).collect::<Vec<_>>(),
        ps.iter().map(|p| p.sess.peers[0].rtt_ms.round()).collect::<Vec<_>>(),
    );
    assert!(rb > 0, "{name}: no rollbacks");
    ps
}

#[test]
fn coop_over_bad_network() {
    let c = |l, j, s| Conditions { latency_ms: l, jitter_ms: j, loss: s };
    run_match("2p vs Kid Buu", &MatchConfig::coop(battle(2, "t60", 1, "wasteland", 13)), c(100.0, 0.0, 0.05), 3000, 1);
    run_match("4p vs Janemba", &MatchConfig::coop(battle(4, "t51", 1, "crater", 47)), c(100.0, 20.0, 0.05), 3000, 2);
    run_match("3p vs 6 Majin Fighters", &MatchConfig::coop(battle(3, "t68", 6, "forest", 7)), c(60.0, 30.0, 0.10), 3000, 3);
    run_match("2p vs Super Buu + helper", &MatchConfig::coop(battle(2, "t26+~t29", 1, "crater", 59)), c(100.0, 0.0, 0.05), 2400, 4);
    run_match("4p vs Yakon", &MatchConfig::coop(battle(4, "t145", 1, "yakon", 23)), c(150.0, 0.0, 0.05), 2400, 5);
}

#[test]
fn pvp_over_bad_network() {
    run_match("2p PvP", &MatchConfig::pvp(battle(2, "t68", 1, "htc", 3), false), Conditions { latency_ms: 100.0, jitter_ms: 0.0, loss: 0.05 }, 3000, 6);
    run_match("4p PvP + enemies", &MatchConfig::pvp(battle(4, "t68", 2, "forest", 11), true), Conditions { latency_ms: 80.0, jitter_ms: 20.0, loss: 0.05 }, 3000, 7);
}

/// players deal each other damage through the normal pipeline only with the PvP rule set
#[test]
fn pvp_rule_set() {
    let d = common::data();
    let count = |pvp: bool| -> (usize, usize, wf_core::battle::Outcome, Option<usize>) {
        let m = if pvp { MatchConfig::pvp(battle(2, "t68", 1, "htc", 3), false) } else { MatchConfig::coop(battle(2, "t68", 1, "htc", 3)) };
        let mut s: Session = m.session(d.clone());
        let ids = s.world.players.clone();
        let (mut pp, mut hits) = (0, 0);
        // both walk toward each other for 14 frames, then punch (knockback separates them again)
        for f in 0..3000 {
            use wf_core::input::{A, LEFT, RIGHT};
            let act = |k: i32, dir: u16| {
                let ph = (f + k) % 40;
                if ph < 14 {
                    dir
                } else if ph % 8 < 2 {
                    A
                } else {
                    0
                }
            };
            let inp = [act(0, RIGHT), act(20, LEFT)];
            let ev = s.step(&inp).to_vec();
            for e in ev {
                if let Event::Damage { target, attacker, .. } = e {
                    if ids.contains(&target) && ids.contains(&attacker) && target != attacker {
                        pp += 1;
                    }
                }
                if e == Event::Hit {
                    hits += 1;
                }
            }
            if f > 100 && s.outcome != wf_core::battle::Outcome::Running {
                break;
            }
        }
        (pp, hits, s.outcome, wf_core::battle::pvp_winner(&s.world))
    };
    let (off, ..) = count(false);
    let (on, hits, outcome, winner) = count(true);
    eprintln!("player-vs-player damage events: off {off}, on {on} (hit cues {hits}), outcome {outcome:?}, winner {winner:?}");
    assert_eq!(off, 0);
    assert!(on >= 5, "{on}");
    assert_eq!(outcome, wf_core::battle::Outcome::Won);
    assert!(winner.is_some());
    // a PvP world without enemies does not end as an instant win
    let mut s = MatchConfig::pvp(battle(2, "t68", 1, "htc", 3), false).session(d.clone());
    for _ in 0..10 {
        s.step(&[0, 0]);
    }
    assert_eq!(s.outcome, wf_core::battle::Outcome::Running);
    assert!(s.world.ents.iter().all(|e| !matches!(e.body, Body::Enemy(_))));
}

#[test]
fn synctest_multiplayer_battles() {
    let d = common::data();
    let cases: Vec<(&str, MatchConfig, bool)> = vec![
        ("4p co-op Majin x8 plateau", MatchConfig::coop(battle(4, "t68", 8, "plateau", 9)), false),
        ("3p co-op Janemba", MatchConfig::coop(battle(3, "t51", 1, "crater", 47)), true),
        ("2p co-op Broly", MatchConfig::coop(battle(2, "t129", 1, "crater", 31)), false),
        ("2p PvP htc", MatchConfig::pvp(battle(2, "t68", 1, "htc", 3), false), true),
        ("4p PvP + enemies", MatchConfig::pvp(battle(4, "t68", 3, "forest", 11), true), false),
        ("1p Kid Buu", MatchConfig::coop(battle(1, "t60", 1, "wasteland", 13)), true),
    ];
    for (name, m, bytes) in cases {
        let n = m.battle.players.len();
        let mut g = CoreGame::new(m.session(d.clone()));
        g.collect_events = false;
        let mut st = SyncTestSession::new(g, 8);
        st.bytes = bytes;
        let mut r: Vec<RandomInput> = (0..n).map(|k| RandomInput::new(100 + k as u64)).collect();
        for f in 0..1500 {
            let inp: Vec<u16> = r.iter_mut().map(|x| x.next()).collect();
            if let Err(e) = st.advance(&inp) {
                panic!("{name}: frame {f}: {e}");
            }
        }
        eprintln!("synctest {name}: 1500 frames x 8 rollback, bytes {bytes}: ok ({} frames re-simulated)", st.resim_frames);
    }
}

/// host + joiner + spectator through the lobby (hellos, start, fighters) and the NetSim frontend adapter
#[test]
fn lobby_to_match() {
    let d = common::data();
    let content = content_id(&d);
    let net = SimNet::new(Conditions { latency_ms: 40.0, jitter_ms: 10.0, loss: 0.05 }, 11);
    let t = Rc::new(Cell::new(0.0f64));
    let tc = t.clone();
    let clock: Clock = Rc::new(move || tc.get());
    let base = MatchConfig::coop(battle(1, "t60", 1, "wasteland", 13));
    let mk = |role: Role, fighter: &str, form: i32| LobbySettings {
        role,
        name: format!("{fighter}-{role:?}"),
        fighter: FighterSetup { fighter: fighter.into(), form, ..Default::default() },
        content: content.clone(),
        want_players: 2,
        base: base.clone(),
        input_delay: 2,
        max_prediction: 12,
        secret: format!("secret-{fighter}-{role:?}"),
        ..Default::default()
    };
    let mut lobbies = vec![
        Some(NetLobby::new(Box::new(net.endpoint("host")), mk(Role::Host, "goku", 2), "ABCDE", clock.clone())),
        Some(NetLobby::new(Box::new(net.endpoint("joiner")), mk(Role::Player, "vegeta", 27), "ABCDE", clock.clone())),
        Some(NetLobby::new(Box::new(net.endpoint("watcher")), mk(Role::Spectator, "goku", 2), "ABCDE", clock.clone())),
    ];
    // a peer with other content is refused
    let mut odd = mk(Role::Player, "gohan", 10);
    odd.content = "something else".into();
    let mut rogue = NetLobby::new(Box::new(net.endpoint("rogue")), odd, "ABCDE", clock.clone());
    let mut sims: Vec<Option<NetSim>> = vec![None, None, None];
    let step = 1000.0 / 59.7275;
    let mut ev = Vec::new();
    let mut k = 0;
    let target = 900;
    while k < 20_000 {
        t.set(k as f64 * step);
        rogue.poll();
        for i in 0..3 {
            if let Some(l) = &mut lobbies[i] {
                if l.poll() {
                    let l = lobbies[i].take().unwrap();
                    sims[i] = Some(l.into_sim(d.clone()).expect("start"));
                }
            }
            if let Some(s) = &mut sims[i] {
                use wf_view::Sim;
                s.step(&[if i == 0 { 0x11 } else { 0x21 }], &mut ev);
                ev.clear();
            }
        }
        if sims.iter().all(|s| s.as_ref().is_some_and(|s| s.hashed() >= target)) {
            break;
        }
        k += 1;
    }
    assert!(rogue.lobby.rejected.is_some(), "content mismatch must be refused");
    let s: Vec<&NetSim> = sims.iter().map(|s| s.as_ref().expect("all started")).collect();
    assert_eq!(s[0].start.config.battle.players.iter().map(|p| (p.fighter.as_str(), p.form)).collect::<Vec<_>>(), vec![("goku", 2), ("vegeta", 27)]);
    assert_eq!(s[0].local_slot(), Some(0));
    assert_eq!(s[1].local_slot(), Some(1));
    assert_eq!(s[2].local_slot(), None);
    let h = s[0].hash_at(target).unwrap();
    assert_eq!(s[1].hash_at(target).unwrap(), h);
    assert_eq!(s[2].hash_at(target).unwrap(), h);
    for x in &s {
        assert!(x.status.borrow().desync.is_none());
    }
    eprintln!("lobby -> match: started and agreed at state {target} after {:.1} s virtual; host status {}", k as f64 * step / 1e3, s[0].status.borrow().to_json());
    let _ = net.endpoint("late").local_id();
}
