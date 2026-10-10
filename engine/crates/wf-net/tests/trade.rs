//! Trading Post: two in-process peers on the virtual network (loopback, latency, loss), and fault injection at every
//! step - a peer killed and restarted from its journal before / after each packet it receives, the link cut - with
//! the invariants: item / zenie totals across both saves conserved, no trade applied twice, both sides agree
//! (both committed or both aborted), nothing applied on a decline or a changed offer. Headless.

use std::collections::BTreeMap;

use wf_net::trade::{Inventory, MemStore, Offer, TState, TradeEvent, TradePeer, TradeStore, SUPPLY_MAX};
use wf_net::transport::{Conditions, PeerChange, SimEndpoint, SimNet, Transport};

const DT: f64 = 10.0;

/// a transport view that counts the trade packets delivered and can stop at one (fault injection)
struct Tap<'a> {
    ep: &'a mut SimEndpoint,
    delivered: &'a mut usize,
    stop_at: Option<usize>,
    hit: bool,
}

impl Transport for Tap<'_> {
    fn local_id(&self) -> Option<String> {
        self.ep.local_id()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        self.ep.send(to, data, now)
    }
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for p in self.ep.recv(now) {
            if self.hit {
                continue; // (crashing: the rest is lost)
            }
            if self.stop_at == Some(*self.delivered) {
                self.hit = true;
                continue;
            }
            *self.delivered += 1;
            out.push(p);
        }
        out
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        self.ep.changes()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fault {
    None,
    /// side, packet index: the side dies before processing that packet (it and the rest of the tick's are lost)
    CrashBefore(usize, usize),
    /// the side processes the packet, then dies before anything else
    CrashAfter(usize, usize),
    /// the link between the two is cut when side 0 has received that many packets, for 3 s
    Cut(usize),
}

struct Side {
    id: String,
    store: MemStore,
    peer: Option<TradePeer>,
    ep: Option<SimEndpoint>,
    delivered: usize,
    restart_at: Option<f64>,
    nonce: u64,
    want: Offer,
    /// the bot trades once: after any settled record it only finishes what is pending
    done: bool,
    events: Vec<TradeEvent>,
}

fn inv(supplies: &[(usize, u32)], equip: &[(i32, bool)], zenie: u32) -> Inventory {
    let mut s = vec![0u32; 83];
    for &(i, n) in supplies {
        s[i] = n;
    }
    let mut story = vec![false; 83];
    story[60] = true;
    Inventory { supplies: s, story, equipment: equip.to_vec(), zenie }
}

fn offer(supplies: &[(u16, u32)], equip: &[i32], zenie: u32) -> Offer {
    Offer { supplies: supplies.iter().copied().collect(), equipment: equip.to_vec(), zenie }
}

/// conserved totals: (supplies by index, equipment by id, zenie)
fn totals(a: &Inventory, b: &Inventory) -> (BTreeMap<usize, u32>, BTreeMap<i32, usize>, u64) {
    let mut s = BTreeMap::new();
    for inv in [a, b] {
        for (i, &n) in inv.supplies.iter().enumerate() {
            if n > 0 {
                *s.entry(i).or_default() += n;
            }
        }
    }
    let mut e = BTreeMap::new();
    for inv in [a, b] {
        for &(id, _) in &inv.equipment {
            *e.entry(id).or_default() += 1;
        }
    }
    (s, e, a.zenie as u64 + b.zenie as u64)
}

struct Outcome {
    a: MemStore,
    b: MemStore,
    events: [Vec<TradeEvent>; 2],
    /// the fault fired with the crashed side's journal in this state (None: no record yet; not fired: absent)
    fired_at: Option<Option<TState>>,
}

