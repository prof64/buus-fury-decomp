//! The whole real run on the app from power-on, no RAM import anywhere (headless, muted, no window / audio device):
//! the launcher's title session fed the emulator's whole-run recording (`tools/wholerun/record.py`: the soft reset,
//! then every key log of the real run), compared row by row.
//!
//!   cargo run --release -p wf-app --example wholerun -- MASTER.jsonl [--lag rec|sim] [--frames N] [--show A..B]
//!       [--rng A..B] [--report OUT.json]
//!
//! `--lag rec` (default): the recording's lag injected - a row is a logic step when its logic counter moved (or a menu
//! shows), else the sound hardware's frame only (as the app soak and the segment harness). `--lag sim`: the sim makes
//! its own load lag (`Options::sim_lag`, `Session::lag_step`), as `SEGMENT_SIMLAG=1`; a lag frame the sim does not model
//! is still taken from the recording (counted). Exact = every compared field equal on the row: area (outside menus),
//! the leader's position and facing (when both show one), the RNG (in the field, outside menus), the scene depth.
//! Prints the first exact divergence (row, stretch, field) and each field's first lasting divergence (150 rows).

use std::io::{BufRead, BufReader};
use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::Sim;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let master = args.first().expect("MASTER.jsonl").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let limit: usize = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let sim_lag = opt("--lag").as_deref() == Some("sim");
    let range = |name: &str| opt(name).and_then(|v| v.split_once("..").and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))));
    let show = range("--show");
    let rng_rows = range("--rng");
    let index: serde_json::Value = std::fs::read_to_string(format!("{master}.index.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let stretches: Vec<(String, usize)> = index["stretches"].as_array().map(|a| a.iter().map(|s| (s[0].as_str().unwrap_or("").to_string(), s[1].as_u64().unwrap_or(0) as usize)).collect()).unwrap_or_default();
    let stretch_of = |k: usize| stretches.iter().rev().find(|s| s.1 <= k).map(|s| s.0.clone()).unwrap_or_default();

    // power-on: the launcher's title app (`launcher::title_app`), no slot, no import, no sync
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    let cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::Seed(1), ..Default::default() };
    let mut session = wf_core::session::Session::new_field(data, cfg);
    // (--no-boot: the title at once, as the launcher opens it today)
    if args.iter().any(|a| a == "--no-boot") {
        wf_core::menu::title::open_title(&mut session.world);
    } else {
        wf_core::menu::title::open_title_boot(&mut session.world);
    }
    // --power-on FILE (tools/wholerun/power_on.py): the sound hardware at the reset - the driver's RAM as the boot code
    // sets it (the same after every reset) and the reset moment's play position and timer phase. Nothing else.
    if let Some(p) = opt("--power-on") {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).expect("power-on")).expect("json");
        let hex = v["driver"].as_str().unwrap_or("");
        let dram: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(0)).collect();
        let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
        if session.world.data.sound.is_some() {
            wf_core::sndsim::import(&mut session.world, &dram, n("pos"), n("last"), Some(n("phase")));
            println!("power-on sound: pos {} last {} phase {}", n("pos"), n("last"), n("phase"));
        } else {
            println!("power-on sound: no sound data loaded - ignored");
        }
    }
    let mut app = StorySim::with_session(&packs, session).expect("story sim");
    app.skip_area_banner();
    app.session.world.opts.sim_lag = sim_lag;
    println!("sound model: driver {} calibrated {} phase {}", app.world().sound.drv.is_some(), !app.world().sound.uncalibrated, app.world().sound.phase);

    let rd = BufReader::new(std::fs::File::open(&master).expect("master"));
    let mut lines = rd.lines().map(|l| serde_json::from_str::<serde_json::Value>(&l.expect("line")).expect("row")).peekable();
    let mut prev_lf = -1i64;
    let mut first_exact: Option<(usize, String)> = None;
    // lasting divergences per field: (count of consecutive bad rows, first row of the run, reported)
    let mut lasting: Vec<(&str, usize, usize, Option<(usize, String)>)> = ["area", "pos", "rng", "depth"].iter().map(|n| (*n, 0, 0, None)).collect();
    let (mut stepped, mut own_lag, mut replayed_lag, mut lag_bad) = (0usize, 0usize, 0usize, 0usize);
    let mut errors = 0usize;
    let mut k = 0usize;
    while let Some(r) = lines.next() {
        if k >= limit {
            break;
        }
        let keys = r["keys"].as_u64().unwrap_or(0) as u16;
        let lf = r["lf"].as_i64().unwrap_or(0);
        let logic = lf != prev_lf;
        prev_lf = lf;
        let menu = wf_core::menu::active(app.world());
        let next_lf = lines.peek().map(|n| n["lf"].as_i64().unwrap_or(0));
        let trace = rng_rows.is_some_and(|(a, b)| a <= k && k <= b);
        if trace {
            eprintln!("  rng calls of row {k} (app rng before {:08x?}, game after {})", app.world().rng.state(), r["rng"]);
        }
        wf_core::rng::TRACE.store(trace, std::sync::atomic::Ordering::Relaxed);
        if sim_lag && !menu && app.session.lag_step() {
            own_lag += 1;
            if logic {
                lag_bad += 1;
            }
        } else if logic || menu {
            let mut ev = vec![];
            app.step(&[keys], &mut ev);
            stepped += 1;
        } else {
            // the game's lag frame (recorded): the sound hardware's frame only
            if sim_lag {
                replayed_lag += 1;
            }
            wf_core::sndsim::lag_frame_ex(&mut app.session.world, next_lf.is_some_and(|n| n != lf));
        }
        wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
        if app.errors.len() > errors {
            for e in &app.errors[errors..] {
                println!("row {k}: script error: {e}");
            }
            errors = app.errors.len();
        }

        // the comparison
        let w = app.world();
        let menu = wf_core::menu::active(w);
        let mut bad: Vec<(&str, String)> = vec![];
        let ga = (r["area"][0].as_i64().unwrap_or(-1), r["area"][1].as_i64().unwrap_or(-1));
        let aa = w.field.as_ref().map(|f| (f.area.world as i64, f.area.area as i64)).unwrap_or((-1, -1));
        let in_field = ga != (0, 0) && !r["p"].is_null();
        if !menu && in_field && aa != ga {
            bad.push(("area", format!("app {aa:?} game {ga:?}")));
        }
        let ap = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x as i64, w.pl_ref(pi).a.y as i64, w.pl_ref(pi).a.facing as i64));
        if let (Some(p), Some(g)) = (ap, r["p"].as_array()) {
            let g = (g[0].as_i64().unwrap_or(0), g[1].as_i64().unwrap_or(0), g[2].as_i64().unwrap_or(0));
            if !menu && p != g {
                bad.push(("pos", format!("app {p:?} game {g:?}")));
            }
        } else if !menu && in_field && ap.is_none() != r["p"].is_null() {
            bad.push(("pos", format!("app leader {:?} game {}", ap, r["p"])));
        }
        if let Some(gr) = r["rng"].as_array() {
            let g = [gr[0].as_u64().unwrap_or(0) as u32, gr[1].as_u64().unwrap_or(0) as u32];
            if w.field.is_some() && !menu && in_field && w.rng.state() != g {
                bad.push(("rng", format!("app {:08x?} game {g:08x?}", w.rng.state())));
            }
        }
        let gd = r["depth"].as_i64().unwrap_or(0);
        if !menu && in_field && (w.scene.depth as i64 > 0) != (gd > 0) {
            bad.push(("depth", format!("app {} game {gd}", w.scene.depth)));
        }
        if first_exact.is_none() {
            if let Some((f, d)) = bad.first() {
                first_exact = Some((k, format!("{f}: {d}")));
                println!("first exact divergence: row {k} ({}): {f}: {d}", stretch_of(k));
            }
        }
        for l in lasting.iter_mut() {
            match bad.iter().find(|b| b.0 == l.0) {
                Some((_, d)) => {
                    if l.1 == 0 {
                        l.2 = k;
                    }
                    l.1 += 1;
                    if l.1 == 150 && l.3.is_none() {
                        l.3 = Some((l.2, d.clone()));
                        println!("lasting {} divergence from row {} ({}): {d}", l.0, l.2, stretch_of(l.2));
                    }
                }
                None => l.1 = 0,
            }
        }
        if show.is_some_and(|(a, b)| a <= k && k <= b) {
            println!("row {k} keys {keys:x} logic {logic} menu {menu} app area {aa:?} p {ap:?} rng {:08x?} depth {} | game area {ga:?} p {} rng {} depth {gd} lf {lf}", w.rng.state(), w.scene.depth, r["p"], r["rng"]);
            if menu || std::env::var("WR_MENU").is_ok() {
                let page = match w.menu.stack.last() {
                    Some(wf_core::menu::Screen::Title(t)) => format!("title {:?} leaving {:?}", t.page, t.leaving),
                    Some(_) => "other".to_string(),
                    None => "-".to_string(),
                };
                println!("    menu phase {:?} lag {} boot {} page {page} pending {} shown {}", w.menu.phase, w.menu.lag, w.menu.boot, w.menu.pending.is_some(), w.menu.shown.is_some());
            }
        }
        k += 1;
    }
    let exact_to = first_exact.as_ref().map(|f| f.0).unwrap_or(k);
    println!("rows {k} stepped {stepped} | sim lag {own_lag} (on game logic rows {lag_bad}), recorded lag replayed {replayed_lag} | script errors {errors}");
    println!("whole run, no imports ({} lag): exact to row {exact_to} of {k} ({})", if sim_lag { "sim" } else { "recorded" }, stretch_of(exact_to));
    for l in &lasting {
        println!("  first lasting {}: {:?}", l.0, l.3.as_ref().map(|(r, d)| (r, stretch_of(*r), d)));
    }
    if let Some(out) = opt("--report") {
        let rep = serde_json::json!({
            "lag": if sim_lag { "sim" } else { "recorded" },
            "rows": k,
            "exact_to": exact_to,
            "exact_stretch": stretch_of(exact_to),
            "first": first_exact.as_ref().map(|f| f.1.clone()),
            "lasting": lasting.iter().map(|l| (l.0.to_string(), l.3.as_ref().map(|(r, d)| serde_json::json!({"row": r, "stretch": stretch_of(*r), "what": d})))).collect::<std::collections::BTreeMap<_, _>>(),
            "stepped": stepped, "sim_lag": own_lag, "sim_lag_on_logic": lag_bad, "recorded_lag_replayed": replayed_lag, "script_errors": errors,
        });
        std::fs::write(out, serde_json::to_string_pretty(&rep).unwrap()).expect("report");
    }
}
