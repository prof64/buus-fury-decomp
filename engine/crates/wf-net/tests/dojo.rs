//! Link play: the Dojo between two in-process peers on the virtual network (engine/docs/netcode.md "Link play:
//! Dojo"): the `dojo` room (save pick, character, the host's mode / rounds and the guest's confirm), the rounds as a
//! rollback match over latency, jitter and loss, the result and the prize exactly once, a disconnect mid-match, and a
//! sync test over the whole match. Headless, muted; needs the ROM (roms/buu.gba) or WF_CONTENT, like wf-core's tests.
mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use wf_core::content::GameData;
use wf_core::menu::save::SaveData;
use wf_core::story::Story;
use wf_net::dojo::{fighter_setup, DojoGame, DojoHello, DojoMatch, DojoMode, DojoResult, DojoRules, DojoSave, DojoSetup, Ending, Phase};
use wf_net::game::Game;
use wf_net::lobby::{LobbySettings, Role, StartInfo, KIND_DOJO, KIND_MATCH};
use wf_net::netsim::{content_id, Clock, NetLobby};
use wf_net::session::{NetConfig, NetEvent};
use wf_net::synctest::SyncTestSession;
use wf_net::testnet;
use wf_net::transport::{Conditions, SimNet};

/// a save whose party is the given characters (index, level)
fn save(d: &GameData, party: &[(usize, i32)]) -> SaveData {
    let mut story = Story::new_game(d);
    for c in story.chars.iter_mut() {
        c.flags &= !1;
    }
    for &(i, lv) in party {
        story.chars[i].flags |= 1;
        story.chars[i].rec.level = lv;
        story.chars[i].rec.str_ += lv / 4;
        story.chars[i].rec.end += lv / 5;
    }
    SaveData { story, world: 2, area: 1, variant: 1, x: 100, y: 100, flag: false }
}

fn rules() -> DojoRules {
    // shorter rounds than the game's 180 s and a round cap keep the test quick (the ROM has no cap: draws repeat the
    // round), Balanced HP a KO fits in, and a prize (the ROM gives none) to check it is written exactly once
    // (`dojo_rom_rules`: the ROM's values)
    DojoRules { round_frames: 1500, between_frames: 60, start_frames: 125, balanced_hp: 300, max_rounds: 5, prize: Some(21) }
}

/// the lobby part: host (Goku's save, its setup) + guest (Vegeta's save) + a peer from a battle room (refused);
/// returns the host's and the guest's start
fn lobby(d: &Arc<GameData>, setup: DojoSetup, seed: u64) -> (StartInfo, StartInfo, Option<String>) {
    let fd = common::field_data();
    let content = content_id(d);
    let net = SimNet::new(Conditions { latency_ms: 120.0, jitter_ms: 30.0, loss: 0.05 }, seed);
    let t = Rc::new(Cell::new(0.0f64));
    let tc = t.clone();
    let clock: Clock = Rc::new(move || tc.get());
    let host_save = DojoSave::from_save(0, &save(&fd, &[(0, 90)]), &fd).expect("host party");
    let guest_save = DojoSave::from_save(1, &save(&fd, &[(4, 110), (3, 80)]), &fd).expect("guest party");
    // the In-Game roster shows display names, never the bare fighter id (playtest 26: "goku Lv90")
    for c in host_save.party.iter().chain(&guest_save.party) {
        assert_ne!(c.name, c.fighter, "raw fighter id as the shown name");
        assert!(c.name.chars().next().is_some_and(|ch| ch.is_uppercase()), "shown name {:?}", c.name);
    }
    let host_hello = DojoHello { save: host_save.clone(), pick: 0, setup: Some(setup) };
    let guest_pick = 0; // Vegeta (the first party member in record order is Trunks 3 - pick by name below)
    let guest_pick = guest_save.party.iter().position(|c| c.fighter == "vegeta").unwrap_or(guest_pick);
    let mk = |role: Role, name: &str, extra: &DojoHello| LobbySettings {
        role,
        name: name.into(),
        content: content.clone(),
        want_players: 2,
        auto_start: false,
        secret: format!("seat-{name}"),
        kind: KIND_DOJO.into(),
        extra: serde_json::to_value(extra).unwrap(),
        base: wf_net::dojo::base_config(seed as u32),
        ..Default::default()
    };
    let mut host = NetLobby::new(Box::new(net.endpoint("host")), mk(Role::Host, "host", &host_hello), "DOJO1", clock.clone());
    host.lobby.set_fighter(fighter_setup(&host_save.party[0], setup.mode, &rules(), &fd));
    host.lobby.set_start_extra(serde_json::to_value(setup).unwrap());
    let guest_hello = DojoHello { save: guest_save.clone(), pick: guest_pick, setup: None };
    let mut guest = NetLobby::new(Box::new(net.endpoint("guest")), mk(Role::Player, "guest", &guest_hello), "DOJO1", clock.clone());
    guest.lobby.set_ready(false);
    // a battle-room peer that ended up in the same room is refused
    let mut odd = mk(Role::Player, "odd", &guest_hello);
    odd.kind = KIND_MATCH.into();
    let mut rogue = NetLobby::new(Box::new(net.endpoint("rogue")), odd, "DOJO1", clock.clone());
    let step = 1000.0 / 59.7275;
    let mut k = 0;
    while k < 20_000 && (host.started().is_none() || guest.started().is_none()) {
        t.set(k as f64 * step);
        rogue.poll();
        host.poll();
        guest.poll();
        // guest: once the host's setup is visible, take it (fighter in its mode), echo it, ready
        if guest.started().is_none() {
            let hs: Option<DojoSetup> = guest
                .lobby
                .peer_extras()
                .iter()
                .filter_map(|(_, v)| serde_json::from_value::<DojoHello>(v.clone()).ok())
                .find_map(|h| h.setup.filter(|_| h.save == host_save));
            if let Some(s) = hs {
                let want = DojoHello { setup: Some(s), ..guest_hello.clone() };
                if guest.lobby.settings.extra != serde_json::to_value(&want).unwrap() {
                    guest.lobby.set_fighter(fighter_setup(&guest_save.party[guest_pick], s.mode, &rules(), &fd));
                    guest.lobby.set_extra(serde_json::to_value(&want).unwrap());
                    guest.lobby.set_ready(true);
                }
            }
        }
        // host: start when the guest confirmed our setup
        if host.started().is_none() {
            let ok = host
                .lobby
                .peer_extras()
                .iter()
                .filter_map(|(_, v)| serde_json::from_value::<DojoHello>(v.clone()).ok())
                .any(|h| h.setup == Some(setup) && h.save == guest_save);
            if ok {
                host.lobby.request_start();
            }
        }
        k += 1;
    }
    let (h, g) = (host.started().cloned().expect("host started"), guest.started().cloned().expect("guest started"));
    (h, g, rogue.lobby.rejected.clone())
}