fn run(cond: Conditions, seed: u64, invs: [Inventory; 2], wants: [Offer; 2], fault: Fault, max_ms: f64) -> Outcome {
    let net = SimNet::new(cond, seed);
    let mut sides: Vec<Side> = (0..2)
        .map(|i| {
            let id = format!("p{i}");
            let ep = net.endpoint(&id);
            Side { id, store: MemStore { inv: invs[i].clone(), journal: BTreeMap::new() }, peer: None, ep: Some(ep), delivered: 0, restart_at: None, nonce: 1000 + i as u64, want: wants[i].clone(), done: false, events: Vec::new() }
        })
        .collect();
    for i in 0..2 {
        let p = TradePeer::new(&sides[i].id, &sides[1 - i].id, sides[i].nonce, &sides[i].store);
        sides[i].peer = Some(p);
    }
    let mut cut_until: Option<f64> = None;
    let mut fired = false;
    let mut fired_at: Option<Option<TState>> = None;
    let mut t = 0.0;
    while t < max_ms {
        t += DT;
        if let Some(u) = cut_until {
            if t >= u {
                cut_until = None;
                // the link is back: a new connection between the two (the transport reconnects)
                for s in sides.iter_mut() {
                    net.leave(&s.id);
                }
                for s in sides.iter_mut() {
                    s.ep = Some(net.endpoint(&s.id));
                }
            }
        }
        for i in 0..2 {
            // a crashed side comes back (a new app run: a new nonce, its journal as it was on disk)
            if let Some(at) = sides[i].restart_at {
                if t >= at {
                    sides[i].restart_at = None;
                    sides[i].nonce += 7;
                    sides[i].ep = Some(net.endpoint(&sides[i].id));
                    let other = sides[1 - i].id.clone();
                    let p = TradePeer::new(&sides[i].id, &other, sides[i].nonce, &sides[i].store);
                    sides[i].peer = Some(p);
                    let settled = sides[i].store.records().iter().any(|r| matches!(r.state, TState::Committed | TState::Done | TState::Aborted));
                    sides[i].done = settled;
                }
            }
            let Some(mut peer) = sides[i].peer.take() else { continue };
            let Some(mut ep) = sides[i].ep.take() else {
                sides[i].peer = Some(peer);
                continue;
            };
            let stop_at = match fault {
                Fault::CrashBefore(s, k) if s == i => Some(k),
                Fault::CrashAfter(s, k) if s == i => Some(k + 1),
                _ => None,
            };
            // (a fault fires once)
            let stop_at = if fired { None } else { stop_at };
            // the bot: offer, accept the friend's offer once it is there and as expected, once
            if !sides[i].done {
                peer.set_offer(sides[i].want.clone());
                let theirs = sides[1 - i].want.clone();
                if peer.friend_offer() == Some(&theirs) {
                    let _ = peer.accept(&sides[i].store);
                }
            }
            let mut delivered = sides[i].delivered;
            let mut tap = Tap { ep: &mut ep, delivered: &mut delivered, stop_at, hit: false };
            let crash_before = matches!(fault, Fault::CrashBefore(..));
            let hit_now;
            {
                let st = &mut sides[i].store;
                if crash_before {
                    // pull the packets first: a hit means the side dies before processing them
                    let pkts = tap.recv(t);
                    hit_now = tap.hit;
                    if !hit_now {
                        // re-inject into a one-shot transport for the tick
                        let mut q = Replay { pkts, ep: &mut ep };
                        peer.tick(t, &mut q, st);
                    }
                } else {
                    peer.tick(t, &mut tap, st);
                    hit_now = tap.hit || (stop_at.is_some() && delivered >= stop_at.unwrap());
                }
            }
            sides[i].delivered = delivered;
            let evs = peer.events();
            for e in &evs {
                if matches!(e, TradeEvent::Complete(_) | TradeEvent::Aborted(..)) {
                    sides[i].done = true;
                }
            }
            sides[i].events.extend(evs);
            if hit_now && !fired && matches!(fault, Fault::CrashBefore(..) | Fault::CrashAfter(..)) {
                fired = true;
                fired_at = Some(sides[i].store.journal.values().map(|r| r.state).next());
                // the side dies: memory gone, the journal stays; back in 500 ms
                net.leave(&sides[i].id);
                sides[i].peer = None;
                sides[i].ep = None;
                sides[i].restart_at = Some(t + 500.0);
                continue;
            }
            if let Fault::Cut(k) = fault {
                if i == 0 && !fired && sides[0].delivered >= k {
                    fired = true;
                    fired_at = Some(sides[0].store.journal.values().map(|r| r.state).next());
                    // the connection drops (both sides see the friend leave); a new one 3 s later
                    net.leave("p0");
                    net.leave("p1");
                    cut_until = Some(t + 3000.0);
                }
            }
            sides[i].peer = Some(peer);
            sides[i].ep = Some(ep);
        }
        // settled on both sides and nothing pending: done
        let settled = sides.iter().all(|s| {
            s.peer.is_some() && s.done && s.store.records().iter().all(|r| !matches!(r.state, TState::Prepared | TState::Committed))
        });
        if settled && t > 2000.0 {
            break;
        }
    }
    let [a, b]: [Side; 2] = sides.try_into().ok().unwrap();
    Outcome { a: a.store, b: b.store, events: [a.events, b.events], fired_at }
}

