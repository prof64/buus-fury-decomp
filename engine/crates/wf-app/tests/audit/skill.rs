//! The skill audit's comparer, shared by its packer (`examples/skill_audit.rs`) and its gate (`tests/audits.rs`): one
//! emulator-recorded player-skill segment (work/skillaudit: `skillseg.py`, the Hyperbolic Time Chamber, no enemy)
//! replayed by the segment harness; state = the harness's comparison (actors, sounds, RNG ..), picture = our view
//! (+ the presenter's one-frame OBJ delay, as `examples/pixels.rs` with `PIX_ALL` / `PIX_NOPROPS`) at 240x160 against
//! the game's screen of every video frame.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use wf_app::wf_content::Packs;
use wf_render::Renderer;
use wf_view::RenderView;

pub struct Skill {
    pub frames: usize,
    /// the harness's first differing frame and the count of its differing comparisons
    pub state: Option<(usize, usize, String)>,
    /// the frames whose picture differs
    pub picture: Vec<usize>,
}

/// `game(k, ours)`: whether our picture of video frame `k` is the game's (`None`: no game picture of that frame)
pub fn check(data: &Arc<wf_core::content::GameData>, files: &BTreeMap<String, String>, packs: &Packs, renderer: &mut Renderer, doc: &Value, game: &mut dyn FnMut(usize, &[u32]) -> Option<bool>) -> Skill {
    let mut presenter = wf_core::view::Presenter::default();
    let mut picture = vec![];
    let rep = wf_import::segment::replay_with(data, files, doc, &mut |f| match f {
        wf_import::segment::Frame::Before(_, w) => presenter.before_step(w),
        wf_import::segment::Frame::After(k, w) => {
            let mut view = RenderView::default();
            wf_core::view::render_view(w, &mut view);
            presenter.apply(&mut view);
            // without the map's prop sprites and BG0 (the Time Chamber's mirror): the warped-in start states have none
            view.sprites.retain(|s| !s.sheet.starts_with("effects/props_"));
            if let Some(m) = view.map.as_mut() {
                m.layers.get_or_insert_with(|| wf_view::BgLayers { visible: [true; 4], ..Default::default() }).visible[0] = false;
            }
            if game(k, renderer.render(packs, &view, 0, 240, 160)) == Some(false) {
                picture.push(k);
            }
        }
    });
    let n: usize = rep.counts.values().sum();
    Skill { frames: rep.frames, state: rep.first.map(|(k, m)| (k, n.max(1), m)), picture }
}

/// the recording without its object-tile allocator rows (the audit ran with `SEGMENT_VRAM=0`: the warped-in start
/// states' allocator is not the import's)
pub fn strip_vram(doc: &mut Value) {
    if let Some(o) = doc["init"].as_object_mut() {
        o.remove("vram");
    }
    for f in doc["frames"].as_array_mut().into_iter().flatten() {
        if let Some(o) = f.as_object_mut() {
            o.remove("vram");
        }
    }
}
