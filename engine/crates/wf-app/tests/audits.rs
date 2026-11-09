//! The audit gates (headless, muted; skipped without the ROM): the two frame-by-frame comparisons with the emulator
//! that used to be run by hand - the opponent audit (every enemy type's scenarios: `data/audits/opp`, packed by
//! `examples/opp_audit.rs` with `OA_PACK`) and the skill audit (every player skill: `data/audits/skill`, packed by
//! `examples/skill_audit.rs` with `SA_PACK`). Every scenario is replayed and compared on every frame: the state rows
//! and the hash of the game's picture.
//!
//! * a scenario exact throughout (state and pictures) is LOCKED: any difference fails;
//! * the others are in the RATCHET (`data/audits/{opp,skill}.json`): per scenario `[first frame with a state
//!   difference, count of them, first frame whose picture differs, count of them]` (`null` = none). The gate fails when
//!   a scenario gets worse (an earlier first difference, or more differing frames) and prints the ones that got better.
//!
//! To tighten the ratchet after a fix (it only ever records what the run measured - read the printed changes first):
//!   AUDIT_RATCHET=write cargo test --release -p wf-app --test audits

#[path = "audit/opp.rs"]
mod opp;
#[path = "audit/pack.rs"]
mod pack;
#[path = "audit/skill.rs"]
mod skill;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use wf_app::wf_content::{DirSource, FileSource, Packs};

/// one scenario's measure: (first frame with a state difference, their count, first frame whose picture differs,
/// their count)
type Measure = (Option<usize>, usize, Option<usize>, usize);

fn content(repo: &Path) -> PathBuf {
    // the segment harness's shared extraction (as tests/pixels.rs)
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    content
}

fn scenarios(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".json.z")).collect()).unwrap_or_default();
    v.sort();
    v
}

/// every scenario through `run` on parallel workers (each with its own state from `init`): name -> (frames, measure)
fn measure<S>(paths: &[PathBuf], init: &(dyn Fn() -> S + Sync), run: &(dyn Fn(&mut S, &serde_json::Value) -> (usize, Measure) + Sync)) -> BTreeMap<String, (usize, Measure)> {
    let next = AtomicUsize::new(0);
    let out = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| {
                let mut st = init();
                loop {
                    let Some(p) = paths.get(next.fetch_add(1, Ordering::Relaxed)) else { break };
                    let name = p.file_name().unwrap().to_string_lossy().trim_end_matches(".json.z").to_string();
                    let doc = pack::read(p).unwrap_or_else(|e| panic!("{e}"));
                    let r = run(&mut st, &doc);
                    out.lock().unwrap().insert(name, r);
                }
            });
        }
    });
    out.into_inner().unwrap()
}

