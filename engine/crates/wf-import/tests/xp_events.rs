//! XP and level-ups frame by frame against the game's character records along the segments.
//!
//! `data/segments/recs/<name>.json` (tools/segments/recs.py: the game's records at row 0 and at every row they
//! change) give the row each character's XP, max HP and stat points change at. The segments gate compares ours at
//! those rows only - an award a frame early would pass there. Here ours is compared at EVERY row a logic iteration
//! of the game ends on, while the segment matches: the XP of every character (a kill's, a script's award), and with
//! it the level-up's record changes (max HP / max EP / points). The awards are counted by what the field was doing
//! in our replay at that row: free, or held by an event (a kill landing under a script - an on-defeat script of the
//! same frame, a shot's kill in a cutscene - and a script's own award).
//! `XPEV=name[,name]` only those; `XPEV_LOG=1` lists every award under an event.

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


fn game_xp(state: &serde_json::Value) -> Vec<(i64, i64, i64, i64)> {
    // (per character: XP, max HP, max EP, points - fields 0, 2, 4, 9 of the recs.py layout)
    state["chars"]
        .as_array()
        .map(|a| a.iter().map(|c| (c[0].as_i64().unwrap_or(0), c[2].as_i64().unwrap_or(0), c[4].as_i64().unwrap_or(0), c[9].as_i64().unwrap_or(0))).collect())
        .unwrap_or_default()
}

fn our_xp(w: &wf_core::world::World) -> Vec<(i64, i64, i64, i64)> {
    let mut c = w.clone();
    wf_core::story::sync_from_players(&mut c);
    c.story.chars.iter().take(9).map(|ch| (ch.rec.xp, ch.rec.max_hp as i64, ch.rec.max_ep as i64, ch.rec.points as i64)).collect()
}

#[test]
fn xp_lands_on_the_games_frames() {
    let dir = repo().join("data/segments/recs");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut names: Vec<String> = rd.filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".json").map(String::from)).collect();
    names.sort();
    if let Ok(only) = std::env::var("XPEV") {
        let pick: Vec<&str> = only.split(',').collect();
        names.retain(|n| pick.contains(&n.as_str()));
    }
    if names.is_empty() {
        return;
    }
    let Some((data, files)) = setup() else { return };
    let log = std::env::var_os("XPEV_LOG").is_some();
    let next = std::sync::atomic::AtomicUsize::new(0);
    // per segment: rows compared, rows differing (first), awards free / under an event, level-ups free / under one
    #[allow(clippy::type_complexity)]
    let out = std::sync::Mutex::new(Vec::<(String, usize, usize, Option<(usize, String)>, [usize; 4])>::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(10);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let k0 = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(name) = names.get(k0).cloned() else { break };
                let Ok(doc) = wf_import::segment::load(&repo().join(format!("data/segments/{name}.json"))) else { continue };
                let Some(game) = wf_import::recs::load(&doc) else { continue };
                // ours per row: the records' numbers (taken when the cheap sum changes), and whether an event held the field
                let mut ours: Vec<(Vec<(i64, i64, i64, i64)>, bool)> = vec![];
                let mut last_sum = i64::MIN;
                let mut cur: Vec<(i64, i64, i64, i64)> = vec![];
                let rep = wf_import::segment::replay_with(&data, &files, &doc, &mut |f| {
                    if let wf_import::segment::Frame::After(k, w) = f {
                        let live: i64 = w.players.iter().filter_map(|&id| w.find(id)).map(|i| { let r = &w.pl_ref(i).rec; r.xp + r.max_hp as i64 * 7 + r.points as i64 * 13 }).sum();
                        let sum = live + w.story.chars.iter().map(|c| c.rec.xp + c.rec.max_hp as i64 * 7 + c.rec.points as i64 * 13).sum::<i64>();
                        if sum != last_sum {
                            last_sum = sum;
                            cur = our_xp(w);
                        }
                        let held = w.field.as_ref().is_some_and(|f| f.busy || f.held || f.held_event);
                        if ours.len() <= k {
                            ours.resize(k + 1, (cur.clone(), held));
                        }
                        ours[k] = (cur.clone(), held);
                    }
                });
                let lf: Vec<i64> = doc["frames"].as_array().map(|a| a.iter().map(|r| r["lf"].as_i64().unwrap_or(0)).collect()).unwrap_or_default();
                let n = rep.compared.min(ours.len());
                let (mut gi, mut bad, mut first, mut cnt) = (0usize, 0usize, None, [0usize; 4]);
                let mut prev: Option<Vec<(i64, i64, i64, i64)>> = None;
                let mut rows = 0;
                // (an event holding the field already at the row before: the level-up's own popup holds it from the
                // award's row on)
                let mut held_before = false;
                for k in 0..n {
                    while gi + 1 < game.len() && game[gi + 1].0 <= k {
                        gi += 1;
                    }
                    if game[gi].0 > k || lf.get(k + 1).is_some_and(|&x| x == lf[k]) {
                        continue;
                    }
                    let g = game_xp(&game[gi].1);
                    rows += 1;
                    // (a character the game has no record for yet is not compared, as recs.rs)
                    let differ: Vec<String> = g
                        .iter()
                        .zip(&ours[k].0)
                        .enumerate()
                        .filter(|(_, (g, o))| !(g.0 == 0 && g.1 == 0) && g != o)
                        .map(|(c, (g, o))| format!("char {c}: ours {o:?} vs the game's {g:?}"))
                        .collect();
                    if !differ.is_empty() {
                        bad += 1;
                        first.get_or_insert((k, differ.join("; ")));
                    }
                    if let Some(p) = &prev {
                        for (c, (a, b)) in p.iter().zip(&g).enumerate() {
                            if b.0 > a.0 {
                                let ev = held_before as usize;
                                cnt[ev] += 1;
                                let up = b.1 != a.1 || b.3 != a.3;
                                if up {
                                    cnt[2 + ev] += 1;
                                }
                                if log && ev == 1 {
                                    eprintln!("XPEV {name} row {k}: char {c} +{} XP under an event{}", b.0 - a.0, if up { " (level-up)" } else { "" });
                                }
                            }
                        }
                    }
                    prev = Some(g);
                    held_before = ours[k].1;
                }
                out.lock().unwrap().push((name, rows, bad, first, cnt));
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort();
    let rows: usize = out.iter().map(|o| o.1).sum();
    let tot = out.iter().fold([0usize; 4], |mut a, o| {
        for k in 0..4 {
            a[k] += o.4[k];
        }
        a
    });
    let known: Vec<String> = std::fs::read_to_string(repo().join("data/segments/timers/xp_known.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["known"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()))
        .unwrap_or_default();
    let bad: Vec<&(String, usize, usize, Option<(usize, String)>, [usize; 4])> = out.iter().filter(|o| o.2 > 0).collect();
    eprintln!(
        "xp: {} segments, {rows} rows compared; awards {} free + {} under an event, level-ups {} free + {} under an event; {} segments differ",
        out.len(), tot[0], tot[1], tot[2], tot[3], bad.len()
    );
    for b in &bad {
        eprintln!("  {}{}: {} rows differ, first {:?}", b.0, if known.contains(&b.0) { " (known)" } else { "" }, b.2, b.3);
    }
    let new: Vec<&String> = bad.iter().map(|b| &b.0).filter(|n| !known.contains(n)).collect();
    assert!(new.is_empty(), "XP / level-up rows differ from the game's: {new:?}");
    if std::env::var_os("XPEV").is_none() {
        assert!(tot[1] > 0, "no award under an event measured");
    }
}