/// packets already pulled from the endpoint, handed to one tick (the rest of the endpoint as usual)
struct Replay<'a> {
    pkts: Vec<(String, Vec<u8>)>,
    ep: &'a mut SimEndpoint,
}

impl Transport for Replay<'_> {
    fn local_id(&self) -> Option<String> {
        self.ep.local_id()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        self.ep.send(to, data, now)
    }
    fn recv(&mut self, _now: f64) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.pkts)
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        self.ep.changes()
    }
}

fn base() -> ([Inventory; 2], [Offer; 2]) {
    let a = inv(&[(0, 5), (3, 2)], &[(7, false), (7, true), (9, false)], 1000);
    let b = inv(&[(1, 4), (3, 250)], &[(12, false)], 50);
    let wa = offer(&[(0, 3), (3, 2)], &[7], 400);
    let wb = offer(&[(1, 4)], &[12], 20);
    ([a, b], [wa, wb])
}

/// the expected inventories after the trade
fn traded(invs: &[Inventory; 2], w: &[Offer; 2]) -> [Inventory; 2] {
    [invs[0].after(&w[0], &w[1]).unwrap(), invs[1].after(&w[1], &w[0]).unwrap()]
}

/// both sides committed the same trade and nothing else, or neither applied anything; totals conserved
fn check(o: &Outcome, invs: &[Inventory; 2], w: &[Offer; 2], label: &str) -> bool {
    let t0 = totals(&invs[0], &invs[1]);
    let t1 = totals(&o.a.inv, &o.b.inv);
    assert_eq!(t0, t1, "{label}: totals changed");
    let committed = |s: &MemStore| s.journal.values().filter(|r| matches!(r.state, TState::Committed | TState::Done)).map(|r| r.id.clone()).collect::<Vec<_>>();
    let (ca, cb) = (committed(&o.a), committed(&o.b));
    assert!(ca.len() <= 1 && cb.len() <= 1, "{label}: more than one trade applied: {ca:?} {cb:?}");
    assert_eq!(ca, cb, "{label}: the sides disagree");
    let pending = |s: &MemStore| s.journal.values().any(|r| matches!(r.state, TState::Prepared));
    assert!(!pending(&o.a) && !pending(&o.b), "{label}: a trade left in doubt: A {:?} B {:?} events {:?}", o.a.journal, o.b.journal, o.events);
    if ca.is_empty() {
        assert_eq!([&o.a.inv, &o.b.inv], [&invs[0], &invs[1]], "{label}: aborted but something changed");
        false
    } else {
        let want = traded(invs, w);
        assert_eq!([&o.a.inv, &o.b.inv], [&want[0], &want[1]], "{label}: applied wrongly");
        true
    }
}

#[test]
fn loopback_trade_completes() {
    let (invs, w) = base();
    let o = run(Conditions::default(), 1, invs.clone(), w.clone(), Fault::None, 20_000.0);
    assert!(check(&o, &invs, &w, "loopback"));
    for e in &o.events {
        assert!(e.iter().any(|x| matches!(x, TradeEvent::Complete(_))), "no TRADE COMPLETE: {e:?}");
    }
}

