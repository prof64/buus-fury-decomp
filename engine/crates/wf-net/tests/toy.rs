//! Session logic on a toy game over the virtual network (no content needed).
use wf_net::game::Game;
use wf_net::session::{NetConfig, NetEvent};
use wf_net::synctest::SyncTestSession;
use wf_net::testnet::{self, ToyGame};
use wf_net::transport::{Conditions, SimNet};

fn cfg() -> NetConfig {
    NetConfig { session: 77, ..Default::default() }
}

fn agree(ps: &[testnet::PeerRun<ToyGame>], frames: i32) -> u64 {
    let live: Vec<_> = ps.iter().filter(|p| !p.left).collect();
    let h = live[0].sess.hashes[frames as usize];
    for p in &live {
        assert_eq!(p.sess.hashes[frames as usize], h, "{} disagrees at {frames}", p.id);
        assert_eq!(p.sess.chain[frames as usize], live[0].sess.chain[frames as usize]);
        assert!(!p.sess.events.iter().any(|e| matches!(e, NetEvent::Desync { .. })), "{}: {:?}", p.id, p.sess.events);
    }
    h
}

#[test]
fn two_peers_clean() {
    let net = SimNet::new(Conditions::default(), 1);
    let mut ps = testnet::peers(&net, vec![ToyGame::default(), ToyGame::default()], &cfg(), 0, 1);
    let r = testnet::run(&net, &mut ps, &mut [], 2000, 120_000.0);
    assert!(r.done);
    agree(&ps, 2000);
    // without latency nothing is mispredicted beyond the input delay
    for p in &ps {
        assert_eq!(p.sess.stats.max_rollback, 0, "{:?}", p.sess.stats);
    }
}

#[test]
fn bad_network_two_and_four_peers() {
    for (n, lat, jit, loss) in [(2, 100.0, 0.0, 0.05), (2, 50.0, 30.0, 0.10), (4, 100.0, 20.0, 0.05), (3, 150.0, 0.0, 0.20)] {
        let net = SimNet::new(Conditions { latency_ms: lat, jitter_ms: jit, loss }, 9);
        let mut ps = testnet::peers(&net, vec![ToyGame::default(); n], &NetConfig { max_prediction: 20, ..cfg() }, 0, 3);
        ps[1].start_tick = 7; // start skew: time sync has to even it out
        let r = testnet::run(&net, &mut ps, &mut [], 3000, 600_000.0);
        assert!(r.done, "n={n} lat={lat}: not done");
        agree(&ps, 3000);
        let rb: u64 = ps.iter().map(|p| p.sess.stats.rollbacks).sum();
        assert!(rb > 100, "n={n}: rollbacks {rb}");
        eprintln!(
            "n={n} lat={lat} jit={jit} loss={loss}: {:.1} s virtual, {:?}",
            r.ms / 1e3,
            ps.iter().map(|p| (p.sess.stats.rollbacks, p.sess.stats.max_rollback, p.sess.stats.stalls, p.sess.stats.skips, p.sess.peers[0].rtt_ms.round())).collect::<Vec<_>>()
        );
    }
}

#[test]
fn clock_drift_is_absorbed() {
    let net = SimNet::new(Conditions { latency_ms: 40.0, jitter_ms: 10.0, loss: 0.02 }, 5);
    let mut ps = testnet::peers(&net, vec![ToyGame::default(); 2], &cfg(), 0, 4);
    ps[1].period_ms *= 0.97; // 3 % fast clock
    let r = testnet::run(&net, &mut ps, &mut [], 4000, 600_000.0);
    assert!(r.done);
    agree(&ps, 4000);
    eprintln!("p0 {:?}\np1 {:?}", ps[0].sess.stats, ps[1].sess.stats);
    assert!(ps[1].sess.stats.skips > 20, "fast peer should wait: {:?}", ps[1].sess.stats);
}

#[test]
fn disconnect_mid_match() {
    let net = SimNet::new(Conditions { latency_ms: 60.0, jitter_ms: 0.0, loss: 0.05 }, 2);
    let mut ps = testnet::peers(&net, vec![ToyGame::default(); 3], &NetConfig { disconnect_ms: 1500.0, ..cfg() }, 0, 8);
    ps[2].leave_tick = Some(600);
    let r = testnet::run(&net, &mut ps, &mut [], 2500, 600_000.0);
    assert!(r.done);
    agree(&ps, 2500);
    for p in ps.iter().take(2) {
        assert!(p.sess.events.iter().any(|e| matches!(e, NetEvent::Disconnected { .. })), "{}", p.id);
    }
}

