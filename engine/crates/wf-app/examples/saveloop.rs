//! Save -> Continue round trip through the app's real screens (headless, muted): a story session walks to a save
//! point, opens it with A (event s011: "-Save / -Switch Characters"), saves through the game's Save screen, the slot
//! file the launcher would write is taken (`launcher::saves_of`), a NEW app session is built on the title screen with
//! that file (`launcher::title_app`), Continue is chosen through the title's menus, and the continued world's story
//! state is compared with the saved one.
//!
//!   cargo run --release -p wf-app --example saveloop -- [W-A x y]      (default 12-1 252 144: the Airship's deck)

use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::Sim;

const A: u16 = 1;
const B: u16 = 2;
const START: u16 = 8;
const RIGHT: u16 = 0x10;
const LEFT: u16 = 0x20;
const UP: u16 = 0x40;
const DOWN: u16 = 0x80;

fn packs() -> Packs {
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap()
}

fn sim(app: &wf_app::App) -> &StorySim {
    app.sim.as_any().and_then(|a| a.downcast_ref::<StorySim>()).expect("story sim")
}

fn step(app: &mut wf_app::App, keys: u16, n: usize) {
    for _ in 0..n {
        app.inputs[0] = keys.into();
        app.step();
    }
}

fn describe(app: &wf_app::App) -> String {
    let w = sim(app).world();
    let p = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py(), w.pl_ref(pi).a.facing));
    format!(
        "area {:?} leader {:?} depth {} menu {} busy {:?} saves gen {} slots {:?}",
        w.field.as_ref().map(|f| (f.area.world, f.area.area)),
        p,
        w.scene.depth,
        wf_core::menu::active(w),
        w.field.as_ref().map(|f| f.busy),
        w.saves.generation,
        (0..3).map(|i| w.saves.slot_bytes(i).len()).collect::<Vec<_>>()
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let area = args.first().and_then(|a| wf_app::story::parse_area(a)).unwrap_or((12, 1));
    let tx: i32 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(252);
    let ty: i32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(144);
    // 1. a story session in the area (the launcher's quick start), a few coins' worth of state changed by play
    let s = StorySim::new(&packs(), area, vec![], 7).expect("story");
    let mut app = wf_app::App::new(packs(), Box::new(s));
    step(&mut app, 0, 90);
    println!("start: {}", describe(&app));
    // walk to (tx, ty): x first, then y (the deck is open floor), then face up
    for _ in 0..2000 {
        let w = sim(&app).world();
        let Some(pi) = w.player_idx(0) else { break };
        let (x, y) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
        let k = if (x - tx).abs() > 1 {
            if x < tx { RIGHT } else { LEFT }
        } else if (y - ty).abs() > 1 {
            if y < ty { DOWN } else { UP }
        } else {
            break;
        };
        step(&mut app, k, 1);
    }
    step(&mut app, UP, 3);
    step(&mut app, 0, 3);
    println!("at the save point: {}", describe(&app));
    // 2. A opens s011; A on "-Save"; the Save screen: A on the slot, A to confirm; B back out
    let gen0 = sim(&app).world().saves.generation;
    let mut log = vec![];
    for (k, n) in [(A, 3), (0, 60), (A, 3), (0, 90), (A, 3), (0, 60), (A, 3), (0, 60), (A, 3), (0, 120), (B, 3), (0, 60), (B, 3), (0, 90)] {
        step(&mut app, k, n);
        log.push(format!("{k:x}x{n}: {}", describe(&app)));
        if sim(&app).world().saves.generation != gen0 && !wf_core::menu::active(sim(&app).world()) && sim(&app).world().scene.depth == 0 {
            break;
        }
    }
    for l in &log {
        println!("  {l}");
    }
    let Some((generation, slots)) = wf_app::launcher::saves_of(&app) else { panic!("not story mode") };
    println!("saved: generation {generation} slot sizes {:?}", slots.iter().map(|s| s.len()).collect::<Vec<_>>());
    if generation == gen0 {
        println!("RESULT: the Save screen did not write a slot (key program / save point position)");
        return;
    }
    let saved = sim(&app).world().story.clone();
    let saved_pos = {
        let w = sim(&app).world();
        (w.field.as_ref().map(|f| (f.area.world, f.area.area)), w.player_idx(0).map(|pi| (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py())))
    };
    // 3. a new app session on the title with the slot files; START, A (Single Player), A (Game 1 = Continue)
    let mut app2 = wf_app::launcher::title_app(packs(), &slots, 99).expect("title app");
    let mut entered = false;
    for i in 0..1200 {
        let k = if i % 40 < 4 { if (i / 40) % 2 == 0 { START } else { A } } else { 0 };
        step(&mut app2, k, 1);
        let w = sim(&app2).world();
        if !wf_core::menu::active(w) && w.field.is_some() && w.player_idx(0).is_some() && w.story.flags == saved.flags && i > 60 {
            entered = true;
            println!("continued after {i} frames: {}", describe(&app2));
            break;
        }
    }
    if !entered {
        println!("RESULT: Continue did not reach the field: {}", describe(&app2));
        return;
    }
    step(&mut app2, 0, 120);
    // 4. compare the story state
    let w = sim(&app2).world();
    let st = &w.story;
    let mut diffs = vec![];
    if st.current != saved.current { diffs.push(format!("current {} vs {}", st.current, saved.current)); }
    if st.flags != saved.flags { diffs.push("flags".into()); }
    if st.supplies != saved.supplies { diffs.push("supplies".into()); }
    if st.equipment != saved.equipment { diffs.push(format!("equipment {:?} vs {:?}", st.equipment, saved.equipment)); }
    if st.zenie != saved.zenie { diffs.push(format!("zenie {} vs {}", st.zenie, saved.zenie)); }
    if st.g != saved.g { diffs.push(format!("G words {:?} vs {:?}", st.g, saved.g)); }
    if st.l != saved.l { diffs.push(format!("L words {:?} vs {:?}", st.l, saved.l)); }
    if st.game_bits != saved.game_bits { diffs.push(format!("game bits {:x} vs {:x}", st.game_bits, saved.game_bits)); }
    for (c, (a, b)) in st.chars.iter().zip(saved.chars.iter()).enumerate() {
        if serde_json::to_string(a).ok() != serde_json::to_string(b).ok() {
            diffs.push(format!("char {c}: {} vs {}", serde_json::to_string(a).unwrap_or_default(), serde_json::to_string(b).unwrap_or_default()));
        }
    }
    let pos = (w.field.as_ref().map(|f| (f.area.world, f.area.area)), w.player_idx(0).map(|pi| (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py())));
    println!("saved at {saved_pos:?}, continued at {pos:?}; play frames {} vs {}", st.play_frames, saved.play_frames);
    println!("script errors after Continue: {:?}", sim(&app2).errors);
    if diffs.is_empty() {
        println!("RESULT: round trip OK - the continued story state equals the saved one");
    } else {
        println!("RESULT: {} differences", diffs.len());
        for d in diffs {
            println!("  {}", &d[..d.len().min(600)]);
        }
    }
}
