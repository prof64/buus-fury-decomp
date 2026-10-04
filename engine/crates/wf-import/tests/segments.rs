//! Segments (`data/segments/*.json`, recorder `tools/segments/segment.py`, spec `engine/docs/segments.md`): each
//! starts at a real-game emulator state, is imported into wf-core (`wf_import::import_emulator_state`) and replayed
//! with the recording's input; the verdict per segment is the first divergence.
//!
//! `SEGMENT=name[,name]` runs only those; `SEGMENT_VERBOSE=N` / `SEGMENT_TRACE=a..b` (wf_import::segment);
//! A segment with `"expect": {"matched": N}` in its file is a gate (N video frames must match continuously); one
//! without it only reports (chapter work in progress). `SEGMENT_STRICT=0` never fails, `SEGMENT_STRICT=all` holds
//! every segment to its whole recording. `SEGMENT_DIR` = another directory of segments.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use wf_core::content::{DirSource, GameData};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn content_dir() -> PathBuf {
    if let Ok(d) = std::env::var("WF_FIELD_CONTENT") {
        return PathBuf::from(d);
    }
    // the field-test extraction shared by the test crates; `wf_extract::cache` re-extracts it when the extractor's
    // source hash in its `.complete` marker differs (no stale content after an extractor change)
    // + `ui`: the menu screens (Switch Characters, the title after the credits)
    // + `audio`, `portraits`: the sound driver runs inside the world for the menus' redraw timing (wf-core sndsim.rs)
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let shared = repo().join("engine/target/tmp/wf-import-segment-content");
    if wf_extract::cache::fresh(&shared, &only) {
        return shared;
    }
    wf_extract::cache::ensure(&shared, &repo().join("roms/buu.gba"), &only)
}

fn setup() -> Option<(Arc<GameData>, BTreeMap<String, String>)> {
    let rom = wf_rom::testutil::rom()?;
    let font = wf_ui::font::Font::load(&rom, "dialog", 0x0806_EE6C);
    wf_script_core::text::set_dialog_font((0..256u16).map(|c| font.advance(c)).collect());
    let data = Arc::new(GameData::load(&DirSource { root: content_dir() }).expect("content"));
    // the step bytecode (an event imported mid-node: the natives its interpreter has run)
    if let Ok(b) = std::fs::read(content_dir().join("scripts/bytecode.json")) {
        wf_import::host::set_step_code(&b);
    }
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, errs) = wf_script::export::export(&rom, &set);
    assert!(errs.is_empty(), "decompile errors: {errs:?}");
    Some((data, files))
}

#[test]
fn segments_match_the_game() {
    // SEGMENT_DIR: another directory of segments (e.g. long local proof runs under work/)
    let dir = std::env::var("SEGMENT_DIR").map(PathBuf::from).unwrap_or_else(|_| repo().join("data/segments"));
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    if let Ok(only) = std::env::var("SEGMENT") {
        let names: Vec<&str> = only.split(',').collect();
        paths.retain(|p| names.iter().any(|n| p.file_stem().is_some_and(|s| s == *n)));
    }
    if paths.is_empty() {
        return;
    }
    let Some((data, files)) = setup() else { return };
    let strict = std::env::var("SEGMENT_STRICT").map(|v| v != "0").unwrap_or(true);
    // SEGMENT_STRICT=all: every segment must match to its end
    let all_strict = std::env::var("SEGMENT_STRICT").is_ok_and(|v| v == "all");
    let mut failed = vec![];
    for p in &paths {
        let doc = wf_import::segment::load(p).unwrap();
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let t = std::time::Instant::now();
        let rep = wf_import::segment::replay(&data, &files, &doc);
        // a segment is a gate only with "expect" (chapter work in progress reports without failing the suite)
        let gate = doc.get("expect").and_then(|e| e["matched"].as_u64()).map(|n| n as usize);
        let need = gate.unwrap_or(rep.frames).min(rep.frames);
        // "recs_exact": the characters' state must match the game's at every check too (wf_import::recs; the gate file's
        // flag, set once a segment is clean on it - tools/segments/recs_lock.py)
        let recs_exact = doc["recs_exact"].as_bool() == Some(true);
        let recs_ok = !recs_exact || rep.recs.as_ref().is_some_and(|t| t.bad_rows == 0 && t.checks > 0);
        let ok = (rep.compared >= need || (gate.is_none() && !all_strict)) && recs_ok;
        eprintln!(
            "segment {name}: {} video frames ({} logic), matched {} / {} | first divergence {:?} | import held {} draws {} | {} actor checks | {:.2}s{}",
            rep.frames,
            rep.logic,
            rep.compared,
            need,
            rep.first,
            rep.import.0,
            rep.import.1,
            rep.actors,
            t.elapsed().as_secs_f64(),
            if ok { "" } else { "  <-- FAIL" }
        );
        for n in &rep.notes {
            eprintln!("    import note: {n}");
        }
        for e in &rep.errors {
            eprintln!("    script error: {e}");
        }
        if !ok {
            eprintln!("    counts {:?}", rep.counts);
            failed.push(name.clone());
        }
        // SEGMENT_RECS=1 (report only): the characters' state against the game's (tools/segments/recs.py rows;
        // wf_import::recs) at the rows where the game's changed, while the segment matches
        if let Some(t) = &rep.recs {
            eprintln!("    recs {name}: {} checks, {} differ{}{}", t.checks, t.bad_rows, t.first.as_ref().map(|f| format!(" - first at f{} {:?}", f.0, f.1)).unwrap_or_default(), if t.fields.is_empty() { String::new() } else { format!(" fields {:?}", t.fields) });
        } else if rep.recs_missing {
            eprintln!("    recs {name}: no game rows (tools/segments/recs.py)");
        }
    }
    if strict {
        assert!(failed.is_empty(), "segments below their expected match: {failed:?}");
    }
}