#[test]
fn lossy_latency_trade_completes() {
    let (invs, w) = base();
    for seed in 1..=8 {
        let c = Conditions { latency_ms: 120.0, jitter_ms: 80.0, loss: 0.3 };
        let o = run(c, seed, invs.clone(), w.clone(), Fault::None, 60_000.0);
        assert!(check(&o, &invs, &w, &format!("lossy seed {seed}")), "seed {seed}: not completed");
    }
}

#[test]
fn over_limit_aborts_on_both_sides() {
    let (invs, mut w) = base();
    // B holds 250 of supply 3: A's 2 more is fine; make it 6 (256 > 255)
    let mut a = invs[0].clone();
    a.supplies[3] = 6;
    let invs = [a, invs[1].clone()];
    w[0].supplies.insert(3, 6);
    assert!(invs[1].supplies[3] + 6 > SUPPLY_MAX);
    // the bot's accept is refused on B's side (it cannot take them): nothing ever agrees
    let o = run(Conditions::default(), 2, invs.clone(), w.clone(), Fault::None, 6_000.0);
    let t0 = totals(&invs[0], &invs[1]);
    assert_eq!(t0, totals(&o.a.inv, &o.b.inv));
    assert_eq!([&o.a.inv, &o.b.inv], [&invs[0], &invs[1]]);
    assert!(o.a.journal.values().chain(o.b.journal.values()).all(|r| r.state == TState::Aborted));
}

#[test]
fn decline_and_changed_offer_apply_nothing() {
    let (invs, w) = base();
    let net = SimNet::new(Conditions::default(), 3);
    let (mut ea, mut eb) = (net.endpoint("p0"), net.endpoint("p1"));
    let (mut sa, mut sb) = (MemStore { inv: invs[0].clone(), journal: BTreeMap::new() }, MemStore { inv: invs[1].clone(), journal: BTreeMap::new() });
    let mut a = TradePeer::new("p0", "p1", 1, &sa);
    let mut b = TradePeer::new("p1", "p0", 2, &sb);
    a.set_offer(w[0].clone());
    b.set_offer(w[1].clone());
    let mut t = 0.0;
    let mut step = |a: &mut TradePeer, b: &mut TradePeer, sa: &mut MemStore, sb: &mut MemStore, n: usize| {
        for _ in 0..n {
            t += DT;
            a.tick(t, &mut ea, sa);
            b.tick(t, &mut eb, sb);
        }
    };
    step(&mut a, &mut b, &mut sa, &mut sb, 50);
    // A accepts; B declines
    a.accept(&sa).unwrap();
    step(&mut a, &mut b, &mut sa, &mut sb, 50);
    b.decline();
    step(&mut a, &mut b, &mut sa, &mut sb, 50);
    assert!(a.events().iter().any(|e| *e == TradeEvent::Declined));
    // A accepted, then B changes its offer and accepts its new one: A's acceptance named B's old revision - void
    a.accept(&sa).unwrap();
    step(&mut a, &mut b, &mut sa, &mut sb, 30);
    b.set_offer(offer(&[(1, 1)], &[], 0));
    b.accept(&sb).unwrap();
    step(&mut a, &mut b, &mut sa, &mut sb, 100);
    assert!(sa.journal.is_empty() && sb.journal.is_empty(), "nothing may be agreed: {:?} {:?}", sa.journal, sb.journal);
    assert_eq!((&sa.inv, &sb.inv), (&invs[0], &invs[1]));
}

