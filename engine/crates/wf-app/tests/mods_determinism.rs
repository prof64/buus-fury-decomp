//! Mods and determinism (engine/docs/modding.md §8), headless and muted, on the extracted base pack + the example
//! mods (`engine/mods-examples/*`): a battle and a story session recorded with mods replay identically frame by frame
//! through the app's replay player; a save state saved and loaded with mods continues identically; a replay / save
//! state made with other mods (or none) is refused with a message naming both mod lists. (Online peers: wf-net's
//! `core_online` / `core_net` tests use the same content fingerprint; `mod_content_fingerprints_differ` below pins
//! that it tells mod sets apart.) Skipped without `roms/buu.gba`.

use std::path::PathBuf;
use std::sync::OnceLock;

use wf_app::control::{Control, Request};
use wf_app::hotkeys::{Action, Hotkeys};
use wf_app::story::StorySim;
use wf_app::wf_content::Packs;
use wf_app::App;
use wf_core::input as btn;

fn base_dir() -> Option<PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if let Ok(d) = std::env::var("WF_FULL_CONTENT") {
            return Some(PathBuf::from(d));
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-mods-content");
        if wf_extract::cache::fresh(&dir, &[]) {
            return Some(dir);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let rom = wf_rom::testutil::rom()?;
        let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
        for (p, b) in &c.files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
        Some(dir)
    })
    .clone()
}