fn game(d: &Arc<GameData>, st: &StartInfo) -> DojoGame {
    let setup: DojoSetup = serde_json::from_value(st.extra.clone()).expect("setup");
    DojoGame::new(d.clone(), st.config.clone(), setup, rules())
}

/// the confirmed game: a fresh game with the confirmed inputs (per slot) up to state `n`
fn replay(d: &Arc<GameData>, st: &StartInfo, inputs: &[Vec<u16>], n: usize) -> DojoGame {
    let mut g = game(d, st);
    for f in 0..n {
        let inp: Vec<u16> = inputs.iter().map(|v| v.get(f).copied().unwrap_or(0)).collect();
        g.advance(&inp, false);
    }
    g
}

#[test]
fn dojo_match_over_a_bad_network() {
    let d = common::data();
    // (setup, network, seed, scripted: P1 walks into P2 punching while P2 stands - a known winner)
    for (setup, cond, seed, scripted) in [
        (DojoSetup { mode: DojoMode::Balanced, rounds: 3 }, Conditions { latency_ms: 120.0, jitter_ms: 30.0, loss: 0.05 }, 3, true),
        (DojoSetup { mode: DojoMode::InGame, rounds: 1 }, Conditions { latency_ms: 0.0, jitter_ms: 0.0, loss: 0.0 }, 5, false),
        (DojoSetup { mode: DojoMode::InGame, rounds: 5 }, Conditions { latency_ms: 80.0, jitter_ms: 20.0, loss: 0.05 }, 9, false),
        (DojoSetup { mode: DojoMode::InGame, rounds: 3 }, Conditions { latency_ms: 120.0, jitter_ms: 30.0, loss: 0.05 }, 13, true),
    ] {
        let (hs, gs, rogue) = lobby(&d, setup, seed);
        assert_eq!(rogue.as_deref(), Some("this room is a dojo room"), "a battle-room peer is refused");
        assert_eq!(hs.config, gs.config, "both peers build the same match");
        assert_eq!(hs.extra, gs.extra);
        assert_eq!(hs.kind, KIND_DOJO);
        assert_eq!(hs.config.battle.arena, "dojo");
        assert_eq!(hs.config.battle.players[1].fighter, "vegeta");
        if setup.mode == DojoMode::InGame {
            assert_eq!(hs.config.battle.players[1].level, 110, "In-Game: the save's level");
        } else {
            let fx = hs.config.battle.players[0].fixed.clone().expect("Balanced: a fixed record");
            assert_eq!((fx.hp, fx.ep, fx.stats, fx.skills), (rules().balanced_hp, 100, [67, 67, 25, 50], Some(vec![7])), "Balanced: the ROM's record");
        }
        // the match: one rollback session per peer on a (fresh) virtual network
        let net = SimNet::new(cond, seed + 100);
        let cfg = NetConfig { session: hs.session, max_prediction: 12, ..Default::default() };
        let mut ps = testnet::peers(&net, vec![game(&d, &hs), game(&d, &gs)], &cfg, 0, seed);
        if scripted {
            use wf_core::input as b;
            // P1 (slot 0, on the left): walks into P2 and punches (A 2 of every 12 ticks); P2 stands still
            ps[0].script = Some(Box::new(|t: u64| if t % 12 < 2 { b::A } else { b::RIGHT }));
            ps[1].script = Some(Box::new(|_| 0));
        }
        let total = (rules().between_frames + rules().start_frames + rules().round_frames + 10) as i32 * rules().max_rounds as i32;
        let r = testnet::run(&net, &mut ps, &mut [], total, 3_000_000.0);
        assert!(r.done, "not finished");
        for p in &ps {
            assert_eq!(p.sess.hashes[total as usize], ps[0].sess.hashes[total as usize], "{} disagrees", p.id);
            assert!(!p.sess.events.iter().any(|e| matches!(e, NetEvent::Desync { .. })), "{:?}", p.sess.events);
        }
        // the result from the confirmed inputs, the same on both; the prize exactly once to the winner
        let mut saves = [save(&common::field_data(), &[(0, 90)]), save(&common::field_data(), &[(4, 110)])];
        let before = saves.clone();
        let mut ends = Vec::new();
        for (i, p) in ps.iter().enumerate() {
            let n = p.sess.confirmed().min(total) as usize;
            let g = replay(&d, if i == 0 { &hs } else { &gs }, p.sess.known_inputs(), n);
            assert_eq!(g.st.phase, Phase::Over, "the match is over by state {n}");
            let mut m = DojoMatch::new(i as u8);
            for _ in 0..3 {
                m.settle(&g, &rules(), &mut saves[i]);
            }
            m.disconnected(); // a drop after the result changes nothing
            ends.push((g.result().unwrap(), g.st.results.clone(), m.ending.clone().unwrap()));
        }
        assert_eq!(ends[0].0, ends[1].0, "same result on both peers");
        assert_eq!(ends[0].1, ends[1].1, "same rounds");
        let prize = rules().prize.unwrap();
        if scripted {
            assert_eq!(ends[0].0, DojoResult::Win(0), "the puncher wins: {:?}", ends[0].1);
        }
        for i in 0..2 {
            let got = saves[i].story.supplies[prize] as i32 - before[i].story.supplies[prize] as i32;
            match ends[0].0 {
                DojoResult::Win(w) if w as usize == i => {
                    assert_eq!(got, 1, "the winner's prize once");
                    assert_eq!(ends[i].2, Ending::Won { prize: Some(prize) });
                }
                DojoResult::Win(_) => {
                    assert_eq!(got, 0);
                    assert_eq!(ends[i].2, Ending::Lost);
                }
                DojoResult::Draw => {
                    assert_eq!(got, 0);
                    assert_eq!(ends[i].2, Ending::Draw);
                }
            }
        }
        eprintln!(
            "dojo {:?} best of {} lat {} loss {}: rounds {:?} -> {:?}; rollbacks {:?}",
            setup.mode,
            setup.rounds,
            cond.latency_ms,
            cond.loss,
            ends[0].1,
            ends[0].0,
            ps.iter().map(|p| p.sess.stats.rollbacks).collect::<Vec<_>>()
        );
    }
}

