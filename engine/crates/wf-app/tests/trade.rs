//! Trading Post on real save slot files ([`wf_app::trade::FileStore`]): two peers on the virtual network trade
//! between their slot files; the commit's write-ahead log survives a crash after each of its writes (the store is
//! re-opened from disk, recovery finishes the commit once); totals conserved, the worn item stays. Headless.

use std::path::Path;

use wf_app::trade::{apply, inventory_of, read_slot, slot_bytes, FileStore};
use wf_core::menu::save::SaveData;
use wf_core::story::{CharState, Story};
use wf_net::trade::{Offer, TState, TradeEvent, TradePeer, TradeStore};
use wf_net::transport::{Conditions, SimNet};

fn story_flags() -> Vec<bool> {
    let mut v = vec![false; 83];
    v[60] = true;
    v
}

fn save(supplies: &[(usize, u8)], equipment: &[i32], worn_body: i32, zenie: u32) -> SaveData {
    let mut st = Story::default();
    st.supplies = vec![0; 83];
    for &(i, n) in supplies {
        st.supplies[i] = n;
    }
    st.equipment = equipment.to_vec();
    st.zenie = zenie;
    let mut c = CharState::default();
    c.equip = [worn_body, 44, 77, 112];
    st.chars = vec![c];
    SaveData { story: st, world: 2, area: 1, variant: 1, x: 100, y: 100, flag: true }
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("wf-trade-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(p: &Path, sd: &SaveData) {
    std::fs::write(p, slot_bytes(sd)).unwrap();
}

fn offer(supplies: &[(u16, u32)], equip: &[i32], zenie: u32) -> Offer {
    Offer { supplies: supplies.iter().copied().collect(), equipment: equip.to_vec(), zenie }
}

/// one trade between two slot files; `crash`: (side, WAL step) - that side's first commit stops after the step and
/// its store is re-opened from disk (the app restarted)
fn trade(name: &str, crash: Option<(usize, u8)>) -> (SaveData, SaveData, [SaveData; 2], [Offer; 2]) {
    let d = dir(name);
    let (pa, pb) = (d.join("slot1.sav"), d.join("slot2.sav"));
    // A wears one of its two Weighted Gis (200) and offers the other one
    let a0 = save(&[(0, 5), (3, 2)], &[200, 200, 9], 200, 1000);
    let b0 = save(&[(1, 4)], &[12], 0, 50);
    write(&pa, &a0);
    write(&pb, &b0);
    let w = [offer(&[(0, 3), (3, 2)], &[200], 400), offer(&[(1, 4)], &[12], 20)];
    let net = SimNet::new(Conditions { latency_ms: 40.0, jitter_ms: 20.0, loss: 0.1 }, 7);
    let mut eps = [net.endpoint("a"), net.endpoint("b")];
    let mut stores = [FileStore::open(&pa, story_flags()).unwrap(), FileStore::open(&pb, story_flags()).unwrap()];
    if let Some((s, step)) = crash {
        stores[s].crash_after = Some(step);
    }
    let mut peers = [TradePeer::new("a", "b", 1, &stores[0]), TradePeer::new("b", "a", 2, &stores[1])];
    let mut done = [false, false];
    let mut reopened = 0;
    let mut t = 0.0;
    for _ in 0..20_000 {
        t += 10.0;
        for i in 0..2 {
            if !done[i] {
                peers[i].set_offer(w[i].clone());
                if peers[i].friend_offer() == Some(&w[1 - i]) {
                    let _ = peers[i].accept(&stores[i]);
                }
            }
            peers[i].tick(t, &mut eps[i], &mut stores[i]);
            for e in peers[i].events() {
                match e {
                    TradeEvent::Complete(_) => done[i] = true,
                    TradeEvent::Aborted(_, why) if why.contains("test crash") => {
                        // the app died mid-commit: re-open from disk (recovery), a new session
                        reopened += 1;
                        let p = stores[i].slot.clone();
                        stores[i] = FileStore::open(&p, story_flags()).unwrap();
                        peers[i] = TradePeer::new(if i == 0 { "a" } else { "b" }, if i == 0 { "b" } else { "a" }, 50 + i as u64, &stores[i]);
                        done[i] = stores[i].records().iter().any(|r| matches!(r.state, TState::Committed | TState::Done));
                    }
                    TradeEvent::Aborted(_, why) => panic!("aborted: {why}"),
                    _ => {}
                }
            }
        }
        if done.iter().all(|&x| x) && stores.iter().all(|s| s.records().iter().all(|r| r.state == TState::Done)) {
            break;
        }
    }
    assert_eq!(reopened, crash.is_some() as usize, "the crash must have happened (once)");
    let (a1, b1) = (read_slot(&pa).unwrap(), read_slot(&pb).unwrap());
    (a1, b1, [a0, b0], w)
}

fn expect(a1: &SaveData, b1: &SaveData, s0: &[SaveData; 2], w: &[Offer; 2]) {
    let f = story_flags();
    let a_want = apply(&s0[0], &f, &w[0], &w[1]).unwrap();
    let b_want = apply(&s0[1], &f, &w[1], &w[0]).unwrap();
    assert_eq!(a1, &a_want);
    assert_eq!(b1, &b_want);
    // the worn Weighted Gi stayed (one 200 left, still worn)
    assert_eq!(a1.story.equipment.iter().filter(|&&x| x == 200).count(), 1);
    assert!(inventory_of(a1, &f).equipment.iter().any(|&(id, worn)| id == 200 && worn));
    // totals
    let tot = |x: &SaveData, y: &SaveData| (x.story.zenie + y.story.zenie, x.story.supplies.iter().chain(&y.story.supplies).map(|&n| n as u32).sum::<u32>(), x.story.equipment.len() + y.story.equipment.len());
    assert_eq!(tot(a1, b1), tot(&s0[0], &s0[1]));
}

#[test]
fn slot_files_trade() {
    let (a1, b1, s0, w) = trade("plain", None);
    expect(&a1, &b1, &s0, &w);
}

#[test]
fn crash_after_each_wal_write() {
    for side in 0..2 {
        for step in [1u8, 2] {
            let (a1, b1, s0, w) = trade(&format!("crash{side}{step}"), Some((side, step)));
            expect(&a1, &b1, &s0, &w);
        }
    }
}

#[test]
fn worn_items_are_traded() {
    // (as the game: a worn copy can be offered; it leaves the list and its wearer's slot goes back to "-None-" -
    // link recordings data/link/trade_*.json)
    let f = story_flags();
    let a = save(&[], &[200], 200, 0);
    let give = offer(&[], &[200], 0);
    assert!(inventory_of(&a, &f).check_give(&give).is_ok());
    let a1 = apply(&a, &f, &give, &Offer::default()).unwrap();
    assert!(a1.story.equipment.is_empty());
    assert_eq!(a1.story.chars[0].equip[0], 0, "the wearer's body slot is -None-");
    assert!(inventory_of(&a, &f).check_give(&offer(&[], &[200, 200], 0)).is_err(), "one copy only");
    let mut story = Offer::default();
    story.supplies.insert(60, 1);
    let b = save(&[(60, 1)], &[], 0, 0);
    assert!(inventory_of(&b, &f).check_give(&story).is_err());
}