fn packs(mods: &[&str]) -> Option<Packs> {
    let base = base_dir()?;
    let dirs: Vec<PathBuf> = mods.iter().map(|m| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods-examples").join(m)).collect();
    let p = wf_app::mods::load(&base, &dirs).expect("packs");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    Some(p)
}

const BATTLE_MODS: &[&str] = &["balance-tweak", "new-skill", "new-character", "scripted-skills", "new-boss"];

/// the new fighter (shadow Goku) with the mods' skills against a base enemy and the new boss type (150)
fn battle_app(p: Packs) -> App {
    let cfg = wf_core::battle::BattleConfig {
        players: vec![wf_core::battle::FighterSetup { fighter: "shadow_goku".into(), form: 54, ..Default::default() }],
        ..wf_app::battle::config("crater", "t68+t150", 1, "goku", None, None, 1, 3)
    };
    App::battle(p, cfg).expect("modded battle")
}

fn keys(f: u64) -> u16 {
    match (f / 45) % 6 {
        0 => btn::RIGHT,
        1 => btn::B,
        2 => btn::UP | btn::A,
        3 => btn::LEFT,
        4 => if f % 3 == 0 { btn::B } else { 0 },
        _ => btn::DOWN | btn::A,
    }
}

/// record `frames` frames through the app's controller: (replay bytes, state hash per frame)
fn record(app: &mut App, frames: u64) -> (Control, Vec<u8>, Vec<u64>) {
    let mut c = Control::new(app, Hotkeys::default(), 1_790_000_000);
    let mut h = vec![app.sim.state_hash()];
    for f in 0..frames {
        assert!(c.step(app, &[keys(f)]));
        h.push(app.sim.state_hash());
    }
    let (_, r) = c.current_replay(app).unwrap();
    (c, r.to_bytes(), h)
}

fn play(app: &mut App, bytes: &[u8], hashes: &[u64]) {
    let mut c = Control::new(app, Hotkeys::default(), 1_790_000_100);
    c.play_replay(app, "modded", bytes).expect("same mods: plays");
    let mut f = 0;
    while c.step(app, &[0]) {
        f += 1;
        assert_eq!(app.sim.state_hash(), hashes[f], "frame {f}");
    }
    assert_eq!(f, hashes.len() - 1);
    assert!(c.divergence().is_none());
}

#[test]
fn modded_battle_replays_and_save_states_are_exact() {
    let Some(p) = packs(BATTLE_MODS) else { return };
    let mut a = battle_app(p);
    let (mut c, bytes, hashes) = record(&mut a, 1500);
    // a fresh app with the same mods (different config running first: the replay's config wins)
    let rep = c.verify(&a, &bytes).unwrap();
    assert!(rep.ok && rep.frames == 1500, "{rep:?}");
    let mut b = battle_app(packs(BATTLE_MODS).unwrap());
    play(&mut b, &bytes, &hashes);
    // a save state at 900, loaded in another app with the same mods, continues exactly
    c.seek(&mut a, 900);
    c.act(&mut a, Action::SaveState(2));
    let st = c.take_requests().into_iter().find_map(|r| match r {
        Request::SaveState { bytes, .. } => Some(bytes),
        _ => None,
    });
    let st = st.expect("save state");
    let mut d = battle_app(packs(BATTLE_MODS).unwrap());
    let mut cd = Control::new(&mut d, Hotkeys::default(), 1_790_000_200);
    cd.load_state(&mut d, &st).expect("same mods: loads");
    assert_eq!(d.sim.state_hash(), hashes[900]);
    for f in 900..1500 {
        cd.step(&mut d, &[keys(f)]);
        assert_eq!(d.sim.state_hash(), hashes[f as usize + 1], "after the load, frame {}", f + 1);
    }
    // other mods (or none): refused with both lists, nothing started
    for other in [&[][..], &["balance-tweak"][..]] {
        let mut o = App::battle(packs(other).unwrap(), wf_app::battle::config("crater", "t68", 1, "goku", None, None, 1, 3)).unwrap();
        let mut co = Control::new(&mut o, Hotkeys::default(), 1_790_000_300);
        let e = co.play_replay(&mut o, "modded", &bytes).expect_err("other mods: refused");
        assert!(e.contains("made with mods: balance-tweak, new-boss, new-character, new-skill, scripted-skills") && e.contains("loaded now"), "{e}");
        assert!(!co.is_playback());
        let e = co.load_state(&mut o, &st).expect_err("other mods: refused");
        assert!(e.contains("save state") && e.contains("load the same mods"), "{e}");
    }
}

#[test]
fn modded_story_replay_is_exact() {
    // the new area's grove (mod map, mod NPC with a Rhai talk script) and the party member's 8-3 additions
    let mods = ["new-area", "new-party-member"];
    let Some(p) = packs(&mods) else { return };
    let mk = |p: Packs| {
        let s = StorySim::new(&p, (90, 2), vec![], 5).expect("story");
        App::new(p, Box::new(s))
    };
    let mut a = mk(p);
    let (mut c, bytes, hashes) = record(&mut a, 1200);
    assert!(c.verify(&a, &bytes).unwrap().ok);
    let mut b = mk(packs(&mods).unwrap());
    play(&mut b, &bytes, &hashes);
    c.seek(&mut a, 0);
    assert_eq!(a.sim.state_hash(), hashes[0]);
}

/// netplay with mods: two peers with the same mods over a lossy virtual network (rollbacks) agree every frame and
/// equal a straight run of the confirmed inputs; the rollback sync test (load / re-simulate 8 frames back every
/// frame) holds with the mods' Rhai skill hooks and AI nodes
#[test]
fn modded_netplay_peers_stay_in_sync() {
    use wf_net::core_game::{CoreGame, MatchConfig};
    use wf_net::transport::{Conditions, SimNet};
    let Some(p) = packs(BATTLE_MODS) else { return };
    let d = wf_app::battle::game_data(&p).unwrap();
    let battle = wf_core::battle::BattleConfig {
        arena: "crater".into(),
        players: vec![
            wf_core::battle::FighterSetup { fighter: "shadow_goku".into(), form: 54, ..Default::default() },
            wf_core::battle::FighterSetup { fighter: "goku".into(), form: 2, ..Default::default() },
        ],
        phases: vec![wf_core::battle::Phase { enemy: "t150".into(), count: 1 }, wf_core::battle::Phase { enemy: "t68".into(), count: 3 }],
        rng: wf_core::battle::RngInit::Seed(21),
        xp: false,
        spawn_near: None,
    };
    let m = MatchConfig::coop(battle);
    let frames = 2400;
    let net = SimNet::new(Conditions { latency_ms: 100.0, jitter_ms: 20.0, loss: 0.05 }, 9);
    let cfg = wf_net::session::NetConfig { session: 77, max_prediction: 16, ..Default::default() };
    let games: Vec<CoreGame> = (0..2)
        .map(|_| {
            let mut g = CoreGame::new(m.session(d.clone()));
            g.collect_events = false;
            g
        })
        .collect();
    let mut ps = wf_net::testnet::peers(&net, games, &cfg, 0, 9);
    let r = wf_net::testnet::run(&net, &mut ps, &mut [], frames, 900_000.0);
    assert!(r.done, "not finished");
    let h = ps[0].sess.hashes[frames as usize];
    assert_eq!(ps[1].sess.hashes[frames as usize], h);
    assert!(ps.iter().all(|q| !q.sess.events.iter().any(|e| matches!(e, wf_net::session::NetEvent::Desync { .. }))));
    assert!(ps.iter().map(|q| q.sess.stats.rollbacks).sum::<u64>() > 0, "no rollbacks");
    let straight = wf_net::netsim::replay_hashes(d.clone(), &m, ps[0].sess.known_inputs(), frames as usize);
    assert_eq!(straight[frames as usize], h, "rollback result != straight run");
    // rollback every frame
    let mut g = CoreGame::new(m.session(d.clone()));
    g.collect_events = false;
    let mut st = wf_net::synctest::SyncTestSession::new(g, 8);
    let mut rin: Vec<wf_net::cli::RandomInput> = (0..2).map(|k| wf_net::cli::RandomInput::new(300 + k)).collect();
    for f in 0..1500 {
        let inp: Vec<u16> = rin.iter_mut().map(|x| x.next()).collect();
        st.advance(&inp).unwrap_or_else(|e| panic!("synctest frame {f}: {e}"));
    }
}

#[test]
fn mod_content_fingerprints_differ() {
    let (Some(base), Some(one), Some(two)) = (packs(&[]), packs(&["balance-tweak"]), packs(&["balance-tweak", "new-skill"])) else { return };
    let ids = [&base, &one, &two].map(|p| wf_app::battle::content_id(p));
    assert_ne!(ids[0].mods_hash, ids[1].mods_hash);
    assert_ne!(ids[1].mods_hash, ids[2].mods_hash);
    assert!(ids[1].mods_mismatch(&ids[1].clone()).is_none());
    assert!(ids[1].mods_mismatch(&ids[2]).is_some());
    // the online lobby's fingerprint (game data + hook sources)
    let net = [&base, &one, &two].map(|p| wf_net::netsim::content_id(&wf_app::battle::game_data(p).unwrap()));
    assert_ne!(net[0], net[1]);
    assert_ne!(net[1], net[2]);
}
