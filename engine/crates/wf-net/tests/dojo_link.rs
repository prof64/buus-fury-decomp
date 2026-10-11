//! The Dojo against the real game's link play: two emulators wired by the link cable (tools/link/dojo_record.py -
//! mGBA's lockstep coordinator in the tools/emu shim), Balanced Mode, Goku against Goku, a fight on scripted keys;
//! the recording (data/link/dojo_*.json) holds per logic frame the keys each fighter's update read, the generator and
//! both fighters (position, facing, animation, HP / EP). Ours: the Dojo match (`DojoGame`, the app's) advanced with
//! those keys from the fighters' creation (the generator's zero), compared every frame: both fighters, the generator
//! and the round timer. Headless, muted;
//! needs the ROM like the other content tests. `DOJO_VERBOSE=1` prints the first differences.
mod common;

use wf_net::dojo::{balanced_roster, DojoChar, fighter_setup, match_config, DojoGame, DojoMode, DojoRules, DojoSetup, Phase};
use wf_net::game::Game;

/// a fighter's record as the game made it (obj+0x240, 0x68 bytes, hex: `wf_import::import_story`'s layout) as the
/// Dojo character a save's party gives
fn rec_char(hex: &str, d: &wf_core::content::GameData) -> DojoChar {
    let r: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap()).collect();
    let u16le = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]) as i32;
    let u32le = |o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let form = r[0x14] as i32;
    let character = d.form(form).map(|f| f.char_).unwrap_or(0);
    let f = d.fighters.iter().find(|f| f.character == character && f.fusion.is_none()).expect("fighter");
    let n = (r[0x0C] as usize).min(16);
    DojoChar {
        name: f.name.clone(),
        character,
        fighter: f.id.clone(),
        form,
        level: wf_core::progression::level_of(u32le(0x24) as i64, &d.xp_table),
        stats: [r[0x28] as i32, r[0x29] as i32, r[0x2A] as i32, r[0x2B] as i32],
        equip: [0, 1, 2, 3].map(|k| (u32le(0x3C + 4 * k) & 0x0FFF_FFFF) as i32),
        max_hp: u16le(0x18),
        max_ep: u16le(0x1C),
        skills: (0..n).map(|k| r[0x0D + k] as i32).collect(),
        sel: r[0x15] as usize,
        skill_levels: (0..n).map(|k| r[0x2E + 2 * k] as i32).collect(),
    }
}

