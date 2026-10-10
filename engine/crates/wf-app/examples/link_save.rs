//! Save slots for link play tests (tools/link/twoproc.sh): headless, no window.
//!
//! `link_save write CONTENT OUT.sav PARTY SUPPLIES ZENIE` - a story save whose party is PARTY (`0:90,4:60` = character
//! index : level), holding SUPPLIES (`0:5,1:4` = supply index : count, or `-`) and ZENIE, standing in Earth 2-1.
//! `link_save read FILE.sav` - the slot's supplies (index: count, non-zero), equipment count and zenie as JSON.

use std::path::PathBuf;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::menu::save::{SaveData, Saves};
use wf_core::story::Story;

fn pairs(s: &str) -> Vec<(usize, i64)> {
    if s == "-" || s.is_empty() {
        return Vec::new();
    }
    s.split(',')
        .filter_map(|p| {
            let (a, b) = p.split_once(':')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("write") if a.len() >= 6 => {
            let packs = Packs::new(vec![Box::new(DirSource(PathBuf::from(&a[1]))) as Box<dyn FileSource>]).expect("content");
            let d = wf_app::battle::load_data(&packs).expect("game data");
            let mut story = Story::new_game(&d);
            for c in story.chars.iter_mut() {
                c.flags &= !1;
            }
            let party = pairs(&a[3]);
            for &(i, lv) in &party {
                let lv = lv as i32;
                story.chars[i].flags |= 1;
                story.chars[i].rec.level = lv;
                story.chars[i].rec.str_ += lv / 4;
                story.chars[i].rec.end += lv / 5;
            }
            story.current = party.first().map(|p| p.0 as i32).unwrap_or(0);
            story.supplies.iter_mut().for_each(|n| *n = 0);
            story.equipment.clear();
            for (i, n) in pairs(&a[4]) {
                story.supplies[i] = n as u8;
            }
            story.zenie = a[5].parse().unwrap_or(0);
            let sd = SaveData { story, world: 2, area: 1, variant: 1, x: 100, y: 100, flag: false };
            let mut s = Saves::default();
            s.set(0, Some(sd));
            let out = PathBuf::from(&a[2]);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).expect("save folder");
            }
            std::fs::write(&out, s.slot_bytes(0)).expect("write the slot");
            println!("wrote {}", out.display());
        }
        Some("read") if a.len() >= 2 => {
            let sd = wf_app::trade::read_slot(&PathBuf::from(&a[1])).expect("a story slot");
            let sup: serde_json::Map<String, serde_json::Value> = sd
                .story
                .supplies
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .map(|(i, &n)| (i.to_string(), serde_json::json!(n)))
                .collect();
            println!("{}", serde_json::json!({ "supplies": sup, "equipment": sd.story.equipment.len(), "zenie": sd.story.zenie }));
        }
        _ => {
            eprintln!("usage: link_save write CONTENT OUT.sav PARTY SUPPLIES ZENIE | link_save read FILE.sav");
            std::process::exit(2);
        }
    }
}
