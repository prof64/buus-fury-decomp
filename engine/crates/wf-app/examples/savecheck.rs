//! In-game save files on the app (headless, muted): for each whole-run cut that `tools/wholerun/savecheck.py` saved
//! and continued on the emulator, the app imports the same cut, saves into slot 0 (`menu::save::save_data`, what the
//! save point's Save Game writes), then Continue on that slot (`menu::title::boot`, the title's path), steps until the
//! leader is controllable, and prints its state in the comparison layout `savecheck.py --compare` reads.
//!
//!   cargo run --release -p wf-app --example savecheck -- SAVECHECK.json OUT.json

use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::{Sim, SimEvent};

fn story_json(w: &wf_core::world::World) -> serde_json::Value {
    let st = &w.story;
    let mut eq: Vec<i32> = st.equipment.clone();
    eq.sort();
    let chars: Vec<serde_json::Value> = st
        .chars
        .iter()
        .take(9)
        .map(|c| {
            let r = &c.rec;
            serde_json::json!([r.xp, r.hp, r.max_hp, r.ep, r.max_ep, r.str_, r.pow, r.end, r.spd, r.points, c.form, c.sel, c.skills])
        })
        .collect();
    let p = w.player_idx(0).map(|i| {
        let a = &w.pl_ref(i).a;
        serde_json::json!([a.x >> 8, a.y >> 8, a.facing])
    });
    serde_json::json!({
        "area": w.field.as_ref().map(|f| [f.area.world, f.area.area, f.area.variant]),
        "p": p,
        "current": st.current,
        "zenie": st.zenie,
        "supplies": st.supplies.iter().take(83).copied().collect::<Vec<u8>>(),
        "flags": st.flags.iter().take(0x8F).map(|b| format!("{b:02x}")).collect::<String>(),
        "equipment": eq,
        "chars": chars,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&args[0]).expect("savecheck json")).expect("json");
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let mut out = vec![];
    for e in input.as_array().expect("array") {
        let Some(state) = e["state"].as_str() else { continue };
        if e.get("skip").is_some() {
            continue;
        }
        let doc_path = state.replace(".ss", ".json");
        let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&doc_path).expect("cut doc")).expect("json");
        let mut app = match StorySim::from_segment(&packs, &doc) {
            Ok(a) => a,
            Err(err) => {
                out.push(serde_json::json!({"state": state, "skip": err}));
                continue;
            }
        };
        app.session.world.opts.no_lag = true;
        let mut ev: Vec<SimEvent> = vec![];
        // (the state is savecheck.py's ready cut: the game saved there, no frame run)
        let w = &mut app.session.world;
        let d = wf_core::menu::save::save_data(w, false);
        let before = story_json(w);
        w.saves.set(0, Some(d));
        w.saves.current = 0;
        let chosen = w.story.current;
        // through the session's own path: a menu step runs, `title::boot` builds the world, its load events stay for the
        // script host (the area's on-enter script runs in the Continue's load)
        wf_core::menu::title::open_title(w);
        w.saves.pending = Some(wf_core::menu::title::Boot::Continue { slot: 0, chosen });
        ev.clear();
        app.step(&[0], &mut ev);
        // until the title's menu is gone and the leader is in the field (the field free, or the area's own event
        // starting as the Continue's fade ends - the game's too: whole-run cut 756,163, Pilaf's in 10-31)
        let mut n = 0;
        while n < 2000 {
            let w = &app.session.world;
            if std::env::var_os("XSC").is_some() && n % 200 == 0 { eprintln!("XSC n {} menu {} player {:?} arrival {:?} busy {:?} depth {} area {:?}", n, wf_core::menu::active(w), w.player_idx(0), w.field.as_ref().map(|f| f.arrival.is_some()), w.field.as_ref().map(|f| f.busy), w.scene.depth, w.field.as_ref().map(|f| f.area)); }
            if !wf_core::menu::active(w) && w.player_idx(0).is_some() {
                break;
            }
            ev.clear();
            app.step(&[0], &mut ev);
            n += 1;
        }
        out.push(serde_json::json!({"state": state, "before": before, "after": story_json(&app.session.world), "continue_steps": n, "rng": app.session.world.rng.state()}));
        println!("{}: continued in {n} steps", state.rsplit('/').next().unwrap_or(state));
    }
    std::fs::write(&args[1], serde_json::to_string(&out).unwrap()).expect("write");
}
