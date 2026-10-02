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
    // reuse wf-core's field-test extraction when present (same options)
    let shared = wf_rom::testutil::repo_root().join("engine/target/tmp/wf-core-field-content");
    if shared.join(".complete").exists() {
        return shared;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-script-core-field-content");
    if dir.join(".complete").exists() {
        return dir;
    }
    let opts = wf_extract::Options { only: ["tables", "sprites", "effects", "maps", "world", "items", "scripts"].iter().map(|s| s.to_string()).collect() };
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

fn field_world(f: &Fixture) -> World {
    Session::new_field(f.data.clone(), wf_core::field::FieldConfig { world: 1, area: 1, variant: 1, ..Default::default() }).world
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
        w.scene.keys = w.input;
        wf_core::scene::tick(w);
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
    w.story.supplies = vec![0; 83];
    w.story.supplies[0] = 2;
    let i = wf_script_core::leader(&w).unwrap();
    w.pl(i).rec.hp = 10;
    let before = w.pl_ref(i).rec.hp;
    // supply 0 use script (Miso Soup), referenced by the supply table at 0x083BD2F8 +0x30
    let (_t, _) = run(&f.set, &mut w, &mut s, Thread::new(ScriptId::Inline(0x083B_BBB0), [0; 4]));
    let after = w.pl_ref(i).rec.hp;
    assert!(after > before, "hp {before} -> {after}");
    assert_eq!(w.story.supplies[0], 1);
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

/// enemy AI script nodes: RunScript names its event by header address (`header_0xADDR`), InlineScript by its
/// bytecode (`inline_0xADDR`); the script host must resolve every one the AI content uses
#[test]
fn ai_script_nodes_resolve_to_scripts() {
    let Some(f) = fixture() else { return };
    let st = Rc::new(RefCell::new(CoreState { world: world(f), story: StoryState::default() }));
    let rt = wf_script_rhai::Runtime::new(&f.files, Box::new(SharedHost(st))).unwrap();
    // Goten's tag game (#121) and Super Buu's three phases (#474-#476)
    for (h, ev) in [(0x0878_3150u32, 121u16), (0x0879_7A90, 474), (0x0879_7BC4, 475), (0x0879_7E14, 476)] {
        assert_eq!(rt.event_id(&format!("header_0x{h:08X}")), Some(ev), "header 0x{h:08X}");
    }
    assert_eq!(rt.event_id("s121"), Some(121));
    // Vegeta 0x88's inline node bytecode
    assert!(f.files.contains_key("scripts/inline/inline_0x0870EA28.rhai"), "inline AI script not exported");
}

/// Vegeta 0x88 (training NPC): with the player close its AI runs the inline script 0x0870EA28 once and the node
/// waits (the game runs the script inside the update: its blocking natives run nested frames) until the host
/// reports the end
#[test]
fn ai_inline_script_node_waits_for_the_script() {
    let Some(f) = fixture() else { return };
    let mut s = Session::new_field(f.data.clone(), wf_core::field::FieldConfig { world: 1, area: 1, variant: 1, ..Default::default() });
    wf_core::field::attach_host(&mut s.world, true);
    // Vegeta at the arena's left edge, the player 30 px to his right: he backs off into the edge, the step-away
    // node ends and the InlineScript node runs
    let pi = s.world.player_idx(0).unwrap();
    let py = s.world.pl_ref(pi).a.py();
    let ei = wf_core::battle::spawn_enemy(&mut s.world, 0x88, -64, py, 0, None).expect("Vegeta 0x88");
    let vx = s.world.actor(ei).map(|a| a.x).unwrap();
    s.world.pl(pi).a.x = vx + (30 << 8);
    let vid = s.world.ents[ei].id;
    let mut runs = 0;
    let mut started = None;
    for k in 0..600 {
        s.step(&[0]);
        for e in &s.world.events {
            if let Event::Field(wf_core::field::FieldEvent::Run { run, cause: wf_core::field::Cause::Ai { entity }, .. }) = e {
                assert_eq!(*entity, vid);
                assert_eq!(run, &wf_core::fieldcontent::Callback::Script("inline_0x0870EA28".into()));
                runs += 1;
                started.get_or_insert(k);
            }
        }
        if started.is_some_and(|f| k > f + 120) {
            break;
        }
    }
    assert!(started.is_some(), "the AI never reached its InlineScript node");
    assert_eq!(runs, 1, "the node must not restart its script while it waits");
    assert!(s.world.scene.ai_wait.contains(&vid));
    wf_core::scene::ai_script_done(&mut s.world, vid);
    s.step(&[0]);
    assert!(s.world.scene.ai_wait.is_empty());
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
    // every native runs on a real World (battle and field mode) with the right result arity; blocking ones block
    // and complete under poll within a bounded number of frames (A pressed every other frame)
    let mut count = std::collections::BTreeMap::new();
    for field in [false, true] {
        for n in 0..248u8 {
            let at = binding(n).at;
            if at == Where::Vm {
                continue;
            }
            let mut w = if field { field_world(f) } else { world(f) };
            let mut s = StoryState::default();
            let sig = wf_script::natives::sig(n);
            // arguments: actor 0 (the leader) / small values; shops need an entry
            let names = wf_script::natives::info(n).map(|i| i.args.clone()).unwrap_or_default();
            let args: Vec<i32> = (0..sig.pops as usize).map(|k| if names.get(k).is_some_and(|a| a.contains("actor")) { 0 } else { 1 }).collect();
            let mut p = [0; 4];
            let mut g = Globals::default();
            let r = wf_script::vm::Host::native(
                &mut CoreHost { w: &mut w, s: &mut s },
                wf_script::vm::Call { native: n, args: &args, script: wf_script::vm::ScriptKey::Inline(0), pc: 0, params: &mut p, globals: &mut g },
            );
            match &r {
                Ok(wf_script::vm::NativeResult::Done(v)) => {
                    if n != 0x8c {
                        assert_eq!(v.is_some(), sig.pushes, "0x{n:02x} result arity");
                    }
                }
                Ok(wf_script::vm::NativeResult::Block) => {
                    let mut frames = 0;
                    loop {
                        frames += 1;
                        if frames >= 200_000 {
                            // play_anim of a looping state never ends: the game hangs the same way (micro play_anim_loop)
                            assert!(n == 0x3c, "0x{n:02x} (field {field}) never completes: {:?}", s.pending);
                            break;
                        }
                        let held = if frames % 2 == 0 { A } else { wf_core::input::B };
                        let mut sess = Session { data: f.data.clone(), world: w, config: BattleConfig::default(), prev: [0; 4], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
                        let mut inputs = [InputFrame::default(); 4];
                        inputs[0] = InputFrame::from_held(held, 0);
                        sess.step_frames(inputs);
                        w = sess.world;
                        if let Some(v) = poll(&mut w, &mut s) {
                            assert_eq!(v.is_some(), sig.pushes && n != 0x8c || (n == 0x8c && v.is_some()), "0x{n:02x} resume arity");
                            break;
                        }
                    }
                }
                // an inline engine hook (give_xp's level-up #3): the event must exist
                Ok(wf_script::vm::NativeResult::RunEvent(ev)) => {
                    assert!(f.set.events.contains_key(ev), "0x{n:02x} runs missing event {ev}");
                    assert!(!sig.pushes, "0x{n:02x} runs an event but pushes a result");
                }
                Err(e) => panic!("0x{n:02x} {} (field {field}): {e}", wf_script_core::binding::name(n)),
            }
            if !field {
                *count.entry(format!("{at:?}")).or_insert(0) += 1;
            }
        }
    }
    eprintln!("bindings: {count:?}");
}
