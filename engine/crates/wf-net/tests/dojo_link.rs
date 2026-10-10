//! The Dojo against the real game's link play: two emulators wired by the link cable (tools/link/dojo_record.py -
//! mGBA's lockstep coordinator in the tools/emu shim), Balanced Mode, Goku against Goku, a fight on scripted keys;
//! the recording (data/link/dojo_*.json) holds per logic frame the keys each fighter's update read, the generator and
//! both fighters (position, facing, animation, HP / EP). Ours: the Dojo match (`DojoGame`, the app's) advanced with
//! those keys from the fighters' creation (the generator's zero), compared every frame: both fighters, the generator
//! and the round timer. Headless, muted;
//! needs the ROM like the other content tests. `DOJO_VERBOSE=1` prints the first differences.
mod common;

use wf_net::dojo::{balanced_roster, fighter_setup, match_config, DojoGame, DojoMode, DojoRules, DojoSetup, Phase};
use wf_net::game::Game;

fn check(name: &str) -> (usize, usize, Option<String>) {
    let p = common::repo().join(format!("data/link/dojo_{name}.json"));
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    let rows = doc["rows"].as_array().unwrap();
    let fd = common::field_data();
    let r = DojoRules::default();
    let goku = &balanced_roster()[0];
    let cfg = match_config(fighter_setup(goku, DojoMode::Balanced, &r, &fd), fighter_setup(goku, DojoMode::Balanced, &r, &fd), 0);
    let setup = DojoSetup { mode: DojoMode::Balanced, rounds: 1 };
    let mut g = DojoGame::new(fd.clone(), cfg, setup, r);
    let v = |x: &serde_json::Value| x.as_i64().unwrap();
    let verbose = std::env::var("DOJO_VERBOSE").is_ok();
    let mut first = None;
    let mut matched = 0;
    for (k, row) in rows.iter().enumerate() {
        // (row 0: the fighters after their creation frame - the game's first update, ours the match's first advance)
        let keys: Vec<u16> = row["keys"].as_array().unwrap().iter().map(|x| v(x) as u16).collect();
        g.advance(&keys, true);
        let w = &g.s.world;
        let ours: Vec<[i64; 9]> = w
            .players
            .iter()
            .map(|&id| {
                let p = w.pl_ref(w.find(id).unwrap());
                [p.a.x as i64, p.a.y as i64, p.a.facing as i64, p.ap.raw as i64, p.ap.frame as i64, p.ap.tick as i64, p.hp() as i64, p.ep() as i64, p.regen_timer as i64]
            })
            .collect();
        let game: Vec<Vec<i64>> = row["p"].as_array().unwrap().iter().map(|f| f.as_array().unwrap().iter().map(v).collect()).collect();
        let mut diffs = vec![];
        for slot in 0..2 {
            for (j, n) in ["x", "y", "dir", "raw", "frame", "tick", "hp", "ep", "regen"].iter().enumerate() {
                if ours[slot][j] != game[slot][j] {
                    diffs.push(format!("p{slot}.{n} {} vs {}", ours[slot][j], game[slot][j]));
                }
            }
        }
        let rng = w.rng.state();
        let grng = [v(&row["rng"][0]) as u32, v(&row["rng"][1]) as u32];
        if rng != grng {
            diffs.push(format!("rng {rng:08x?} vs {grng:08x?}"));
        }
        // the round timer's seconds (0 while "Round N" shows)
        let timer = match g.st.phase {
            Phase::Fight => 180 - (g.st.t / 60) as i64,
            _ => 0,
        };
        if timer != v(&row["timer"]) && g.st.phase == Phase::Fight {
            diffs.push(format!("timer {timer} vs {}", row["timer"]));
        }
        let gw = [v(&row["wins"][0]) as u8, v(&row["wins"][1]) as u8];
        if g.st.wins != gw {
            diffs.push(format!("wins {:?} vs {gw:?}", g.st.wins));
        }
        // (the match is over at the round's result: the game shows its ending, ours stops)
        let over = g.st.phase == Phase::Over;
        if diffs.is_empty() {
            if first.is_none() {
                matched = k + 1;
            }
        } else if first.is_none() {
            first = Some(format!("row {k} (lf {}): {}", row["lf"], diffs.join(", ")));
            if !verbose {
                break;
            }
        }
        if verbose && !diffs.is_empty() && k < matched + 5 {
            eprintln!("row {k}: {}", diffs.join(", "));
        }
        if over && diffs.is_empty() {
            eprintln!("  dojo_{name}: the round's result at row {k} (lf {}): wins {:?}", row["lf"], g.st.wins);
            return (k + 1, k + 1, first);
        }
    }
    (matched, rows.len(), first)
}

#[test]
fn dojo_link_fight_matches_the_game() {
    if !common::repo().join("roms/buu.gba").exists() && std::env::var("WF_CONTENT").is_err() {
        eprintln!("no ROM: skipped");
        return;
    }
    for (name, want) in [("fight1", 2529usize), ("fight2", 5535)] {
        let (m, n, first) = check(name);
        eprintln!("dojo_{name}: {m} / {n} logic frames exact; first difference {first:?}");
        assert!(m >= want, "dojo_{name}: {m} < {want}");
    }
}