fn check(name: &str) -> (usize, usize, Option<String>) {
    let p = common::repo().join(format!("data/link/dojo_{name}.json"));
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    // (from the fighters' creation on: the rows before it are the setup's last frames and the load)
    // (`lost`: the host's last fight logic frame of a recording whose guest was reset - the rows after it are the link
    // error's script, not the fight: wf-app tests/dojo_pixels.rs compares their pictures)
    let lost = doc["lost"].as_i64().unwrap_or(i64::MAX);
    let rows: Vec<&serde_json::Value> = doc["rows"]
        .as_array()
        .unwrap()
        .iter()
        .skip_while(|r| r["p"].as_array().is_none_or(|p| p.is_empty()))
        // (its own row is read while the host waits in the middle of the next update: the fighters a tick on)
        .take_while(|r| r["lf"].as_i64().unwrap_or(0) < lost)
        .collect();
    let fd = common::field_data();
    let r = DojoRules::default();
    let pick = |k: usize| doc["pick"].get(k).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    let roster = balanced_roster();
    let rounds = doc["rounds"].as_u64().unwrap_or(3) as u8;
    // In-Game Mode: each side's fighter is its save's character as recorded (the record the game made, `recs`)
    let mode = if doc["mode"].as_str() == Some("ingame") { DojoMode::InGame } else { DojoMode::Balanced };
    let ch = |k: usize| match mode {
        DojoMode::InGame => rec_char(doc["recs"][k].as_str().expect("an In-Game recording keeps the records"), &fd),
        DojoMode::Balanced => roster[pick(k)].clone(),
    };
    let cfg = match_config(fighter_setup(&ch(0), mode, &r, &fd), fighter_setup(&ch(1), mode, &r, &fd), 0);
    let setup = DojoSetup { mode, rounds };
    let mut g = DojoGame::new(fd.clone(), cfg, setup, r);
    let v = |x: &serde_json::Value| x.as_i64().unwrap();
    let verbose = std::env::var("DOJO_VERBOSE").is_ok();
    let mut first = None;
    let mut matched = 0;
    for (k, row) in rows.iter().enumerate() {
        // (each row a completed logic frame: the recorder writes a frame once the next has begun - a link stall
        // leaves no half-updated row)
        let keys: Vec<u16> = row["keys"].as_array().unwrap().iter().map(|x| v(x) as u16).collect();
        // (logic frames with no row of their own - a load's frame counted between two video frames: the next round's
        // fighters made while the screen was black)
        if k > 0 {
            let gap = v(&row["lf"]) - v(&rows[k - 1]["lf"]);
            for _ in 1..gap {
                g.advance(&keys, true);
            }
        }
        let tr = std::env::var("DOJO_RNG").ok().and_then(|f| f.parse::<i64>().ok()) == Some(v(&row["lf"])) && name == std::env::var("DOJO_NAME").unwrap_or_default();
        wf_core::rng::TRACE.store(tr, std::sync::atomic::Ordering::Relaxed);
        // (row 0 is the fighters' creation frame: the match as `new` made it)
        if k > 0 {
            g.advance(&keys, true);
        }
        wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
        let w = &g.s.world;
        if std::env::var("DOJO_COINS").ok().and_then(|f| f.parse::<i64>().ok()).is_some_and(|f| (f..f + 3).contains(&v(&row["lf"]))) {
            let cs: Vec<String> = w.ents.iter().filter(|e| !e.removed).filter_map(|e| match &e.body {
                wf_core::world::Body::Pickup(p) => Some(format!("{:?} x {} y {} t {} mv {}", p.bx, p.x >> 8, p.y >> 8, p.t, p.mv)),
                _ => None,
            }).collect();
            eprintln!("lf {} coins {:?} game {} item-get {:?}", row["lf"], cs, row["coins"], w.link_get.as_ref().map(|g| g.left));
        }
        let ours: Vec<[i64; 9]> = w
            .players
            .iter()
            .map(|&id| {
                let p = w.pl_ref(w.find(id).unwrap());
                // (the animation by its first ROM slot, as the recorder names it: two slots of a sprite can hold one
                // animation - Gotenks' punches 5 / 6)
                let raw = p.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|s| s.anim_raw(p.ap.raw)).map(|a| a.canon_raw).unwrap_or(p.ap.raw);
                [p.a.x as i64, p.a.y as i64, p.a.facing as i64, raw as i64, p.ap.frame as i64, p.ap.tick as i64, p.hp() as i64, p.ep() as i64, p.regen_timer as i64]
            })
            .collect();
        let game: Vec<Vec<i64>> = row["p"].as_array().unwrap().iter().map(|f| f.as_array().unwrap().iter().map(v).collect()).collect();
        let mut diffs = vec![];
        // (no fighters in the game's row: the frame the next round's are made)
        for slot in 0..game.len().min(2) {
            for (j, n) in ["x", "y", "dir", "raw", "frame", "tick", "hp", "ep", "regen"].iter().enumerate() {
                if ours[slot][j] != game[slot][j] {
                    diffs.push(format!("p{slot}.{n} {} vs {}", ours[slot][j], game[slot][j]));
                }
            }
        }
        let rng = w.rng.state();
        let grng = [v(&row["rng"][0]) as u32, v(&row["rng"][1]) as u32];
        // (a row without fighters is the next round's load, caught before its draws)
        if rng != grng && !game.is_empty() {
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
        if let Some(z) = row["zenie"].get(0).and_then(|z| z.as_i64()) {
            if g.st.zenie as i64 != z {
                diffs.push(format!("zenie {} vs {z}", g.st.zenie));
            }
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
        if over && first.is_none() {
            eprintln!("  dojo_{name}: the match's result at row {k} (lf {}): wins {:?}", row["lf"], g.st.wins);
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
    // every recording, exact to its end (or to the match's end)
    let mut names: Vec<String> = std::fs::read_dir(common::repo().join("data/link"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("dojo_")?.strip_suffix(".json").map(String::from))
        .collect();
    names.sort();
    // (`DOJO_ONLY=name`: that recording alone)
    if let Ok(o) = std::env::var("DOJO_ONLY") {
        names.retain(|n| *n == o);
    }
    let mut bad = vec![];
    for name in &names {
        let (m, n, first) = check(name);
        eprintln!("dojo_{name}: {m} / {n} logic frames exact; first difference {first:?}");
        // (`expect` in the recording: a known difference from that row on, written up in netcode.md)
        let p = common::repo().join(format!("data/link/dojo_{name}.json"));
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        let want = doc["expect"].as_u64().map(|e| e as usize).unwrap_or(n);
        if m < want {
            bad.push(name.clone());
        }
    }
    assert!(bad.is_empty(), "not exact: {bad:?}");
}
