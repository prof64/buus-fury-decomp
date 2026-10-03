//! The title screen and the New Game / Continue / Delete flow.
//!
//! Not implemented yet: a stand-in page (the pause-page skeleton, B closes).

use super::kit::{Container, StubPage};
use super::{Action, MenuData, Page};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::MenuView;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Title {
    pub stub: StubPage,
}

impl Title {
    pub fn new(w: &World) -> Title {
        Title { stub: StubPage::new(w, 0) }
    }
}

impl Page for Title {
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
