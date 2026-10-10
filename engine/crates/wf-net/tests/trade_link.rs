//! The Trading Post against the real game's link trade: two emulators wired by the link cable (tools/link/
//! trade_record.py), both on run 2's save; the recording (data/link/trade_*.json) holds each side's offer as the trade
//! screen held it at the proposal and game 1's tradable part of the save before and after "TRADE COMPLETE!" (supply
//! counts, the equipment list's words - item id, the wearer in the high nibble -, zenie, the characters' slots).
//! Ours: `Inventory::after` on the before with the two offers must give the recorded after, for both sides; and a
//! worn copy that left takes its wearer's slot back to "-None-" (as `wf_app::trade::apply`). Needs the ROM like the
//! other content tests; headless.
mod common;

use wf_net::trade::{Inventory, Offer};

/// the "-None-" item of each equipment slot
const NONE_ITEM: [u32; 4] = [0, 44, 77, 112];

fn inventory(side: &serde_json::Value, story: &[bool]) -> Inventory {
    let supplies: Vec<u32> = side["supplies"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    Inventory {
        story: (0..supplies.len()).map(|i| story.get(i).copied().unwrap_or(false)).collect(),
        supplies,
        equipment: side["equipment"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).map(|w| ((w & 0x0FFF_FFFF) as i32, w >> 28 != 0)).collect(),
        zenie: side["zenie"].as_u64().unwrap() as u32,
    }
}

fn offer(entries: &serde_json::Value) -> Offer {
    let mut o = Offer::default();
    for e in entries.as_array().unwrap() {
        let (n, kind, id) = (e[0].as_u64().unwrap() as u32, e[1].as_u64().unwrap(), e[2].as_u64().unwrap());
        match kind {
            0 => o.equipment.extend(std::iter::repeat_n(id as i32, n as usize)),
            1 => *o.supplies.entry(id as u16).or_default() += n,
            _ => o.zenie += n,
        }
    }
    o
}

#[test]
fn trading_post_matches_the_game() {
    let d = common::field_data();
    let story = d.menu.as_ref().map(|m| m.inv.story.clone()).unwrap_or_default();
    let dir = common::repo().join("data/link");
    let mut n = 0;
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("trade_")).collect();
    names.sort();
    for p in names {
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        let offers = [offer(&doc["offers"][0]), offer(&doc["offers"][1])];
        for s in 0..2 {
            let before = &doc["before"][s];
            let after = &doc["after"][s];
            let inv = inventory(before, &story);
            let ours = inv.after(&offers[s], &offers[1 - s]).unwrap_or_else(|e| panic!("{}: side {s}: refused: {e}", p.display()));
            let game = inventory(after, &story);
            assert_eq!(ours, game, "{}: side {s}: the save after the trade", p.display());
            // a worn copy that left: its wearer's slot is "-None-" now (remove equipment `0x08004468`)
            let words = |v: &serde_json::Value| -> Vec<u32> { v["equipment"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u32).collect() };
            let slots = |v: &serde_json::Value, c: usize, k: usize| v["worn"][c][k].as_u64().unwrap() as u32;
            let (w0, w1) = (words(before), words(after));
            for w in w0.iter().filter(|&&w| w >> 28 != 0) {
                let left = w0.iter().filter(|&&x| x == *w).count() > w1.iter().filter(|&&x| x == *w).count();
                let (id, c) = (w & 0x0FFF_FFFF, (w >> 28) as usize - 1);
                for k in 0..4 {
                    if slots(before, c, k) == id {
                        let want = if left { NONE_ITEM[k] } else { id };
                        assert_eq!(slots(after, c, k), want, "{}: side {s}: character {c}'s slot {k}", p.display());
                    }
                }
            }
        }
        eprintln!("{}: both sides exact ({:?} for {:?})", p.file_name().unwrap().to_string_lossy(), offers[0], offers[1]);
        n += 1;
    }
    assert!(n > 0, "no recordings in {}", dir.display());
}
