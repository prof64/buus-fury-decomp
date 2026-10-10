import os
D = '/Users/prof/source/buus-fury-decomp/engine/crates/wf-core/src/menu/'
p = D + 'mod.rs'
s = open(p).read()
a = s[s.index("/// the pause menu's pages and the other screens (one variant per mode class)"):s.index("/// the pause page L (`right` = false) / R of page `from`")]
b = '''/// One menu mode (a page or screen). Every screen is plain data (`Clone + Hash + Serialize`, postcard-safe: no
/// tagged / untagged enums, no `skip_serializing_if`), stepped only through `step` with `World.menu`'s input.
pub trait Page {
    /// the page's widget container (screens drawn without widgets keep an empty one: its `drawn` flag is the
    /// redraw request)
    fn container(&mut self) -> &mut kit::Container;
    fn container_ref(&self) -> &kit::Container;
    /// the mode's update before the redraw check (vt +0xC prologue: counters that request redraws)
    fn update(&mut self, _w: &World) {}
    /// draw-time side effects and values sampled when the surface is drawn (kept in the shown copy)
    fn before_draw(&mut self, _w: &World) {}
    /// the mode's step (input, logic, done check)
    fn step(&mut self, w: &mut World) -> Action;
    /// a nested mode this one ran returned `r`
    fn child_result(&mut self, _w: &mut World, _r: i32) -> Action {
        Action::None
    }
    /// the screen as drawn from this state (pure)
    fn view(&self, w: &World, d: &MenuData) -> MenuView;
    /// video frames one redraw takes (CPU time; `re/buu/menu/timing.py`)
    fn redraw_cost(&self) -> u8 {
        1
    }
}

macro_rules! screens {
    ($($v:ident($t:ty)),* $(,)?) => {
        /// the menu modes (one variant per mode class of the game)
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum Screen {
            $($v($t)),*
        }
        impl Screen {
            pub fn page(&self) -> &dyn Page {
                match self {
                    $(Screen::$v(p) => p),*
                }
            }
            pub fn page_mut(&mut self) -> &mut dyn Page {
                match self {
                    $(Screen::$v(p) => p),*
                }
            }
        }
    };
}

screens! {
    Status(status::Status),
    /// a page not implemented: the skeleton with its title, L / R and B work
    Stub(kit::StubPage),
    Inventory(inventory::Inventory),
    Equip(equip::Equip),
    Options(options::Options),
    Journal(journal::Journal),
    Skills(skills::Skills),
    Shop(shop::Shop),
    Save(save::Save),
    Title(title::Title),
    CharSelect(charsel::CharSelect),
    GameOver(gameover::GameOver),
    Credits(credits::Credits),
    Minigame(minigame::Minigame),
    MapScreen(mapscreen::MapScreen),
}

impl Screen {
    pub fn container(&mut self) -> &mut kit::Container {
        self.page_mut().container()
    }
    pub fn container_ref(&self) -> &kit::Container {
        self.page().container_ref()
    }
    fn update(&mut self, w: &World) {
        self.page_mut().update(w)
    }
    fn before_draw(&mut self, w: &World) {
        self.page_mut().before_draw(w)
    }
    fn step(&mut self, w: &mut World) -> Action {
        self.page_mut().step(w)
    }
    fn child_result(&mut self, w: &mut World, r: i32) -> Action {
        self.page_mut().child_result(w, r)
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        self.page().view(w, d)
    }
    fn redraw_cost(&self) -> u8 {
        self.page().redraw_cost()
    }
}

'''
s = s.replace(a, b)
a = '''    match to {
        0 => Screen::Status(status::Status::new(w)),
        _ => Screen::Stub(kit::StubPage::new(w, to)),
    }
}

/// the Skills page of a character (`0x08017160`), run as a nested mode of the Status page
pub fn skills_screen(w: &World, _ci: i32) -> Screen {
    Screen::Stub(kit::StubPage::new(w, 0))
}'''
b = '''    match to {
        0 => Screen::Status(status::Status::new(w)),
        1 => Screen::Inventory(inventory::Inventory::new(w)),
        2 => Screen::Equip(equip::Equip::new(w)),
        3 => Screen::Options(options::Options::new(w)),
        _ => Screen::Journal(journal::Journal::new(w)),
    }
}

/// the Skills page of a character (`0x08017160`), run as a nested mode of the Status page
pub fn skills_screen(w: &World, ci: i32) -> Screen {
    Screen::Skills(skills::Skills::new(w, ci))
}'''
assert a in s
s = s.replace(a, b)
s = s.replace('''pub mod kit;
pub mod status;
pub mod text;
''', '''pub mod charsel;
pub mod credits;
pub mod equip;
pub mod gameover;
pub mod inventory;
pub mod journal;
pub mod kit;
pub mod mapscreen;
pub mod minigame;
pub mod options;
pub mod save;
pub mod shop;
pub mod skills;
pub mod status;
pub mod text;
pub mod title;
''')
open(p, 'w').write(s)

# status.rs: implement Page
p = D + 'status.rs'
s = open(p).read()
s = s.replace("use super::{Action, MenuData};", "use super::{Action, MenuData, Page};")
s = s.replace('''    /// draw-time side effects of the tab bar (`0x080160CA`): layout, the arrows' visibility / state
    pub fn prepare(&mut self) {''', '''    /// draw-time side effects of the tab bar (`0x080160CA`): layout, the arrows' visibility / state
    pub fn prepare_tabs(&mut self) {''')
