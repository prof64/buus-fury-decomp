//! The Journal page (goals / completed goals of the 40 quests).
//!
//! Not implemented yet: a stand-in page (the pause-page skeleton, B closes).

use super::kit::{Container, StubPage};
use super::{Action, MenuData, Page};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::MenuView;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Journal {
    pub stub: StubPage,
}

impl Journal {
    pub fn new(w: &World) -> Journal {
        Journal { stub: StubPage::new(w, 4) }
    }
}

impl Page for Journal {
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
