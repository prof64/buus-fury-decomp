//! The timed boost's count (Holy Water / `boost_stats`: GameData +0x4DC, frames left) along the segments.
//!
//! `data/segments/timers/holy.json` (`/tmp`-style tool: replay each segment's keys on the emulator from its start with
//! the count poked to 20,000 and the four bonuses to 0 - nothing else of the game changes, so the recording's keys stay
//! in step - and read the count after every row): per segment the runs `[row, value, step]`. The game counts it in the
//! field update's own routine (0x0800425C, with the countdown and the stopwatch): one a frame while the field runs
//! free. Ours: the segment's start with the same poke (`init.ram.poke`), replayed; compared at every row a logic
//! iteration of the game ends on, while the segment matches. The file's `known` segments are reported, not failed.
//! `HOLY=name[,name]` only those; `HOLY_LOG=N` prints where ours and the game's part (N changes), `HOLY_DBG=a-b` our
//! field's state over those rows.

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
    if let Ok(b) = std::fs::read(content_dir().join("scripts/bytecode.json")) {
        wf_import::host::set_step_code(&b);
    }
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, errs) = wf_script::export::export(&rom, &set);
    assert!(errs.is_empty(), "decompile errors: {errs:?}");
    Some((data, files))
}

const START: i64 = 20_000;

/// the game's value per row from the runs `[row, value, step]`
fn unpack(v: &serde_json::Value, rows: usize) -> Vec<i64> {
    let a = v.as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(rows);
    for (k, e) in a.iter().enumerate() {
        let (r, val, st) = (e[0].as_u64().unwrap_or(0) as usize, e[1].as_i64().unwrap_or(0), e[2].as_i64().unwrap_or(0));
        let end = a.get(k + 1).and_then(|n| n[0].as_u64()).map_or(rows.max(r + 1), |x| x as usize);
        out.extend((0..end.saturating_sub(r)).map(|j| val + st * j as i64));
    }
    out
}

#[test]
fn timed_boost_counts_as_the_game() {
    let Ok(b) = std::fs::read(repo().join("data/segments/timers/holy.json")) else { return };
    let all: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let mut paths: Vec<(String, serde_json::Value)> = all["segments"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    if let Ok(only) = std::env::var("HOLY") {
        let names: Vec<&str> = only.split(',').collect();
        paths.retain(|p| names.contains(&p.0.as_str()));
    }
    if paths.is_empty() {
        return;
    }
    let Some((data, files)) = setup() else { return };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::<(String, usize, usize, Option<(usize, i64, i64)>)>::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(10);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let k0 = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some((name, game)) = paths.get(k0).cloned() else { break };
                let Ok(mut doc) = wf_import::segment::load(&repo().join(format!("data/segments/{name}.json"))) else { continue };
                // the start's GameData with the timer set and no bonuses (`init.ram.poke`)
                const GD: u64 = 0x0300_1068;
                let mut poke = vec![serde_json::json!([GD + 0x4DC, START])];
                poke.extend((0..4).map(|k| serde_json::json!([GD + 0x4E0 + 4 * k, 0])));
                doc["init"]["ram"]["poke"] = serde_json::Value::Array(poke);
                let mut ours: Vec<i64> = vec![];
                let rep = wf_import::segment::replay_with(&data, &files, &doc, &mut |f| {
                    if let wf_import::segment::Frame::After(k, w) = f {
                        let v = w.player_idx(0).map(|i| w.pl_ref(i).holy_left as i64).unwrap_or(-1);
                        if let Some((a, b)) = std::env::var("HOLY_DBG").ok().and_then(|v| v.split_once('-').map(|(a, b)| (a.parse::<usize>().unwrap_or(0), b.parse::<usize>().unwrap_or(0)))) {
                            if (a..=b).contains(&k) {
                                let f = w.field.as_ref();
                                eprintln!("DBG {k}: holy {v} mode {:?} busy {:?} item_get {} menu {} dead {:?} frame {} scene depth {} flying {} held {:?} it_sel {:?} gs {:?}", f.map(|f| format!("{:?}", f.mode)), f.map(|f| f.busy), f.is_some_and(|f| f.item_get.is_some()), w.menu.stack.len(), w.player_idx(0).map(|i| w.pl_ref(i).dead), w.frame, w.scene.depth, wf_core::fieldworld::flying(w), f.map(|f| (f.held, f.drop_held, f.held_event, f.nested, f.completing)), w.player_idx(0).map(|i| w.pl_ref(i).it_sel.is_some()), w.player_idx(0).map(|i| format!("{:?}", w.pl_ref(i).gs)));
                            }
                        }
                        if ours.len() <= k {
                            ours.resize(k + 1, v);
                        }
                        ours[k] = v;
                    }
                });
                let want = unpack(&game, rep.frames);
                let n = rep.compared.min(want.len()).min(ours.len());
                // (compared where a logic iteration of the game ends: the rows in between are its lag frames, which
                // ours does not have)
                let lf: Vec<i64> = doc["frames"].as_array().map(|a| a.iter().map(|r| r["lf"].as_i64().unwrap_or(0)).collect()).unwrap_or_default();
                for k in 0..n {
                    if lf.get(k + 1).is_some_and(|&x| x == lf[k]) {
                        ours[k] = -1;
                    }
                }
                let first = (0..n).find(|&k| ours[k] != want[k] && ours[k] >= 0).map(|k| (k, ours[k], want[k]));
                let bad = (0..n).filter(|&k| ours[k] != want[k] && ours[k] >= 0).count();
                if std::env::var("HOLY_LOG").is_ok() {
                    let mut last = 0;
                    let mut ch = vec![];
                    for k in 0..n {
                        let d = ours[k] - want[k];
                        if ours[k] >= 0 && d != last {
                            ch.push((k, d, rep.started.iter().rev().find(|s| s.0 <= k).map(|s| s.1.clone()).unwrap_or_default()));
                            last = d;
                        }
                    }
                    eprintln!("HOLYLOG {name} ({n} rows): {:?}", &ch[..ch.len().min(std::env::var("HOLY_LOG").ok().and_then(|v| v.parse().ok()).unwrap_or(14))]);
                }
                out.lock().unwrap().push((name, n, bad, first));
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort();
    let mut failed = vec![];
    for (name, n, bad, first) in &out {
        if let Some((k, a, b)) = first {
            let known = all["known"].get(name).and_then(|x| x.as_str());
            eprintln!("holy {name}: {bad} of {n} rows differ - first at row {k}: ours {a}, the game {b}{}", known.map(|w| format!(" (known: {w})")).unwrap_or_default());
            if known.is_none() {
                failed.push(name.clone());
            }
        }
    }
    eprintln!("timed boost: {} segments, {} rows compared, {} segments differ", out.len(), out.iter().map(|o| o.1).sum::<usize>(), out.iter().filter(|o| o.3.is_some()).count());
    assert!(failed.is_empty(), "the timed boost's count differs: {failed:?}");
}
