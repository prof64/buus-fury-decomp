//! The mod checker (`wf_app::modcheck`, engine/docs/modding.md §7) on small broken mods (`tests/modcheck_fixtures/`,
//! one problem each) and on the example mods: each problem is reported with its file, what is wrong and a hint; a
//! good mod passes. Headless. Skipped without `roms/buu.gba`.

use std::path::PathBuf;
use std::sync::OnceLock;

/// the full base pack, extracted once into the cargo tmp dir (`WF_FULL_CONTENT=<content/base>` uses an existing one)
fn base_dir() -> Option<PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if let Ok(d) = std::env::var("WF_FULL_CONTENT") {
            return Some(PathBuf::from(d));
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-mods-content");
        if wf_extract::cache::fresh(&dir, &[]) {
            return Some(dir);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let rom = wf_rom::testutil::rom()?;
        let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
        for (p, b) in &c.files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
        Some(dir)
    })
    .clone()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/modcheck_fixtures").join(name)
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods-examples").join(name)
}

/// the one problem a fixture must produce: (file, words in `what`, words in `hint`)
fn expect_one(name: &str, file: &str, what: &str, hint: &str) {
    let Some(base) = base_dir() else { return };
    let r = wf_app::modcheck::check(&base, &[fixture(name)]);
    assert!(!r.ok(), "{name}: no problem reported\n{}", r.text());
    let hit = r.errors.iter().find(|e| e.file.contains(file) && e.what.contains(what));
    let Some(e) = hit else { panic!("{name}: expected {file}: {what}\n{}", r.text()) };
    assert!(e.hint.contains(hint), "{name}: hint {:?} lacks {hint:?}", e.hint);
}

#[test]
fn broken_json_is_reported_with_its_position() {
    expect_one("bad_json", "bad_json/enemies/majin_fighter.patch.json", "not valid JSON: key must be a string at line 1 column 12", "syntax");
}

#[test]
fn a_call_to_an_unknown_script_function_is_reported_with_its_line() {
    expect_one("rhai_unknown_native", "rhai_unknown_native/scripts/events/s931.rhai:2", "give_superpowers()", "docs/scripting.md");
}

#[test]
fn a_new_skill_reusing_a_base_number_is_an_id_clash() {
    expect_one("id_clash_base", "id_clash_base/skills/24_x.json", "`skill` 7 is already used by base/skills/07_energy_blast.json", "24..255");
}

#[test]
fn a_bad_recolour_colour_is_reported() {
    expect_one("derive_bad_colour", "icons/skills/skill_07.png.derive.json", "\"#zz00zz\" is not a colour", "#rrggbb");
}

#[test]
fn a_skill_without_hud_icons_gets_a_derive_hint() {
    expect_one("skill_no_icon", "skill 24", "missing HUD icon icons/skills/skill_24.png", "skill_24.png.derive.json");
}

#[test]
fn an_item_sold_in_a_missing_or_non_shop_event_is_reported() {
    expect_one("item_sold_at_bad", "items/090_mod_tea.json", "no event scripts/events/s999.rhai", "shop events");
    expect_one("item_sold_at_bad", "items/090_mod_tea.json", "s110: that event is not a shop", "Purpose: shop");
}

#[test]
fn a_supply_that_does_nothing_is_reported() {
    expect_one("item_unusable", "items/090_mod_tea.json", "neither `use_script` nor an `effect`", "restore_hp");
}

#[test]
fn a_dialogue_override_of_a_non_text_node_is_reported() {
    expect_one("text_bad_node", "text_bad_node/scripts/events/s028.text.json", "node 0 is not a say / choice node", "");
}

#[test]
fn the_example_mods_pass() {
    let Some(base) = base_dir() else { return };
    let mods: Vec<PathBuf> = ["balance-tweak", "hud-layout", "new-area", "new-arena-scenario", "new-boss", "new-character", "new-party-member", "new-skill", "scripted-skills"]
        .iter()
        .map(|m| example(m))
        .collect();
    let r = wf_app::modcheck::check(&base, &mods);
    assert!(r.ok(), "{}", r.text());
}

/// the unknown-call scan has no false positives on the game's own 1,296 scripts
#[test]
fn the_base_scripts_call_only_known_functions() {
    let Some(base) = base_dir() else { return };
    let packs = wf_app::mods::load(&base, &[]).unwrap();
    let files: Vec<(String, String)> = packs
        .list()
        .into_iter()
        .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
        .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
        .collect();
    let api = wf_script_rhai::api_names();
    let defined: std::collections::BTreeSet<String> = files.iter().flat_map(|(_, s)| wf_script_rhai::defined_fns(s)).collect();
    let bad: Vec<String> = files
        .iter()
        .flat_map(|(p, s)| wf_script_rhai::unknown_calls(s, &defined, &api).into_iter().map(move |(l, n)| format!("{p}:{l} {n}")))
        .collect();
    assert!(bad.is_empty(), "{} unknown calls, e.g. {:?}", bad.len(), &bad[..bad.len().min(10)]);
}
