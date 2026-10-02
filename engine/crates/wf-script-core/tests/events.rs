//! Real ROM scripts against a headless wf-core World (no windows, no audio: sounds are Event data).
//! Content is extracted from roms/buu.gba once into the cargo tmp dir (`WF_CONTENT` overrides); skipped
//! without the ROM.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use wf_core::battle::BattleConfig;
use wf_core::content::{DirSource, GameData};
use wf_core::input::{InputFrame, A};
use wf_core::session::Session;
use wf_core::world::{Event, World};
use wf_script::vm::{Globals, Status, Thread};
use wf_script::{ScriptId, ScriptSet};
use wf_script_core::binding::{binding, Where};
use wf_script_core::{poll, CoreHost, CoreState, DropKind, Present, SharedHost, StoryState};

fn content_dir(rom: &[u8]) -> PathBuf {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return PathBuf::from(d);
    }
    // reuse wf-core's test extraction when present (same options)
    let shared = wf_rom::testutil::repo_root().join("engine/target/tmp/wf-core-content");
    if shared.join(".complete").exists() {
        return shared;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-script-core-content");
    if dir.join(".complete").exists() {
        return dir;
    }
    let opts = wf_extract::Options { only: ["tables", "sprites", "effects", "maps"].iter().map(|s| s.to_string()).collect() };
    let (content, _) = wf_extract::extract(rom.to_vec(), &opts).expect("extract");
    for (path, data) in &content.files {
        let p = dir.join(path.strip_prefix("base/").unwrap_or(path));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
    }
    std::fs::write(dir.join(".complete"), b"ok").unwrap();
    dir
}

struct Fixture {
    data: Arc<GameData>,
    set: Arc<ScriptSet>,
    files: Arc<std::collections::BTreeMap<String, String>>,
}

fn fixture() -> Option<&'static Fixture> {
    static F: OnceLock<Option<Fixture>> = OnceLock::new();
    F.get_or_init(|| {
        let rom = wf_rom::testutil::rom()?;
        let dir = content_dir(rom.data());
        let data = Arc::new(GameData::load(&DirSource { root: dir }).expect("content"));
        let set = ScriptSet::load_all(&rom);
        let (files, errs) = wf_script::export::export(&rom, &set);
        assert!(errs.is_empty());
        Some(Fixture { data, set: Arc::new(set), files: Arc::new(files) })
    })
    .as_ref()
}

fn world(f: &Fixture) -> World {
    Session::new(f.data.clone(), BattleConfig::default()).world
}

/// Run a VM thread to completion, answering blocks by stepping "frames" (leader presses A every 2nd frame).
fn run(set: &ScriptSet, w: &mut World, s: &mut StoryState, mut t: Thread) -> (Thread, u32) {
    let mut g = Globals::default();
    let mut st = t.run(set, &mut g, &mut CoreHost { w, s });
    let mut frames = 0;
    while let Ok(Status::Blocked) = st {
        frames += 1;
        assert!(frames < 10_000, "script never finished");
        let held = if frames % 2 == 0 { A } else { 0 };
        w.input[0] = InputFrame::from_held(held, 0);
        if let Some(v) = poll(w, s) {
            st = t.resume(v, set, &mut g, &mut CoreHost { w, s });
        }
    }
    st.unwrap_or_else(|e| panic!("{e:?}"));
    (t, frames)
}

#[test]
fn item_use_heals_consumes_and_plays_sfx() {
    let Some(f) = fixture() else { return };
    let mut w = world(f);
    let mut s = StoryState::default();
    s.supplies.insert(0, 2);
    let i = wf_script_core::leader(&w).unwrap();
    w.pl(i).rec.hp = 10;
    let before = w.pl_ref(i).rec.hp;
    // supply 0 use script (Miso Soup), referenced by the supply table at 0x083BD2F8 +0x30
    let (_t, _) = run(&f.set, &mut w, &mut s, Thread::new(ScriptId::Inline(0x083B_BBB0), [0; 4]));
    let after = w.pl_ref(i).rec.hp;
    assert!(after > before, "hp {before} -> {after}");
    assert_eq!(s.supplies[&0], 1);
    assert!(w.events.iter().any(|e| matches!(e, Event::Sfx { .. })));
}