#[test]
fn desync_is_detected() {
    let net = SimNet::new(Conditions { latency_ms: 30.0, jitter_ms: 0.0, loss: 0.0 }, 3);
    let bad = ToyGame { bad_from: Some(500), ..Default::default() };
    let mut ps = testnet::peers(&net, vec![ToyGame::default(), bad], &cfg(), 0, 2);
    testnet::run(&net, &mut ps, &mut [], 900, 200_000.0);
    for p in &ps {
        let d = p.sess.events.iter().find_map(|e| match e {
            NetEvent::Desync { frame, .. } => Some(*frame),
            _ => None,
        });
        // state 501 is the first one that differs
        assert!(d.is_some_and(|f| (501..520).contains(&f)), "{}: {:?}", p.id, p.sess.events);
    }
}

#[test]
fn spectators_follow_the_host() {
    let net = SimNet::new(Conditions { latency_ms: 80.0, jitter_ms: 10.0, loss: 0.05 }, 4);
    let c = cfg();
    let mut ps = testnet::peers(&net, vec![ToyGame::default(); 2], &c, 2, 6);
    let mut sp = vec![testnet::spectator(&net, ToyGame::default(), &c, 2, 0), testnet::spectator(&net, ToyGame::default(), &c, 2, 1)];
    sp[1].start_tick = 900; // late joiner catches up
    let r = testnet::run(&net, &mut ps, &mut sp, 2000, 600_000.0);
    eprintln!("{:?} peers {:?} specs {:?}", r, ps.iter().map(|p| (p.sess.frame(), p.sess.hashed())).collect::<Vec<_>>(), sp.iter().map(|s| (s.sess.frame(), s.sess.hashes.len(), s.sess.inputs().len(), s.sess.packets_recv)).collect::<Vec<_>>());
    assert!(r.done);
    let h = agree(&ps, 2000);
    for s in &sp {
        assert_eq!(s.sess.hashes[2000], h);
        assert!(s.sess.events.is_empty(), "{:?}", s.sess.events);
    }
}

#[test]
fn synctest_catches_nondeterminism() {
    let mut ok = SyncTestSession::new(ToyGame::default(), 7);
    for f in 0..500u16 {
        ok.advance(&[f % 7, f % 3]).unwrap();
    }
    // a game whose result depends on how often it was simulated (state outside the snapshot)
    #[derive(Clone, Default)]
    struct Leaky(ToyGame, u64);
    impl Game for Leaky {
        type State = (u64, u32);
        fn save(&self) -> (u64, u32) {
            self.0.save()
        }
        fn load(&mut self, s: &(u64, u32)) {
            self.0.load(s)
        }
        fn advance(&mut self, i: &[u16], first: bool) {
            self.1 += 1;
            self.0.advance(i, first);
            if self.1 % 97 == 0 {
                self.0.acc ^= 5;
            }
        }
        fn checksum(&self, s: &(u64, u32)) -> u64 {
            self.0.checksum(s)
        }
    }
    let mut bad = SyncTestSession::new(Leaky::default(), 7);
    let err = (0..500u16).find_map(|f| bad.advance(&[f, 1]).err());
    assert!(err.is_some());
}

fn rejoined_events(p: &testnet::PeerRun<ToyGame>) -> usize {
    p.sess.events.iter().filter(|e| matches!(e, NetEvent::Rejoined { .. })).count()
}

