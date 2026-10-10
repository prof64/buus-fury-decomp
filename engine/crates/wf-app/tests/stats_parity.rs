//! Stats against the game's own routines, called in the emulator (headless; skipped without the ROM).
//!
//! `data/buu/stats_recalc.json`: RecalcStats (0x08013C66) on Goku's record for every equipment item alone (as it is,
//! at base stats 3 - the clamp at 0 -, at 1 HP), the sets, stacked effects, the low-HP item's limits and timed boosts:
//! the inputs and what the game wrote into the record (+0x4C.. and +0x13). Ours: `progression::recalc_stats_with`.

use std::path::PathBuf;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::progression::{recalc_stats_with, Record};

fn content_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return Some(PathBuf::from(d));
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-link-content");
    if wf_extract::cache::fresh(&dir, &[]) {
        return Some(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    for (p, d) in &c.files {
        let rel = p.strip_prefix("base/").unwrap_or(p);
        let f = dir.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, d).unwrap();
    }
    std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
    Some(dir)
}

#[test]
fn recalc_stats_as_the_game() {
    let Some(dir) = content_dir() else {
        eprintln!("no ROM: skipped");
        return;
    };
    let packs = Packs::new(vec![Box::new(DirSource(dir)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(wf_rom::testutil::repo_root().join("data/buu/stats_recalc.json")).unwrap()).unwrap();
    let i = |v: &serde_json::Value| v.as_i64().unwrap() as i32;
    let mut bad = vec![];
    let cases = doc["cases"].as_array().unwrap();
    for c in cases {
        let st: Vec<i32> = c["stats"].as_array().unwrap().iter().map(i).collect();
        let rec = Record { level: 60, xp: 0, hp: i(&c["hp"]), max_hp: i(&c["max_hp"]), ep: 0, max_ep: 0, str_: st[0], pow: st[1], end: st[2], spd: st[3], points: 0 };
        let slots: Vec<i32> = c["slots"].as_array().unwrap().iter().map(i).collect();
        let eq: Vec<Option<&wf_core::content::Equip>> = slots.iter().map(|&id| data.equipment.iter().find(|e| e.id == id)).collect();
        let boost = c["boost"].as_array().map(|b| [i(&b[0]), i(&b[1]), i(&b[2]), i(&b[3])]);
        let e = recalc_stats_with(&rec, &eq, boost);
        let ours = [e.str_, e.pow, e.end, e.end2, e.spd, e.ep_cost_factor, e.ep_block_factor, e.xp_mult, e.thorns, e.block_factor, e.ss_cooldown_rate, e.hp_regen256, e.crit_pct];
        let keys = ["str", "pow", "end", "end2", "spd", "ep_cost", "ep_block", "xp", "thorns", "block", "ss", "regen", "crit"];
        let game: Vec<i32> = keys.iter().map(|k| i(&c[*k])).collect();
        if ours[..] != game[..] {
            let d: Vec<String> = keys.iter().zip(ours.iter().zip(&game)).filter(|(_, (a, b))| a != b).map(|(k, (a, b))| format!("{k} ours {a} game {b}")).collect();
            bad.push(format!("slots {slots:?} stats {st:?} hp {}/{} boost {boost:?}: {}", rec.hp, rec.max_hp, d.join(", ")));
        }
    }
    eprintln!("RecalcStats: {} cases, {} differ", cases.len(), bad.len());
    for b in bad.iter().take(60) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty(), "{} of {} cases differ", bad.len(), cases.len());
}

/// the game's XP routines called in the emulator (re/buu/xp_routine.py -> data/buu/xp_routine.json): a script's
/// give_xp adds its amount as it is whatever the XP factor, nothing at 0 HP or for a form without the XP flag; LevelUp
/// over every main character's form (one / several levels, the rolls, the stat points)
#[test]
fn xp_routine_matches_the_game() {
    let Some(dir) = content_dir() else {
        eprintln!("no ROM: skipped");
        return;
    };
    let packs = Packs::new(vec![Box::new(DirSource(dir)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(wf_rom::testutil::repo_root().join("data/buu/xp_routine.json")).unwrap()).unwrap();
    let i = |v: &serde_json::Value| v.as_i64().unwrap();
    let mut bad = vec![];
    let ups = doc["level_up"].as_array().unwrap();
    for c in ups {
        let form = data.form(i(&c["form"]) as i32).expect("form");
        let growth = data.chars.iter().find(|x| x.index == form.char_).expect("character").growth;
        let mut rec = Record { level: i(&c["old"]) as i32, xp: 0, hp: 1, max_hp: i(&c["max_hp"]) as i32, ep: 1, max_ep: i(&c["max_ep"]) as i32, str_: 0, pow: 0, end: 0, spd: 0, points: i(&c["points"]) as i32 };
        let mut rng = wf_core::rng::Rng::new(i(&c["rng"][0]) as u32, i(&c["rng"][1]) as u32);
        wf_core::progression::level_up(&mut rec, &growth, i(&c["old"]) as i32, i(&c["new"]) as i32, &mut rng);
        let a = &c["after"];
        let ours = (rec.hp as i64, rec.max_hp as i64, rec.ep as i64, rec.max_ep as i64, rec.points as i64);
        let game = (i(&a["hp"]), i(&a["max_hp"]), i(&a["ep"]), i(&a["max_ep"]), i(&a["points"]));
        if ours != game {
            bad.push(format!("form {} {} -> {} from {}/{} points {}: ours {ours:?} game {game:?}", c["form"], c["old"], c["new"], c["max_hp"], c["max_ep"], c["points"]));
        }
    }
    // the XP cases on a battle's fighter wearing the case's item: the script's give_xp (`player::gain_xp_raw`) adds
    // the amount as it is, a kill's goes through the record's XP factor
    for c in doc["xp"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let Some(item) = name.strip_prefix("native item ").and_then(|x| x.parse::<i32>().ok()) else { continue };
        let mut f = wf_core::battle::FighterSetup::default();
        if let Some(e) = data.equipment.iter().find(|e| e.id == item && item != 0) {
            let slot = ["body", "hands", "feet", "accessory"].iter().position(|s| *s == e.slot.as_str()).expect("slot");
            f.equip[slot] = item;
        }
        let cfg = wf_core::battle::BattleConfig { arena: "forest".into(), players: vec![f], phases: Vec::new(), rng: wf_core::battle::RngInit::Seed(1), xp: true, spawn_near: None, arena_rules: false, even_level: None };
        let mut w = wf_core::battle::create(data.clone(), &cfg);
        let pi = w.player_idx(0).expect("player");
        let mult = w.pl_ref(pi).eff.xp_mult as i64;
        assert_eq!(mult, i(&c["mult"]), "{name}: the XP factor");
        let x0 = w.pl_ref(pi).rec.xp;
        let amount = i(&c["xp"]);
        wf_core::player::gain_xp_raw(&mut w, pi, amount);
        let raw = w.pl_ref(pi).rec.xp - x0;
        if raw != i(&c["xp_after"]) - i(&c["xp_before"]) {
            bad.push(format!("{name}: give_xp {amount} adds {raw}, the game {}", i(&c["xp_after"]) - i(&c["xp_before"])));
        }
        let x1 = w.pl_ref(pi).rec.xp;
        wf_core::player::gain_xp(&mut w, pi, amount);
        assert_eq!(w.pl_ref(pi).rec.xp - x1, (amount * mult + 255) >> 8, "{name}: a kill's XP");
    }
    eprintln!("LevelUp: {} cases, {} differ", ups.len(), bad.len());
    for b in bad.iter().take(40) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty(), "{} of {} cases differ", bad.len(), ups.len());
}
