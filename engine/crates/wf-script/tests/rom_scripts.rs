//! ROM-backed tests (skipped when roms/buu.gba is absent).
use wf_script::rom::{Entry, NodeBody};
use wf_script::{decompile, ScriptId, ScriptSet};

#[test]
fn all_events_and_scripts_load_and_inflate_like_the_game() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let set = ScriptSet::load_all(&rom);
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    assert_eq!(set.events.len(), 522);
    // 1766 bytecode nodes reachable in events + 692 catalogued inline scripts + 82 enemy drop scripts
    assert_eq!(set.scripts.len(), 2540);
    let texts = set
        .events
        .values()
        .flat_map(|e| e.entries.values())
        .filter(|e| matches!(e, Entry::Node(n) if matches!(n.body, NodeBody::Text { .. } | NodeBody::TextArgs { .. })))
        .count();
    assert!(texts > 2000, "{texts}");
    // compressed scripts: compare with the game's own decompressor output cached by re/buu/decomp.py
    let cache = wf_rom::testutil::repo_root().join("work/buu/decomp");
    let mut checked = 0;
    for sc in set.scripts.values() {
        let Some(src) = sc.stream_addr else { continue };
        let Ok(game) = std::fs::read(cache.join(format!("{src:08x}.bin"))) else { continue };
        assert!(game.len() >= sc.code.len(), "{:?}", sc.id);
        assert_eq!(&game[..sc.code.len()], &sc.code[..], "{:?}", sc.id);
        checked += 1;
    }
    eprintln!("compressed scripts checked against emulator output: {checked}");
    // event 450 (Janemba -> Super Janemba) text node
    let e = &set.events[&450];
    assert!(matches!(&e.entries[&2], Entry::Node(n) if matches!(&n.body, NodeBody::Text { text, .. } if text.contains("Kakarot"))));
    assert!(set.get(ScriptId::Node { event: 497, node: 0 }).is_some());
}

/// Every unit decompiles; native stack effects are consistent with all 2458 units (a wrong arity would
/// make the stack depths disagree at a join); every unit is structured.
#[test]
fn every_script_decompiles_structured() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let set = ScriptSet::load_all(&rom);
    let mut unstructured = vec![];
    for sc in set.scripts.values() {
        let d = decompile::decompile(sc, &sc.id.slug()).unwrap_or_else(|e| panic!("{}: {e}", sc.id.slug()));
        if let Some(e) = d.structure_error {
            unstructured.push(format!("{}: {e}", sc.id.slug()));
        }
    }
    assert!(unstructured.is_empty(), "{unstructured:?}");
}