/// every protocol step: kill a side before / after each packet it receives (restart from its journal 500 ms later),
/// or cut the link there - on a clean and a bad network
#[test]
fn fault_injection_every_step() {
    let (invs, w) = base();
    let clean = run(Conditions::default(), 1, invs.clone(), w.clone(), Fault::None, 20_000.0);
    assert!(check(&clean, &invs, &w, "clean"));
    let mut cases = 0;
    let mut committed = 0;
    let mut at: BTreeMap<String, usize> = BTreeMap::new();
    for cond in [Conditions::default(), Conditions { latency_ms: 60.0, jitter_ms: 40.0, loss: 0.2 }] {
        for k in 0..30 {
            for fault in [Fault::CrashBefore(0, k), Fault::CrashBefore(1, k), Fault::CrashAfter(0, k), Fault::CrashAfter(1, k), Fault::Cut(k)] {
                let o = run(cond, 10 + k as u64, invs.clone(), w.clone(), fault, 90_000.0);
                if check(&o, &invs, &w, &format!("{fault:?} {cond:?}")) {
                    committed += 1;
                }
                *at.entry(format!("{:?}", o.fired_at)).or_default() += 1;
                cases += 1;
            }
        }
    }
    // (most cases still trade - the faults delay it; some abort before anything was prepared)
    assert!(committed > cases / 2, "{committed} of {cases} committed");
    // the faults must land in every phase: before any record, with a Prepared one, with a Committed / Done one
    eprintln!("faults by the crashed side's journal state: {at:?}");
    for k in ["Some(None)", "Some(Some(Prepared))", "Some(Some(Committed))"] {
        assert!(at.get(k).copied().unwrap_or(0) > 0, "no fault at {k}: {at:?}");
    }
}

/// a side prepared while the friend is gone stays in doubt (nothing applied, the save locked) and settles when the
/// friend comes back
#[test]
fn in_doubt_until_the_friend_returns() {
    let (invs, w) = base();
    let net = SimNet::new(Conditions::default(), 4);
    let (mut ea, mut eb) = (net.endpoint("p0"), net.endpoint("p1"));
    let (mut sa, mut sb) = (MemStore { inv: invs[0].clone(), journal: BTreeMap::new() }, MemStore { inv: invs[1].clone(), journal: BTreeMap::new() });
    let mut a = TradePeer::new("p0", "p1", 1, &sa);
    let mut b = TradePeer::new("p1", "p0", 2, &sb);
    a.set_offer(w[0].clone());
    b.set_offer(w[1].clone());
    let mut t = 0.0;
    // run until A has its Prepared record, then B vanishes before it sees A's
    let mut b_gone = false;
    let mut a_prepared_then = false;
    for _ in 0..2000 {
        t += DT;
        a.tick(t, &mut ea, &mut sa);
        if !b_gone {
            b.tick(t, &mut eb, &mut sb);
        }
        if a.friend_offer().is_some() && a.offer() == &w[0] && sa.journal.is_empty() {
            let _ = a.accept(&sa);
        }
        if b.friend_offer().is_some() && sb.journal.is_empty() {
            let _ = b.accept(&sb);
        }
        if !b_gone && sb.journal.values().any(|r| r.state == TState::Prepared) {
            b_gone = true;
            a_prepared_then = !sa.journal.is_empty();
            net.leave("p1");
        }
    }
    assert!(b_gone);
    // both prepared; A might have committed already if it saw B's Prepared before B left - either is legal; B is in doubt
    let b_rec = sb.journal.values().next().unwrap().clone();
    assert_eq!(b_rec.state, TState::Prepared, "B never heard A after preparing: in doubt");
    assert_eq!(sb.inv, invs[1], "nothing applied while in doubt");
    // B restarts much later (its journal intact) and reconnects: the trade settles to the same outcome on both
    let mut eb = net.endpoint("p1");
    let mut b = TradePeer::new("p1", "p0", 99, &sb);
    assert!(b.pending(&sb).is_some());
    for _ in 0..2000 {
        t += DT;
        a.tick(t, &mut ea, &mut sa);
        b.tick(t, &mut eb, &mut sb);
    }
    let o = Outcome { a: sa, b: sb, events: [a.events(), b.events()], fired_at: None };
    // A had prepared too -> it must commit; A never prepared -> it aborts (A refuses the unknown id) - either way the
    // same on both sides (check)
    assert_eq!(check(&o, &invs, &w, "in doubt"), a_prepared_then);
}
