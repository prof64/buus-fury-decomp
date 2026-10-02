//! Renderer vs emulator screenshots (headless; skipped without the ROM or without the captures from
//! `crates/wf-render/re/capture.py` in `<repo>/work/frontend/`). Map layers, tile animations, the player sprite
//! with halo and shadow, tree-top blending over the character, the HUD and the dialogue box must be pixel-exact.
//! Scenes with other actors (enemies / NPCs are not rebuilt here) must match everywhere outside their OBJs.

#[allow(dead_code)]
#[path = "../examples/parity.rs"]
mod parity;

#[test]
fn renderer_matches_emulator() {
    let work = std::env::var("WF_PARITY_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/frontend"));
    if !work.is_dir() {
        return;
    }
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let opts = wf_extract::Options { only: ["sprites", "effects", "items", "maps", "ui"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let packs = wf_app::wf_content::Packs::new(vec![Box::new(wf_app::wf_content::MemSource(c.files)) as Box<dyn wf_app::wf_content::FileSource>]).unwrap();
    let res = parity::run(&packs, &work, None, &[]);
    assert!(!res.is_empty());
    for r in &res {
        let actors = r.name.starts_with("fight") || r.name.starts_with("snakeway") || r.name.starts_with("hud_snake");
        assert_eq!(r.hud, 0, "{}: HUD", r.name);
        assert_eq!(r.background, 0, "{}: background", r.name);
        if !actors {
            assert_eq!(r.total, 0, "{}: whole frame", r.name);
        }
    }
}