/// the lock and the ratchet: a scenario not in the ratchet file must be exact, one in it must not be worse
fn gate(what: &str, file: &Path, got: &BTreeMap<String, (usize, Measure)>) {
    let old: BTreeMap<String, Measure> = std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| {
            Some(
                v["ratchet"]
                    .as_object()?
                    .iter()
                    .map(|(k, r)| {
                        let n = |i: usize| r[i].as_u64().map(|x| x as usize);
                        (k.clone(), (n(0), n(1).unwrap_or(0), n(2), n(3).unwrap_or(0)))
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    let frames: usize = got.values().map(|r| r.0).sum();
    let exact = got.values().filter(|r| r.1 == (None, 0, None, 0)).count();
    let (sd, pd): (usize, usize) = got.values().fold((0, 0), |a, r| (a.0 + r.1 .1, a.1 + r.1 .3));
    eprintln!("{what}: {} scenarios, {frames} frames | exact scenarios {exact} | state differences {sd} | frames with a picture difference {pd}", got.len());
    let show = |m: &Measure| format!("state {} x{} picture {} x{}", m.0.map(|f| f.to_string()).unwrap_or("-".into()), m.1, m.2.map(|f| f.to_string()).unwrap_or("-".into()), m.3);
    // (an earlier first difference or more differing frames = worse; `None` = no difference)
    let first = |f: Option<usize>| f.unwrap_or(usize::MAX);
    let (mut fails, mut better) = (vec![], vec![]);
    for (name, (_, m)) in got {
        match old.get(name) {
            None if *m != (None, 0, None, 0) => fails.push(format!("{name}: LOCKED (exact) scenario now differs: {}", show(m))),
            None => {}
            Some(o) => {
                let worse = first(m.0) < first(o.0) || m.1 > o.1 || first(m.2) < first(o.2) || m.3 > o.3;
                if worse {
                    fails.push(format!("{name}: worse than the ratchet: {} (was {})", show(m), show(o)));
                } else if m != o {
                    better.push(format!("{name}: {} (was {}){}", show(m), show(o), if *m == (None, 0, None, 0) { " - EXACT now" } else { "" }));
                }
            }
        }
    }
    for name in old.keys().filter(|n| !got.contains_key(*n)) {
        fails.push(format!("{name}: in the ratchet, no data"));
    }
    if std::env::var("AUDIT_RATCHET").as_deref() == Ok("write") {
        let r: serde_json::Map<String, serde_json::Value> = got.iter().filter(|(_, r)| r.1 != (None, 0, None, 0)).map(|(n, (_, m))| (n.clone(), serde_json::json!([m.0, m.1, m.2, m.3]))).collect();
        let doc = serde_json::json!({
            "note": "audit gate (engine/crates/wf-app/tests/audits.rs): the scenarios that are not exact - [first frame with a state difference, count, first frame whose picture differs, count]; every other scenario is locked exact. Re-made by `AUDIT_RATCHET=write cargo test --release -p wf-app --test audits`",
            "scenarios": got.len(),
            "frames": frames,
            "exact": exact,
            "state_differences": sd,
            "picture_frames": pd,
            "ratchet": r,
        });
        std::fs::write(file, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        eprintln!("{what}: ratchet written ({} scenarios in it, {} worse and {} better than before)", r.len(), fails.len(), better.len());
        for l in fails.iter().chain(&better) {
            eprintln!("  {l}");
        }
        return;
    }
    if !better.is_empty() {
        eprintln!("{what}: {} scenarios better than the ratchet (tighten it: AUDIT_RATCHET=write cargo test --release -p wf-app --test audits):", better.len());
        for l in &better {
            eprintln!("  {l}");
        }
    }
    assert!(fails.is_empty(), "{what}:\n{}", fails.join("\n"));
}

#[test]
fn opponent_audit_locked_and_ratcheted() {
    let repo = wf_rom::testutil::repo_root();
    let paths = scenarios(&repo.join("data/audits/opp"));
    if paths.is_empty() || wf_rom::testutil::rom().is_none() {
        return;
    }
    let content = content(&repo);
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let d = wf_app::battle::load_data(&packs).expect("data");
    let _ = opp::DATA.set(std::sync::Arc::new((*d).clone()));
    let got = measure(&paths, &|| Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap(), &|packs, doc| {
        let torn = doc["pic"]["torn"].as_object().map(|o| o.iter().filter_map(|(f, s)| Some((f.parse().ok()?, s.as_array()?.iter().map(|l| l.as_str().unwrap_or("").to_string()).collect()))).collect()).unwrap_or_default();
        let pics = opp::Pictures::Hashes { tile_off: doc["pic"]["tile_off"].as_u64().unwrap_or(0) as u32, hashes: pack::hashes(doc), torn };
        let r = opp::audit(doc, &pics, packs, None, &mut None);
        (r.frames as usize, (r.state.first().copied(), r.state.len(), r.picture.first().copied(), r.picture.len()))
    });
    gate("opponent audit", &repo.join("data/audits/opp.json"), &got);
}

#[test]
fn skill_audit_locked_and_ratcheted() {
    let repo = wf_rom::testutil::repo_root();
    let paths = scenarios(&repo.join("data/audits/skill"));
    let Some(rom) = wf_rom::testutil::rom() else { return };
    if paths.is_empty() {
        return;
    }
    let content = content(&repo);
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    if let Ok(f) = wf_render::ui::Font::load(&packs, "dialog") {
        wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
    }
    if let Ok(b) = std::fs::read(content.join("scripts/bytecode.json")) {
        wf_import::host::set_step_code(&b);
    }
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, _) = wf_script::export::export(&rom, &set);
    let init = || (Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap(), wf_render::Renderer::new(), wf_render::Renderer::new());
    let got = measure(&paths, &init, &|(packs, renderer, tear_renderer), doc| {
        let h = pack::hashes(doc);
        let torn = &doc["pic"]["torn"];
        let r = skill::check(&data, &files, packs, renderer, tear_renderer, doc, &mut |k, ours, cand| {
            let g = *h.get(k)?;
            if g == pack::hash(ours) {
                return Some(true);
            }
            // (a torn frame: our picture with the previous state on the lines its spec names)
            let Some(spec) = torn[k.to_string().as_str()].as_array() else { return Some(false) };
            let spec: Vec<String> = spec.iter().map(|l| l.as_str().unwrap_or("").to_string()).collect();
            let mut px = ours.to_vec();
            pack::compose(&mut px, &spec, cand);
            Some(g == pack::hash(&px))
        });
        (r.frames, (r.state.as_ref().map(|s| s.0), r.state.as_ref().map(|s| s.1).unwrap_or(0), r.picture.first().copied(), r.picture.len()))
    });
    gate("skill audit", &repo.join("data/audits/skill.json"), &got);
}
