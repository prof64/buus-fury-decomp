//! Game over / continue.
//!
//! Not implemented yet: a stand-in page (the pause-page skeleton, B closes).

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
        GameOver { stub: StubPage::new(w, 0) }
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
pub fn open_json(_w: &World, _v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    None
}