/// a player drops (no goodbye), the others give its slot neutral input; a new connection of that player gets the
/// slot back from the acting host with a state snapshot, catches up and plays on: everyone agrees to the end
#[test]
fn rejoin_after_disconnect() {
    for (lat, loss, seed) in [(60.0, 0.05, 1u64), (120.0, 0.10, 2), (20.0, 0.0, 3)] {
        let net = SimNet::new(Conditions { latency_ms: lat, jitter_ms: 10.0, loss }, seed);
        let c = NetConfig { disconnect_ms: 1500.0, ..cfg() };
        let mut ps = testnet::peers(&net, vec![ToyGame::default(); 3], &c, 0, 8 + seed);
        ps[2].leave_tick = Some(600);
        let base = 1000.0 / 59.7275;
        let mut added = false;
        let r = testnet::run_with(&net, &mut ps, &mut [], 3000, 900_000.0, &mut |now, ps| {
            if added || now < 600.0 * base + 2000.0 {
                return None;
            }
            let p = testnet::try_rejoin(&net, ps, 0, 2, "peer2b", ToyGame::default(), &c, 99, now)?;
            added = true;
            Some(p)
        });
        assert!(added, "lat {lat}: the host never granted the rejoin");
        assert!(r.done, "lat {lat}: not done");
        let h = agree(&ps, 3000);
        let rj = ps.iter().find(|p| p.id == "peer2b").unwrap();
        assert_eq!(rj.sess.hashes[3000], h);
        for p in ps.iter().take(2) {
            assert_eq!(rejoined_events(p), 1, "{}: {:?}", p.id, p.sess.events);
        }
        // the slot is played again: the rejoiner's inputs are in everyone's log after the handover
        let from = ps[0].sess.events.iter().find_map(|e| if let NetEvent::Rejoined { from, .. } = e { Some(*from) } else { None }).unwrap();
        let used = &ps[1].sess.used_inputs()[from as usize..2900];
        assert!(used.iter().any(|u| u[2] != 0), "slot 2 stays neutral after the rejoin");
        eprintln!("rejoin lat {lat} loss {loss}: base {} handover at {from}, catch-up {} frames, rejoiner stats {:?}", rj.sess.base(), rj.sess.stats.catchup_frames, rj.sess.stats);
    }
}

/// the host leaves: the next player acts as host (serves the spectator, grants the host's rejoin)
#[test]
fn host_leaves_then_rejoins() {
    let net = SimNet::new(Conditions { latency_ms: 50.0, jitter_ms: 10.0, loss: 0.05 }, 21);
    let c = NetConfig { disconnect_ms: 1500.0, ..cfg() };
    let mut ps = testnet::peers(&net, vec![ToyGame::default(); 3], &c, 1, 31);
    let mut sp = vec![testnet::spectator(&net, ToyGame::default(), &c, 3, 0)];
    ps[0].leave_tick = Some(500);
    let base = 1000.0 / 59.7275;
    let mut added = false;
    let r = testnet::run_with(&net, &mut ps, &mut sp, 2500, 900_000.0, &mut |now, ps| {
        if added || now < 500.0 * base + 2500.0 {
            return None;
        }
        let p = testnet::try_rejoin(&net, ps, 1, 0, "peer0b", ToyGame::default(), &c, 77, now)?;
        added = true;
        Some(p)
    });
    assert!(added);
    assert!(r.done, "{:?}", sp[0].sess.frame());
    let h = agree(&ps, 2500);
    assert_eq!(sp[0].sess.hashes[2500], h);
    assert!(sp[0].sess.events.is_empty(), "{:?}", sp[0].sess.events);
    // back: the original slot's player is the lowest slot again, so it acts as host
    let back = ps.iter().find(|p| p.id == "peer0b").unwrap();
    assert!(back.sess.acting_host());
    assert!(!ps[1].sess.acting_host());
}

/// a wrong snapshot is refused (the hash of the loaded state must match the host's)
#[test]
fn rejoin_snapshot_is_verified() {
    let net = SimNet::new(Conditions::default(), 5);
    let c = NetConfig { disconnect_ms: 500.0, ..cfg() };
    let mut ps = testnet::peers(&net, vec![ToyGame::default(); 2], &c, 0, 3);
    ps[1].leave_tick = Some(200);
    testnet::run(&net, &mut ps, &mut [], 400, 60_000.0);
    let (g, bytes) = ps[0].sess.grant_rejoin(1, "x", 60_000.0).expect("grant");
    let mut bad = ToyGame::default();
    bad.load_bytes(&bytes);
    bad.acc ^= 1;
    assert!(wf_net::session::P2PSession::resume(bad, NetConfig { slots: 2, ..c.clone() }, &g, "x", 0.0).is_err());
    let mut good = ToyGame::default();
    good.load_bytes(&bytes);
    assert!(wf_net::session::P2PSession::resume(good, NetConfig { slots: 2, ..c }, &g, "x", 0.0).is_ok());
}
