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
