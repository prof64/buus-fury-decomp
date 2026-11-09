//! Coverage recorder, the world's side (diagnostics; off unless `WF_COVERAGE` is set - `wf_view::coverage`): what a
//! world holds when a picture of it is made. Read-only: the sim never looks at it.

pub use wf_view::coverage::{at, commit, context, flush_counts, level, note, on};

use crate::world::{Body, World};

/// called by `view::render_view`: the world's content for the picture being built (`sample`) and what its actors are
/// doing (`note`: AI states, skills, player states - also those that ran between two compared pictures, when the
/// harness builds the view every frame)
/// the state comparison's frame (the segment harness, level `state`): its world as a view, committed
pub fn commit_world(w: &World) {
    let mut v = wf_view::RenderView::default();
    crate::view::render_view(w, &mut v);
    commit(&v);
}

pub fn world(w: &World) {
    if !on() {
        return;
    }
    let mut s: Vec<(&'static str, String)> = vec![];
    if let Some(f) = &w.field {
        s.push(("area", format!("{}-{}", f.area.world, f.area.area)));
        s.push(("map", f.map.clone()));
        if f.flight.is_some() {
            s.push(("screen", "WorldMapFlight".into()));
        }
    } else {
        s.push(("arena", w.arena.map.clone()));
    }
    for e in w.ents.iter().filter(|e| !e.removed) {
        match &e.body {
            Body::Player(p) => {
                s.push(("form", format!("{}", p.form)));
                note("player_state", format!("{:?}", p.state));
                if let Some(h) = p.hold_kind {
                    note("player_hold", format!("{h:?}"));
                }
                if let Some(sk) = &p.sk {
                    note("skill", format!("{}:{}", p.form, sk.id));
                    note("skill_any_form", format!("{}", sk.id));
                }
                if matches!(p.state, crate::player::PState::Transform) {
                    note("transform", format!("{}>{}", p.form, p.transform_to));
                }
            }
            Body::Enemy(en) => {
                s.push(("enemy", format!("{}", en.ty)));
                for st in &en.ai.stack {
                    note("enemy_state", format!("{}:{}", en.ty, st.state));
                }
            }
            Body::Npc(_) | Body::MapSprite(_) | Body::Injector(_) => {}
            _ => s.push(("object", e.kind().to_string())),
        }
    }
    for v in &w.scene.visuals {
        let k = format!("{:?}", v.kind);
        s.push(("visual", k.split([' ', '(', '{']).next().unwrap_or("?").to_string()));
    }
    if let Some(top) = w.menu.stack.last() {
        use crate::menu::Screen;
        let k = match top {
            Screen::Status(_) => "Status".to_string(),
            Screen::Stub(p) => format!("Stub{}", p.page),
            Screen::Inventory(_) => "Inventory".into(),
            Screen::Equip(_) => "Equip".into(),
            Screen::Options(_) => "Options".into(),
            Screen::Journal(_) => "Journal".into(),
            Screen::Skills(_) => "Skills".into(),
            Screen::Shop(p) => if p.sell { "ShopSell".into() } else { "ShopBuy".into() },
            Screen::Save(_) => "Save".into(),
            Screen::Title(p) => format!("Title{:?}", p.page).split([' ', '(', '{']).next().unwrap_or("Title").to_string(),
            Screen::CharSelect(_) => "CharSelect".into(),
            Screen::GameOver(_) => "GameOver".into(),
            Screen::Credits(_) => "Credits".into(),
            Screen::Minigame(p) => format!("Minigame{:?}", p.kind),
            Screen::MapScreen(p) => format!("MapScreen{:?}", p.page).split([' ', '(', '{']).next().unwrap_or("MapScreen").to_string(),
            Screen::Link(_) => "Link".into(),
        };
        s.push(("screen", k));
    }
    wf_view::coverage::sample(w.frame as u64, s);
}
