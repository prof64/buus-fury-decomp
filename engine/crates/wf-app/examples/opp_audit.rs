//! Opponent visual audit - the comparer (headless): the emulator's per-frame record of an enemy scenario
//! (tools/battle/lockstep/visual.py: a lockstep trace + `vis` + the screen of every frame) against our engine's battle
//! sim on the same arena (8-3, arena rules OFF - the faithful core), frame by frame.
//!
//!   cargo run --release -p wf-app --example opp_audit -- work/ub/oa/oa_t44_*.json
//!
//! Per scenario one line: frames compared; the first diverging frame of each category -
//!   player (the lockstep row: state / anim / position / HP / EP / sounds / RNG), dmg (every ComputeDamage), enemy
//!   (the scenario's enemy: x / y / facing / HP), anim (its raw / frame / tick), ai (its AI state), others (the other
//!   live enemies: parts / summons - count, type, position, anim), pix (the picture: OBJ pixels; bg = a background
//!   pixel differs)
//! and the count of frames whose OBJ pixels differ. `OA_OUT=dir` (default work/ub/oa/out): NAME.json (per frame the
//! differing fields, emulator vs ours) and NAME_fNNN.png triptychs (game | ours | diff) of the first `OA_PNG` (6)
//! frames with OBJ pixel differences. `OA_PACK=dir` also writes the gate's data (`data/audits/opp/NAME.json.z`: the
//! trace, the fitted tile clock and the hash of every game picture - no pictures; tests/audits.rs). The comparer is
//! `tests/audit/opp.rs` (shared with the gate). Usage and the scenario format: tools/battle/LOCKSTEP.md, "Visual audit".

#[path = "../tests/audit/opp.rs"]
mod opp;
#[path = "../tests/audit/pack.rs"]
mod pack;

use std::sync::Arc;

fn main() {
    use wf_app::wf_content::{DirSource, FileSource, Packs};
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = wf_rom::testutil::repo_root();
    let out_dir = std::env::var("OA_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| repo.join("work/ub/oa/out"));
    let max_png: usize = std::env::var("OA_PNG").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    let pack = std::env::var("OA_PACK").ok().map(std::path::PathBuf::from);
    std::fs::create_dir_all(&out_dir).unwrap();
    if let Some(p) = &pack {
        std::fs::create_dir_all(p).unwrap();
    }
    // the segment harness's shared extraction (as examples/pixels.rs)
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let d = wf_app::battle::load_data(&packs).expect("data");
    let _ = opp::DATA.set(Arc::new((*d).clone()));
    let mut tile_hint = std::env::var("OA_TILE").ok().and_then(|v| v.parse().ok());
    for a in &args {
        wf_core::coverage::context(std::path::Path::new(a).file_stem().and_then(|s| s.to_str()).unwrap_or("?"));
        let path = std::path::Path::new(a);
        let mut t: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let name = t["name"].as_str().unwrap_or("?").to_string();
        let Some(game) = opp::read_fb(&path.with_extension("fb")) else {
            println!("{name}: no screen record (.fb)");
            continue;
        };
        let hashes: String = game.iter().map(|g| format!("{:016x}", opp::hash(g))).collect();
        let r = opp::audit(&t, &opp::Pictures::Game(game), &packs, Some((&out_dir, max_png)), &mut tile_hint);
        println!("{}", r.summary);
        if let Some(p) = &pack {
            let torn: serde_json::Map<String, serde_json::Value> = r.torn.iter().map(|(f, s)| (f.to_string(), serde_json::json!(s))).collect();
            t["pic"] = serde_json::json!({"tile_off": r.tile_off, "hash": hashes, "torn": torn});
            let z = miniz_oxide::deflate::compress_to_vec_zlib(&serde_json::to_vec(&t).unwrap(), 9);
            std::fs::write(p.join(format!("{name}.json.z")), z).unwrap();
        }
    }
}