fn loot_vm(f: &Fixture, seed: (u32, u32)) -> (Vec<wf_script_core::DropRequest>, [u32; 2]) {
    let mut w = world(f);
    w.rng.set_state([seed.0, seed.1]);
    let mut s = StoryState { loot_pos: (120, 80), ..Default::default() };
    let id = f.set.run_script_target(497).unwrap();
    run(&f.set, &mut w, &mut s, Thread::new(id, [75, -1, 0, 0]));
    (s.drops, w.rng.state())
}

#[test]
fn loot_drops_use_the_world_rng_deterministically() {
    let Some(f) = fixture() else { return };
    let mut any = 0;
    for k in 0..20u32 {
        let a = loot_vm(f, (k * 7919, k ^ 0x55aa));
        let b = loot_vm(f, (k * 7919, k ^ 0x55aa));
        assert_eq!(a, b);
        any += a.0.len();
        assert!(a.0.iter().all(|d| (d.x, d.y) == (120, 80)));
        assert!(a.0.iter().all(|d| match d.kind {
            DropKind::Zenie(z) => (0..4).contains(&z),
            _ => true,
        }));
    }
    assert!(any > 0, "20 kills dropped nothing");
}

#[test]
fn rhai_runtime_on_the_core_host_matches_the_vm() {
    let Some(f) = fixture() else { return };
    for k in 0..10u32 {
        let (vm_drops, vm_rng) = loot_vm(f, (k * 31337, 77 + k));
        let mut w = world(f);
        w.rng.set_state([k * 31337, 77 + k]);
        let st = Rc::new(RefCell::new(CoreState { world: w, story: StoryState { loot_pos: (120, 80), ..Default::default() } }));
        let mut rt = wf_script_rhai::Runtime::new(&f.files, Box::new(SharedHost(st.clone()))).unwrap();
        rt.start_unit(wf_script_rhai::UnitRef::new("s497", "n0_", false), [75, -1, 0, 0]);
        let r = rt.run().unwrap();
        assert_eq!(r, wf_script_rhai::Status::Finished);
        let b = st.borrow();
        assert_eq!(b.story.drops, vm_drops, "seed {k}");
        assert_eq!(b.world.rng.state(), vm_rng);
    }
}

#[test]
fn npc_talk_event_shows_text_and_waits_for_a() {
    let Some(f) = fixture() else { return };
    let mut w = world(f);
    let mut s = StoryState::default();
    // event 43: Gregory on Snake Way (one text node, no bytecode)
    let (t, frames) = run(&f.set, &mut w, &mut s, Thread::new_event(&f.set, 43, [0; 4]).unwrap());
    assert!(t.is_finished());
    assert!(frames >= 2, "the text box must wait for A");
    assert!(matches!(&s.present[0], Present::Text { text, .. } if text.contains("GREGORY")), "{:?}", s.present);
}

#[test]
fn bindings_are_honest() {
    let Some(f) = fixture() else { return };
    let (mut core, mut story, mut missing) = (0, 0, 0);
    for n in 8..248u8 {
        let mut w = world(f);
        let mut s = StoryState::default();
        let sig = wf_script::natives::sig(n);
        let args = vec![1; sig.pops as usize];
        let mut p = [0; 4];
        let mut g = Globals::default();
        let r = CoreHost { w: &mut w, s: &mut s }.call(&wf_script::vm::Call {
            native: n,
            args: &args,
            script: wf_script::vm::ScriptKey::Inline(0),
            pc: 0,
            params: &mut p,
            globals: &mut g,
        });
        match binding(n).at {
            Where::Vm => {}
            Where::Missing => {
                missing += 1;
                assert!(matches!(&r, Err(e) if e.contains("not available")), "0x{n:02x} {r:?}");
            }
            at => {
                if at == Where::Core {
                    core += 1;
                } else {
                    story += 1;
                }
                assert!(!matches!(&r, Err(e) if e.contains("not available")), "0x{n:02x} claims {at:?} but is missing");
                if let Ok(wf_script::vm::NativeResult::Done(v)) = &r {
                    assert_eq!(v.is_some(), sig.pushes, "0x{n:02x} result arity");
                }
            }
        }
    }
    eprintln!("bindings: {core} core, {story} story-state, {missing} missing");
}
