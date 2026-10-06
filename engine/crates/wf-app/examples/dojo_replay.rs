//! `dojo_replay CONTENT REPORT.json [OUT.json]` - a Dojo match recorded by EITHER build (a native `--link-report`
//! file, or a browser page's `wfLinkInfo().report` saved as JSON) replayed on THIS (native) build: the game rebuilt
//! from the report's `match.replay.def`, advanced with its confirmed inputs, every frame's checksum compared with the
//! recorded ones (`wf_app::link::dojo_replay`). Prints `identical: N frames` or the first differing frame; OUT gets
//! this build's per-frame checksums. Exit 0 = identical on every frame (engine/docs/netcode.md "Link play: browser
//! <-> native").
use std::path::PathBuf;
use wf_app::wf_content::{DirSource, FileSource, Packs};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 {
        eprintln!("usage: dojo_replay CONTENT REPORT.json [OUT.json]");
        std::process::exit(2);
    }
    // the base pack as the app loads it: `<content>/base` when there is one (the app's --content folder), else the
    // folder itself (an extracted base). (The test-extracted folder holds both: an older flat copy and base/ - the
    // flat one replayed differently from frame 60, the first fight frame)
    let dir = PathBuf::from(&a[0]);
    let base = if dir.join("base/pack.json").exists() { dir.join("base") } else { dir };
    let packs = Packs::new(vec![Box::new(DirSource(base)) as Box<dyn FileSource>]).expect("content");
    // (game_data, as the app and the browser: it also sets the battle roster the round sessions are built from)
    let data = wf_app::battle::game_data(&packs).expect("game data");
    let rep: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[1]).expect("report")).expect("report json");
    // a native report has {match: {replay}}, a browser info {report: {match: {replay}}}
    let m = if rep["match"].is_object() { &rep["match"] } else { &rep["report"]["match"] };
    let replay = &m["replay"];
    let (hashes, first) = wf_app::link::dojo_replay(data, replay).unwrap_or_else(|e| {
        eprintln!("bad replay: {e}");
        std::process::exit(2)
    });
    if let Some(out) = a.get(2) {
        let hex: Vec<String> = hashes.iter().map(|x| format!("{x:016x}")).collect();
        std::fs::write(out, serde_json::to_string(&hex).unwrap()).expect("out");
    }
    let recorded = replay["hashes"].as_array().map(|v| v.len()).unwrap_or(0);
    match first {
        None => {
            println!("identical: {} frames (recorded {recorded})", hashes.len());
        }
        Some(f) => {
            println!(
                "DIFFERS at frame {f}: this build {:?}, recorded {:?} ({} frames here, {recorded} recorded)",
                hashes.get(f).map(|x| format!("{x:016x}")),
                replay["hashes"].get(f),
                hashes.len()
            );
            std::process::exit(1);
        }
    }
}