#[test]
fn dojo_disconnect_mid_match() {
    let d = common::data();
    let setup = DojoSetup { mode: DojoMode::Balanced, rounds: 3 };
    let (hs, gs, _) = lobby(&d, setup, 21);
    let net = SimNet::new(Conditions { latency_ms: 60.0, jitter_ms: 0.0, loss: 0.05 }, 22);
    let cfg = NetConfig { session: hs.session, disconnect_ms: 1500.0, ..Default::default() };
    let mut ps = testnet::peers(&net, vec![game(&d, &hs), game(&d, &gs)], &cfg, 0, 23);
    ps[1].leave_tick = Some(700);
    let r = testnet::run(&net, &mut ps, &mut [], 1200, 600_000.0);
    assert!(r.done);
    let fd = common::field_data();
    let orig = save(&fd, &[(0, 90)]);
    for (i, p) in ps.iter().enumerate() {
        // the host sees the drop; the leaver knows it left
        let dropped = p.left || p.sess.events.iter().any(|e| matches!(e, NetEvent::Disconnected { .. }));
        assert!(dropped, "{} must see the disconnect", p.id);
        let mut m = DojoMatch::new(i as u8);
        let n = p.sess.confirmed().min(1200) as usize;
        let g = replay(&d, if i == 0 { &hs } else { &gs }, p.sess.known_inputs(), n);
        let mut s = orig.clone();
        if g.result().is_none() {
            assert_eq!(*m.disconnected(), Ending::Disconnected);
        }
        // a late settle after the disconnect does not award anything
        m.settle(&g, &rules(), &mut s);
        assert_eq!(m.ending, Some(Ending::Disconnected));
        assert_eq!(s, orig, "no prize, the save untouched");
    }
}

