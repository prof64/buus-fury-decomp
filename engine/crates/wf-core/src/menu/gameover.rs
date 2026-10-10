//! Game over. Buu's Fury has no game-over / continue screen: the leader's KO (`0x0801070E`) queues a scene timer
//! of 200 frames whose callback (`0x08010588`) reverts a temporary form (form flag 0x40 → its `down` form), resets
//! the sprite and runs the engine hook script #0 (`0x08012588`, event `s000`): with a Lazarus Crystal (supply 23)
//! the leader revives at 25 % HP ("The magic of the Lazarus Crystal restores your life-force!"); otherwise the death
//! music, a fade to the death scene (world 0, area 2), "You have died!" and `return_to_title` (0x8E: the session
//! restarts on the title, [`super::title::title_world`]). Flags 281..283 route special deaths (story fights).
//! [`tick`] starts the hook at KO + 199 (the frame the battle engine's death script blocks the object updates,
//! `player.rs`); not frame-checked against a capture.
//!
//! [`GameOver`] stays a stand-in page (no such screen in the game; nothing opens it).

use super::kit::{Container, StubPage};
use super::{Action, MenuData, Page};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::MenuView;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameOver {
    pub stub: StubPage,
}

impl GameOver {
    pub fn new(w: &World) -> GameOver {
        GameOver {
            stub: StubPage::new(w, 0),
        }
    }
}

impl Page for GameOver {
    fn container(&mut self) -> &mut Container {
        &mut self.stub.c
    }
    fn container_ref(&self) -> &Container {
        &self.stub.c
    }
    fn step(&mut self, w: &mut World) -> Action {
        self.stub.step_stub(w)
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        self.stub.draw_stub(w, d)
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`); None = not this screen's kind
pub fn open_json(
    _w: &World,
    _v: &serde_json::Value,
) -> Option<(super::Screen, super::Transition, bool)> {
    None
}

/// the KO timer: the hook script starts KO + 199 (`0x0801070E`: a 200-frame timer ticked from the next frame)
pub const DEATH_DELAY: i64 = 199;

/// The field session's per-frame death check (`Session::step_update`): the single-player leader's KO timer ran out
/// → a temporary form reverts and the engine hook script #0 runs.
pub fn tick(w: &mut World) {
    if w.field.is_none() || w.players.len() != 1 {
        return;
    }
    let Some(i) = w.player_idx(0) else { return };
    let (dead, ko) = (w.pl_ref(i).dead, w.pl_ref(i).ko_at);
    // (not under a `return_to_title` already asked for: a harness gives the game over's that way)
    if !dead && !w.scene.requests.iter().any(|r| matches!(r, crate::scene::Request::ReturnToTitle)) {
        w.game_over = false;
    }
    if !dead || ko < 0 || w.frame as i64 - ko != DEATH_DELAY {
        return;
    }
    w.game_over = true;
    let form = w.pl_ref(i).form;
    let to = w
        .data
        .form(form)
        .filter(|f| f.flags & 0x40 != 0 && w.data.form(f.down).is_some())
        .map(|f| f.down)
        .unwrap_or(form);
    let mut pl = w.pl_ref(i).clone();
    pl.apply_form(w, to);
    *w.pl(i) = pl;
    w.events
        .push(crate::world::Event::Field(crate::field::FieldEvent::Run {
            run: crate::fieldcontent::Callback::Event("s000".into()),
            cause: crate::field::Cause::Menu,
            slot: 0,
        }));
}