s = s.replace("        s.prepare();\n", "        s.prepare_tabs();\n")
s = s.replace('''    /// update (vt +0xC, before the redraw check): the blink counter; every 8 updates with points a redraw
    pub fn update(&mut self, w: &World) {''', '''    /// update (vt +0xC, before the redraw check): the blink counter; every 8 updates with points a redraw
    fn update_blink(&mut self, w: &World) {''')
s = s.replace('''    pub fn step(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(''', '''    fn step_page(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(''')
s = s.replace('''    /// the Skills sub-page closed with `r`
    pub fn child_result(&mut self, w: &mut World, r: i32) -> Action {''', '''    /// the Skills sub-page closed with `r`
    fn skills_closed(&mut self, w: &mut World, r: i32) -> Action {''')
s = s.replace('''    /// redraw `0x080154D4`
    pub fn view(&self, w: &World, d: &MenuData) -> MenuView {''', '''    /// redraw `0x080154D4`
    fn draw(&self, w: &World, d: &MenuData) -> MenuView {''')
s += '''
impl Page for Status {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn update(&mut self, w: &World) {
        self.update_blink(w)
    }
    fn before_draw(&mut self, w: &World) {
        self.prepare_tabs();
        self.time = w.story.play_frames;
    }
    fn step(&mut self, w: &mut World) -> Action {
        self.step_page(w)
    }
    fn child_result(&mut self, w: &mut World, r: i32) -> Action {
        self.skills_closed(w, r)
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        self.draw(w, d)
    }
    fn redraw_cost(&self) -> u8 {
        REDRAW_FRAMES
    }
}
'''
open(p, 'w').write(s)

# kit.rs StubPage implements Page
p = D + 'kit.rs'
s = open(p).read()
s = s.replace('''    pub fn step(&mut self, w: &mut crate::world::World) -> super::Action {
        let (consumed, act) = run_step(self, w, |p| &mut p.c, |p, w, _src, code| match code {''', '''    pub fn step_stub(&mut self, w: &mut crate::world::World) -> super::Action {
        let (consumed, act) = run_step(self, w, |p| &mut p.c, |p, w, _src, code| match code {''')
s = s.replace('''    pub fn view(&self, w: &crate::world::World, d: &super::MenuData) -> wf_view::MenuView {
        let _ = w;
        let mut cv = Canvas::new();''', '''    pub fn draw_stub(&self, w: &crate::world::World, d: &super::MenuData) -> wf_view::MenuView {
        let _ = w;
        let mut cv = Canvas::new();''')
s += '''
impl super::Page for StubPage {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn step(&mut self, w: &mut crate::world::World) -> super::Action {
        self.step_stub(w)
    }
    fn view(&self, w: &crate::world::World, d: &super::MenuData) -> wf_view::MenuView {
        self.draw_stub(w, d)
    }
}
'''
open(p, 'w').write(s)

STUB = '''//! {doc}
//!
//! Not implemented yet: a stand-in page (the pause-page skeleton, B closes).

use super::kit::{{Container, StubPage}};
use super::{{Action, MenuData, Page}};
use crate::world::World;
use serde::{{Deserialize, Serialize}};
use wf_view::MenuView;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct {name} {{
    pub stub: StubPage,
}}

impl {name} {{
    pub fn new({args}) -> {name} {{
        {name} {{ stub: StubPage::new(w, {page}) }}
    }}
}}

impl Page for {name} {{
    fn container(&mut self) -> &mut Container {{
        &mut self.stub.c
    }}
    fn container_ref(&self) -> &Container {{
        &self.stub.c
    }}
    fn step(&mut self, w: &mut World) -> Action {{
        self.stub.step_stub(w)
    }}
    fn view(&self, w: &World, d: &MenuData) -> MenuView {{
        self.stub.draw_stub(w, d)
    }}
}}
'''
mods = [
    ('inventory', 'Inventory', 'The Inventory page (Supplies / Story Items; item use runs the supply\'s script).', 'w: &World', 1),
    ('equip', 'Equip', 'The Equip page (body / hands / feet / accessory; RecalcStats).', 'w: &World', 2),
    ('options', 'Options', 'The Options page (sound FX / music volume, text speed, Credits, Sleep).', 'w: &World', 3),
    ('journal', 'Journal', 'The Journal page (goals / completed goals of the 40 quests).', 'w: &World', 4),
    ('skills', 'Skills', 'The Skills sub-page of Status (`0x08017160`), a nested mode.', 'w: &World, _ci: i32', 0),
    ('shop', 'Shop', 'The shop screens (`open_shop_buy` 0xCE / `open_shop_sell` 0xCF over the `shop_add_item` 0xCD list).', 'w: &World', 0),
    ('save', 'Save', 'The Save Game screen (`save_menu` 0x8C) and the save slots.', 'w: &World', 0),
    ('title', 'Title', 'The title screen and the New Game / Continue / Delete flow.', 'w: &World', 0),
    ('charsel', 'CharSelect', 'The Switch Character screen (`character_menu` 0x8D).', 'w: &World', 0),
    ('gameover', 'GameOver', 'Game over / continue.', 'w: &World', 0),
    ('credits', 'Credits', 'The staff credits (`run_credits` 0xD1, Options -> Credits).', 'w: &World', 0),
    ('minigame', 'Minigame', 'The mini-games: Watch This! (0xA7), the power struggle (0xC6), the punch gauge (0xC7).', 'w: &World', 0),
    ('mapscreen', 'MapScreen', 'The map screen (SELECT in the field).', 'w: &World', 0),
]
for f, name, doc, args, page in mods:
    path = D + f + '.rs'
    if os.path.exists(path):
        continue
    open(path, 'w').write(STUB.format(doc=doc, name=name, args=args, page=page))
print('ok')
