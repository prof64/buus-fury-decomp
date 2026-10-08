//! Determinism, snapshot / restore (clone and bytes), state hash, observation and the frontend view contract.
mod common;
use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::session::Session;
use wf_view::{RenderView, Sim};

fn cfg(enemy: &str, arena: &str) -> BattleConfig {
    BattleConfig {
        arena: arena.into(),
        players: vec![FighterSetup::default()],
        phases: vec![Phase {
            enemy: enemy.into(),
            count: 3,
        }],
        rng: RngInit::Seed(4242),
        xp: true,
        spawn_near: None,
        arena_rules: false,
        even_level: None,
    }
}
/// a deterministic input script (pseudo-random buttons, changing every few frames)
fn keys(f: u32) -> u16 {
    let x = (f / 9).wrapping_mul(2654435761) >> 7;
    [0u16, 0x20, 0x10, 0x40, 0x80, 1, 1, 2, 0x100, 0x21, 0x12][(x % 11) as usize]
}

#[test]
fn same_inputs_same_hashes_and_snapshots_replay() {
    for (enemy, arena) in [
        ("t68", "forest"),
        ("t18", "demon"),
        ("t129", "crater"),
        ("t60", "wasteland"),
    ] {
        let mut a = Session::new(common::data(), cfg(enemy, arena));
        let mut b = Session::new(common::data(), cfg(enemy, arena));
        let mut hashes = vec![];
        let mut snap = None;
        let mut bytes = None;
        for f in 0..900 {
            if f == 400 {
                snap = Some(a.snapshot());
                bytes = Some(a.snapshot_bytes());
            }
            a.step(&[keys(f)]);
            b.step(&[keys(f)]);
            assert_eq!(a.hash(), b.hash(), "{enemy} frame {f}");
            hashes.push(a.hash());
        }
        // in-process snapshot
        let mut c = Session::new(common::data(), cfg(enemy, arena));
        c.restore(snap.as_ref().unwrap());
        for f in 400..900 {
            c.step(&[keys(f)]);
            assert_eq!(
                c.hash(),
                hashes[f as usize],
                "{enemy} clone-restore frame {f}"
            );
        }
        // serialized snapshot
        let mut d = Session::new(common::data(), cfg(enemy, arena));
        assert!(d.restore_bytes(bytes.as_ref().unwrap()));
        for f in 400..900 {
            d.step(&[keys(f)]);
            assert_eq!(
                d.hash(),
                hashes[f as usize],
                "{enemy} bytes-restore frame {f}"
            );
        }
    }
}

#[test]
fn view_and_observe_are_read_only() {
    let mut s = Session::new(common::data(), cfg("t68", "forest"));
    let mut v = RenderView::default();
    let mut ev = vec![];
    for f in 0..300 {
        Sim::step(&mut s, &[keys(f)], &mut ev);
        let h = s.hash();
        s.view(&mut v);
        let o = s.observe(0);
        let _ = serde_json::to_string(&o).unwrap();
        assert_eq!(h, s.hash());
    }
    assert!(!v.sprites.is_empty());
    assert_eq!(v.cameras.len(), 1);
    assert!(ev.iter().any(|e| matches!(e, wf_view::SimEvent::Audio(_))));
}

/// the scenario of engine/web/core_check.mjs: the wasm build prints the same hash (cross-target determinism;
/// WF_WASM_HASH="<hi> <lo>" asserts it)
#[test]
fn native_hash_of_the_wasm_check_scenario() {
    let cfg: BattleConfig = serde_json::from_str(r#"{"arena":"forest","players":[{"fighter":"goku","form":2,"level":60}],"phases":[{"enemy":"t68","count":4}],"rng":{"Seed":7}}"#).unwrap();
    let mut s = Session::new(common::data(), cfg);
    for f in 0..1200u32 {
        s.step(&[[0u16, 0x10, 0x20, 1, 0x40][((f >> 4) % 5) as usize]]);
    }
    eprintln!("native hash {:x} {:x}", s.hash() >> 32, s.hash() as u32);
    if let Ok(h) = std::env::var("WF_WASM_HASH") {
        assert_eq!(format!("{:x} {:x}", s.hash() >> 32, s.hash() as u32), h);
    }
}

#[test]
fn two_players_coop() {
    let mut c = cfg("t68", "forest");
    c.players.push(FighterSetup {
        fighter: "vegeta".into(),
        form: 26,
        ..Default::default()
    });
    let mut s = Session::new(common::data(), c);
    for f in 0..600 {
        s.step(&[keys(f), keys(f + 77)]);
    }
    let o = s.observe(1);
    assert_eq!(o.players.len(), 2);
    assert!(o.me.is_some());
}