#[test]
fn dojo_synctest() {
    let d = common::data();
    let fd = common::field_data();
    let a = DojoSave::from_save(0, &save(&fd, &[(0, 90)]), &fd).unwrap();
    let b = DojoSave::from_save(1, &save(&fd, &[(4, 110)]), &fd).unwrap();
    let r = DojoRules { round_frames: 600, between_frames: 30, balanced_hp: 300, max_rounds: 5, ..Default::default() };
    for (mode, bytes) in [(DojoMode::Balanced, false), (DojoMode::InGame, false)] {
        let setup = DojoSetup { mode, rounds: 5 };
        let m = wf_net::dojo::match_config(fighter_setup(&a.party[0], mode, &r, &fd), fighter_setup(&b.party[0], mode, &r, &fd), 77);
        let g = DojoGame::new(d.clone(), m, setup, r);
        let mut st = SyncTestSession::new(g, 8);
        st.bytes = bytes;
        let mut ri: Vec<wf_net::cli::RandomInput> = (0..2).map(|k| wf_net::cli::RandomInput::new(300 + k)).collect();
        let mut over = None;
        for f in 0..4000 {
            let inp: Vec<u16> = ri.iter_mut().map(|x| x.next()).collect();
            if let Err(e) = st.advance(&inp) {
                panic!("{mode:?}: frame {f}: {e}");
            }
            if over.is_none() && st.game.st.phase == Phase::Over {
                over = Some(f);
            }
        }
        eprintln!("dojo synctest {mode:?}: 4000 frames x 8 rollback ok; rounds {:?}, over at {over:?}", st.game.st.results);
        assert!(over.is_some(), "a 5-round match of 600-frame rounds ends within 4000 frames");
    }
}

/// the ROM's rules (engine/docs/netcode.md "Link play: Dojo"): 180 s rounds, 300 updates between, no round cap, no
/// prize; Balanced = the 24-fighter roster of 0x0872BFCC; In-Game = the save's record as is
#[test]
fn dojo_rom_rules() {
    use wf_net::dojo::{balanced_roster, BALANCED_ROSTER};
    let fd = common::field_data();
    let r = DojoRules::default();
    assert_eq!(r, DojoRules { round_frames: 10800, between_frames: 300, start_frames: 125, balanced_hp: 15000, max_rounds: 0, prize: None });
    let roster = balanced_roster();
    assert_eq!(roster.len(), 24);
    for c in &roster {
        let f = fighter_setup(c, DojoMode::Balanced, &r, &fd);
        let fx = f.fixed.expect("fixed");
        assert_eq!((f.fighter.as_str(), f.form), (c.fighter.as_str(), c.form), "{}", c.name);
        assert_eq!((fx.hp, fx.ep, fx.skills), (15000, 100, Some(vec![7])), "{}", c.name);
        let want = if c.name == "Hercule" { [60, 74, 25, 50] } else { [67, 67, 25, 50] };
        assert_eq!(fx.stats, want, "{}", c.name);
    }
    assert_eq!(BALANCED_ROSTER[0].0, "Goku");
    // a save character picked in Balanced Mode: its fighter's first roster entry
    let a = DojoSave::from_save(0, &save(&fd, &[(4, 110)]), &fd).unwrap();
    let f = fighter_setup(&a.party[0], DojoMode::Balanced, &r, &fd);
    assert_eq!((f.fighter.as_str(), f.fixed.unwrap().hp), ("vegeta", 15000));
    // In-Game: the save's own record
    let c = &a.party[0];
    let f = fighter_setup(c, DojoMode::InGame, &r, &fd);
    let fx = f.fixed.expect("fixed");
    assert_eq!((f.level, fx.hp, fx.ep, fx.stats), (110, c.max_hp, c.max_ep, c.stats));
    assert!(c.max_hp > 0 && !c.skills.is_empty());
    assert_eq!(fx.skills.as_ref(), Some(&c.skills));
}

