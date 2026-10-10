//! World extraction checks against facts verified in the emulator (re/buu/world/verify_objects.py, t_dialog.py).
//! Skipped when the ROM is absent (`$WF_ROM` or `<repo>/roms/buu.gba`).

use serde_json::Value;
use wf_rom::content::Content;

fn extract() -> Option<Content> {
    let rom = wf_rom::testutil::rom()?;
    let mut out = Content::new();
    wf_world::extract(&rom, &mut out).unwrap();
    Some(out)
}

fn area_objects(out: &Content, key: &str) -> Vec<Value> {
    let areas = out.json("world/areas.json").unwrap();
    let a = areas["areas"].as_array().unwrap().iter().find(|a| a["key"] == key).unwrap().clone();
    let map = a["maps"][0].as_str().unwrap();
    let o = out.json(&format!("maps/{map}/objects.json")).unwrap();
    o["areas"].as_array().unwrap().iter().find(|a| a["key"] == key).unwrap()["objects"].as_array().unwrap().clone()
}

#[test]
fn text_banks_fully_referenced_and_lines_match_runtime() {
    let Some(out) = extract() else { return };
    let d = out.json("text/dialog.json").unwrap();
    // all 7 banks (196 386 UTF-16 units) are reached from event nodes
    assert_eq!(d["counts"]["bank_units"], 196386);
    assert_eq!(d["counts"]["bank_units_referenced"], 196386);
    let lines = d["lines"].as_array().unwrap();
    assert!(lines.len() >= 2700, "{}", lines.len());
    let find = |t: &str| lines.iter().find(|l| l["plain"] == t).cloned();
    // the Snake Way text trigger (seen on screen and in EWRAM in buu_snakeway_start.ss)
    let l = find("It'll take me too long to cross Snake Way if I just walk!").expect("snakeway line");
    assert_eq!(l["speaker"]["kind"], "player");
    let l = find("Make Goku run by double-tapping the\n\n+ Control Pad.").expect("tutorial line");
    assert_eq!(l["text"], "Make Goku run by double-tapping the\n\n{color 7}+ Control Pad{color 2}.");
    assert_eq!(l["speaker"]["kind"], "none");
    // save point menu (choice prompt)
    assert!(lines.iter().any(|l| l["kind"] == "bank" && l["plain"] == "-Save\n-Switch Characters"));
}

#[test]
fn render_encode_round_trip() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let b = wf_world::text::Banks::load(&rom);
    for (_, u) in &b.banks {
        assert_eq!(&wf_world::text::encode(&wf_world::text::render(u)), u);
    }
}

#[test]
fn objects_match_emulator() {
    let Some(out) = extract() else { return };
    // Southwest Forest 8-3: the three live map exits (rects read from the trigger objects in RAM)
    let m = out.json("maps/area_08_003/objects.json").unwrap();
    let ex: Vec<(i64, i64, i64, i64, String)> = m["map_triggers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let r = &t["rect"];
            (r["x"].as_i64().unwrap(), r["y"].as_i64().unwrap(), r["w"].as_i64().unwrap(), r["h"].as_i64().unwrap(),
             t["to"]["key"].as_str().unwrap().to_string())
        })
        .collect();
    assert_eq!(
        ex,
        vec![(64, 0, 49, 17, "8-6".into()), (408, 536, 49, 17, "8-2".into()), (0, 248, 9, 57, "8-4".into())]
    );
    // 8-3: 12 enemies + 2 breakables
    let objs = area_objects(&out, "8-3");
    assert_eq!(objs.iter().filter(|o| o["type"] == "enemy").count(), 12);
    assert_eq!(objs.iter().filter(|o| o["type"] == "breakable").count(), 2);
    // Snakeway 1-1: the text trigger at (361,153)-(384,232) (object 0x0203DFA4 in buu_snakeway_start.ss)
    let objs = area_objects(&out, "1-1");
    let t = objs.iter().find(|o| o["type"] == "event_trigger").unwrap();
    assert_eq!(t["rect"], serde_json::json!({"x": 361, "y": 153, "w": 23, "h": 79}));
    // Yemma's Office 1-3: standing NPCs exactly where the game puts them
    let objs = area_objects(&out, "1-3");
    for p in [[184, 400], [280, 400], [112, 240], [192, 280]] {
        assert!(objs.iter().any(|o| o["type"] == "npc" && o["pos"] == serde_json::json!(p)), "{p:?}");
    }
    // Z-Mart shop inventory from its event's shop_add_item calls
    let objs = area_objects(&out, "0-93");
    let shop = objs.iter().find_map(|o| o.get("shop")).unwrap();
    assert_eq!(shop["items"].as_array().unwrap().len(), 22);
}

#[test]
fn graph_targets_exist() {
    let Some(out) = extract() else { return };
    let g = out.json("world/graph.json").unwrap();
    let keys: std::collections::HashSet<&str> =
        g["nodes"].as_array().unwrap().iter().map(|n| n["key"].as_str().unwrap()).collect();
    let edges = g["edges"].as_array().unwrap();
    assert!(edges.len() > 900);
    for e in edges {
        assert!(keys.contains(e["to"].as_str().unwrap()), "{e}");
    }
}
