//! Menu screens as part of the simulation: the pause menu and its pages, shops, save / load, character switch,
//! title, game over, credits, mini-games, map screen (`engine/docs/menus.md`, RE notes `re/buu/menu/NOTES.md`).
//!
//! The game runs each of these as a *mode* (`run_mode 0x0803D8A0`): a loop of `update` (redraw the 8bpp
//! surface if dirty and copy it to VRAM) → VBlank wait → input read (`0x0803D4A0`, edges between reads) →
//! `step` (widgets' input, page logic, done check). The world is saved (`0x08011DB4`) and frozen while a menu runs.
//! Everything here is plain data in `World.menu`, stepped by the leader's input only, hashed and snapshotted with
//! the world, so replays / rollback / netplay stay exact.
//!
//! Timing that comes from CPU time on the GBA is modelled per screen: a redraw that takes longer than a frame
//! makes the loop skip whole frames (`Menu.lag`): no logic runs and the input is not read in those frames (a short
//! tap there is lost, like on the GBA). The screen shows the last copied surface (`Menu.shown`).
//!
//! Multiplayer: the menu belongs to the leader (player slot 0); everybody sees the same screen and the world
//! pauses for all players (like the GBA game, which has one menu). See `docs/menus.md` "Netplay".

pub mod charsel;
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
pub mod scouter;
pub mod shop;
pub mod skills;
pub mod status;
pub mod text;
pub mod title;

use crate::world::World;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use text::TextData;
use wf_view::MenuView;

/// static menu content (`GameData.menu`)
#[derive(Clone, Debug, Default)]
pub struct MenuData {
    pub text: TextData,
    /// portraits/index.json: dialog portrait table index -> content path
    pub portraits: Vec<String>,
    /// per form (form table `0x086B82EC`, 24 bytes): name string id (+4), portrait bytes (+0x10..+0x15)
    pub forms: Vec<FormUi>,
    /// supplies' and equipment's menu data (names, descriptions, icons, prices, slots): shop.rs
    pub items: shop::ItemsUi,
    /// the skill table's and the quest table's menu data (`ui/menu/opts.json`): options.rs
    pub opts: options::OptsData,
    /// supplies' inventory data (def +4 name, +6 plural, +8 description, icon, story flag, use script): inventory.rs
    pub inv: inventory::InvData,
    /// the map screen's area records (`ui/mapscreen/area_maps.json`): mapscreen.rs
    pub maps: Option<Arc<mapscreen::MapData>>,
}

#[derive(Clone, Debug, Default)]
pub struct FormUi {
    pub name_str: u16,
    pub name: Vec<u16>,
    /// +4: the upper-case name (Status page)
    pub upper: Vec<u16>,
    pub portraits: [u8; 6],
}

impl MenuData {
    pub fn load(src: &dyn crate::content::ContentSource) -> Option<Arc<MenuData>> {
        let text = TextData::load(src)?;
        let mut portraits = Vec::new();
        if let Some(b) = src.read("portraits/index.json") {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) {
                if let Some(a) = v["portraits"].as_array().or_else(|| v.as_array()) {
                    for p in a {
                        let i = p["portrait_index"].as_u64().or_else(|| p["index"].as_u64()).unwrap_or(portraits.len() as u64) as usize;
                        let f = p["png"].as_str().or_else(|| p["file"].as_str()).unwrap_or("").to_string();
                        if portraits.len() <= i {
                            portraits.resize(i + 1, String::new());
                        }
                        portraits[i] = if f.starts_with("portraits/") { f } else { format!("portraits/{f}") };
                    }
                }
            }
        }
        let mut forms = Vec::new();
        let all = src.list();
        for p in all.iter().filter(|p| p.starts_with("forms/") && p.ends_with(".json") && !p.ends_with("index.json")) {
            let Some(b) = src.read(p) else { continue };
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) else { continue };
            let i = v["index"].as_u64().unwrap_or(0) as usize;
            if forms.len() <= i {
                forms.resize(i + 1, FormUi::default());
            }
            let mut pb = [0u8; 6];
            if let Some(a) = v["portrait_or_anim_bytes"].as_array() {
                for (k, x) in a.iter().take(6).enumerate() {
                    pb[k] = x.as_u64().unwrap_or(0) as u8;
                }
            }
            // form +4: the upper-case name (content has its text; mod forms without one: the name upper-cased)
            let name = v["name"].as_str().unwrap_or("");
            let upper = v["upper_name"].as_str().map(text::units).unwrap_or_else(|| text::units(&name.to_uppercase()));
            let name_str = v["name_string"].as_u64().unwrap_or(0) as u16;
            forms[i] = FormUi { name_str, name: text::units(name), upper, portraits: pb };
        }
        let items = shop::ItemsUi::load(src);
        let opts = options::OptsData::load(src);
        let inv = inventory::InvData::load(src);
        Some(Arc::new(MenuData { text, portraits, forms, items, opts, inv, maps: mapscreen::MapData::load(src) }))
    }
}

/// One menu mode (a page or screen). Every screen is plain data (`Clone + Hash + Serialize`, postcard-safe: no
/// tagged / untagged enums, no `skip_serializing_if`), stepped only through `step` with `World.menu`'s input.
pub trait Page {
    /// the page's widget container (screens drawn without widgets keep an empty one: its `drawn` flag is the
    /// redraw request)
    fn container(&mut self) -> &mut kit::Container;
    fn container_ref(&self) -> &kit::Container;
    /// the mode's update before the redraw check (vt +0xC prologue: counters that request redraws)
    fn update(&mut self, _w: &World) {}
    /// the screen's live OBJs (character objects updated every iteration, independent of the BG redraw):
    /// shown over the last copied picture
    fn objs(&self, _w: &World) -> Vec<wf_view::MenuObj> {
        Vec::new()
    }
    /// the closing iteration's frame is already black (Switch Character), not the page
    fn leave_black(&self) -> bool {
        false
    }
    /// draw-time side effects and values sampled when the surface is drawn (kept in the shown copy)
    fn before_draw(&mut self, _w: &World) {}
    /// draw-time side effects the game applies after the part of the surface that shows them (the state shown
    /// keeps the old values; the live state takes the new ones)
    fn after_draw(&mut self, _w: &World) {}
    /// the mode's step (input, logic, done check)
    fn step(&mut self, w: &mut World) -> Action;
    /// a nested mode this one ran returned `r`
    fn child_result(&mut self, _w: &mut World, _r: i32) -> Action {
        Action::None
    }
    /// the scripts this screen started in its step (`FieldEvent::Run`, run by the host after the step: item use) are
    /// done: apply what the game does after them in the same iteration (see [`post`])
    fn after_scripts(&mut self, _w: &World) {}
    /// the screen as drawn from this state (pure)
    fn view(&self, w: &World, d: &MenuData) -> MenuView;
    /// video frames one redraw takes (CPU time; `re/buu/menu/timing.py`)
    fn redraw_cost(&self) -> u8 {
        1
    }
    /// frames the mode's enter + first update take beyond the first one (its picture shows at once; no
    /// iteration in them): credits / Watch This! 1
    fn enter_lag(&self) -> u8 {
        0
    }
    /// a long redraw whose visible change happens at once (registers written before the slow drawing: the
    /// credits' page change at EVA 0): the new state shows in the redraw's first frame
    fn show_at_once(&self) -> bool {
        false
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
    fn after_draw(&mut self, w: &World) {
        self.page_mut().after_draw(w)
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

/// Open a screen described by JSON (test harnesses: `menu_parity.rs` opens script-started screens the frame the
/// emulator's script did): `{"kind": "...", ...}`, handled by the screen modules' `open_json`.
pub fn open_from_json(w: &mut World, v: &serde_json::Value) -> bool {
    let tries: [fn(&World, &serde_json::Value) -> Option<(Screen, Transition, bool)>; 13] = [
        inventory::open_json,
        equip::open_json,
        options::open_json,
        journal::open_json,
        skills::open_json,
        shop::open_json,
        save::open_json,
        title::open_json,
        charsel::open_json,
        gameover::open_json,
        credits::open_json,
        minigame::open_json,
        mapscreen::open_json,
    ];
    for f in tries {
        if let Some((sc, tr, scripted)) = f(w, v) {
            // `"prev": {...}`: the screen was opened mid-redraw; this one's picture is on screen meanwhile
            let prev = v.get("prev").filter(|p| p.is_object()).and_then(|p| tries.iter().find_map(|g| g(w, p)));
            open(w, sc, tr, scripted);
            if let Some((mut ps, _, _)) = prev {
                ps.before_draw(w);
                w.menu.boot_shown = Some(Box::new(ps));
            }
            return true;
        }
    }
    false
}

/// the pause page L (`right` = false) / R of page `from` (Status 0, Inventory 1, Equip 2, Options 3, Journal 4)
pub fn page_screen(w: &World, from: u8, right: bool) -> Screen {
    let n = kit::PAGES.len() as u8;
    let to = if right { (from + 1) % n } else { (from + n - 1) % n };
    match to {
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
}

/// a single 240x160 BG surface from a display list (BG2, priority 0)
pub fn surface_view(cv: text::Canvas) -> MenuView {
    MenuView {
        layers: vec![wf_view::MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 0, ops: cv.ops }],
        objs: Vec::new(),
        palette: Vec::new(),
        backdrop: 0,
        fade: None,
        fade_black: 0,
        overlay: false,
        blend: None,
    }
}

/// where the menu is in its life
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    /// no menu
    #[default]
    Idle,
    /// the field fades to black (palette fade 0x08007074, level += 16 per frame; the world is drawn, not updated)
    FadeOut { level: u16 },
    /// black: the world is saved, the first page is built and drawn (frames of CPU time)
    Enter { t: u16 },
    /// the menu's mode loop runs
    Run,
    /// black: the world is restored (EnterArea-like reload, frames of CPU time)
    Leave { t: u16 },
    /// the field fades in (level -= 16 per frame)
    FadeIn { level: u16 },
}

/// What a closed menu leaves for its owner (the script host reads it for script-opened screens).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MenuResult {
    pub code: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Menu {
    pub phase: Phase,
    /// the running mode stack (pages replace each other; sub-pages like Skills nest)
    pub stack: Vec<Screen>,
    /// what is on screen: the state of the top screen at its last copy to VRAM
    pub shown: Option<Box<Screen>>,
    /// a redraw in progress (on screen when its lag frames end)
    pub pending: Option<Box<Screen>>,
    /// a picture copied before `pending` within the same iteration (a page redrawn by an event handler before a
    /// nested mode draws: the Inventory's Use dialog); shown first, dropped when `pending` shows
    #[serde(default)]
    pub pre: Option<Box<Screen>>,
    /// the top screen ran scripts through the host this frame (item use): `post` refreshes its redraw after them
    #[serde(default)]
    pub after_scripts: bool,
    /// video frames the mode loop is still busy (no logic, no input read)
    pub lag: u8,
    /// buttons at the mode loop's last input read (edges are computed between reads)
    pub held: u16,
    pub pressed: u16,
    pub released: u16,
    /// the mode loop's frame counter (`0x03000F94`, counts logic frames only)
    pub ticks: u32,
    /// the closed menu's result (cleared by whoever reads it)
    pub result: Option<MenuResult>,
    /// opened by a script (the script waits for `result`) rather than by START
    pub scripted: bool,
    /// the screen change around this menu (fade steps, black frames)
    pub tr: Transition,
    /// the menu closed this frame with a transition that hands the frame back to the world (`leave` 1, `fade_in` 0:
    /// screens drawn over the field): the world updates in this same frame
    #[serde(default)]
    pub resume: bool,
    /// the game options (`0x03001914`: text speed, SFX volume, music volume, 0; the cartridge's options block,
    /// shared by every save): kept across menus (options.rs)
    #[serde(default)]
    pub settings: options::Settings,
    /// an event the closed menu queued for the field (`GD+0x610` deferred scripts: Options -> Sleep = s007); run
    /// in the first field frame after the menu
    #[serde(default)]
    pub then: Option<String>,
    /// a nested mode ended with this result (`0x0803D876`): its exit stub runs one more iteration (input read, no
    /// draw) before the parent's step resumes
    #[serde(default)]
    pub exit_stub: Option<i32>,
    /// a screen picture left on screen after the menu is gone, while the world runs below it until `World.frame`
    /// reaches `overlay_until` (New Game: the prologue script fades the title's Select Game picture out; title.rs)
    #[serde(default)]
    pub overlay: Option<Box<Screen>>,
    #[serde(default)]
    pub overlay_until: u32,
    /// harnesses: the picture on screen when a screen is opened mid-redraw (the first redraw becomes `pending`)
    #[serde(default)]
    pub boot_shown: Option<Box<Screen>>,
}

/// How a menu screen comes and goes (all measured per screen kind): the field fades out by `fade_out` per frame
/// (palette fade level 0 -> 256), `enter` black frames (world save, mode enter, first draw: the page shows on the
/// last), the menu runs, `leave` frames after the closing iteration (the first still shows the page, then black:
/// the world restore), the field fades in by `fade_in` per frame. A step of 0 = no fade (the screen cuts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transition {
    pub fade_out: u16,
    pub enter: u16,
    pub leave: u16,
    pub fade_in: u16,
}

impl Default for Transition {
    fn default() -> Self {
        PAUSE
    }
}

/// the pause menu (START): see [`step`]
/// `Transition.fade_out` flag: the fade-out's last level stays one more frame before the black frames (the quick
/// fade `0x0805058C` of script-opened screens)
pub const FADE_HOLD: u16 = 0x8000;

pub const PAUSE: Transition = Transition { fade_out: 16, enter: 5, leave: 25, fade_in: 16 };

/// Open a menu screen with a transition (`scripted`: a script waits for `World.menu.result`). The field fades out
/// from the next frame.
pub fn open(w: &mut World, screen: Screen, tr: Transition, scripted: bool) {
    let settings = w.menu.settings;
    w.menu = Menu { phase: Phase::FadeOut { level: 0 }, stack: vec![screen], scripted, tr, settings, ..Menu::default() };
    if tr.fade_out == 0 {
        w.menu.phase = Phase::Enter { t: 0 };
    }
}

/// is a menu screen up (the world does not update)?
pub fn active(w: &World) -> bool {
    w.menu.phase != Phase::Idle
}

/// sound ids (table 0x083192A8): 40 = menu open / page / confirm, 39 = cursor move, 41 = buy / accept
pub const SFX_MENU: i32 = 40;
pub const SFX_CURSOR: i32 = 39;
pub const SFX_ACCEPT: i32 = 41;

/// Field main loop `0x08006E3C`: START pressed with the current character's HP != 0 opens the pause menu (sound
/// 40, then the 16-frame fade). Called after the field rules of a normal field frame.
pub fn check_pause(w: &mut World) -> bool {
    // a closed menu's queued event (options.rs Sleep) runs in the first field frame after it
    if !active(w) {
        if let Some(ev) = w.menu.then.take() {
            w.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run { run: crate::fieldcontent::Callback::Event(ev), cause: crate::field::Cause::Menu, slot: 0 }));
        }
    }
    if active(w) || w.menu.scripted {
        return false;
    }
    let Some(f) = w.field.as_ref() else { return false };
    if f.busy || f.item_get.is_some() || f.held || !matches!(f.mode, crate::field::Mode::Normal) {
        return false;
    }
    if crate::scene::frozen(w) || w.scene.depth > 0 {
        return false;
    }
    if w.input[0].pressed & crate::input::START == 0 {
        return false;
    }
    if w.data.menu.is_none() {
        return false;
    }
    let cur = w.story.current.max(0) as usize;
    let hp = w.player_idx(0).and_then(|i| match &w.ents[i].body {
        crate::world::Body::Player(p) => Some(p.rec.hp),
        _ => None,
    });
    let hp = hp.unwrap_or_else(|| w.story.chars.get(cur).map(|c| c.rec.hp).unwrap_or(0));
    if hp == 0 {
        return false;
    }
    w.sfx(SFX_MENU, 64);
    let page = status::Status::new(w);
    open(w, Screen::Status(page), PAUSE, false);
    true
}

/// pause-menu timing (captures `re/buu/menu/capture.py` status_open_close, frame 0 = START):
/// * frames 1..16: the fade-out mode (one iteration per frame, level 16k; frame 16 is black), the world frozen;
/// * frames 17..20 black: the world save, the page's mode enter and its first draw (CPU time; frame 20 shows the
///   VRAM copy mid-scanout, not modelled: black here), the page at frame 21, the first input read at 22;
/// * the closing iteration still shows the page; then 24 black frames (the exit stub's iteration + the world
///   restore), the 16-frame fade-in (levels 240..0), the world runs again the frame after.

/// One video frame of an active menu (called instead of the world update).
pub fn step(w: &mut World) {
    let held = w.input[0].held;
    match w.menu.phase {
        Phase::Idle => {}
        Phase::FadeOut { level } => {
            // the fade mode's iteration reads the input (edges of the first menu read are relative to it)
            w.menu.held = held;
            w.story.play_frames += 1;
            let step = (w.menu.tr.fade_out & !FADE_HOLD).max(1);
            let end = if w.menu.tr.fade_out & FADE_HOLD != 0 { 256 + step } else { 256 };
            let l = level + step;
            w.menu.phase = if l >= end { Phase::Enter { t: 0 } } else { Phase::FadeOut { level: l } };
        }
        Phase::Enter { t } => {
            if t + 1 >= w.menu.tr.enter.max(1) {
                w.menu.phase = Phase::Run;
                // the first update draws during the black frames: on screen now, no lag after it
                redraw(w, true);
            } else {
                w.menu.phase = Phase::Enter { t: t + 1 };
            }
        }
        Phase::Run => {
            if w.menu.lag > 0 {
                w.menu.lag -= 1;
                if w.menu.lag == 0 {
                    // the long update ends with the VRAM copy: the new surface shows in its last frame
                    if let Some(p) = w.menu.pending.take() {
                        w.menu.shown = Some(p);
                    }
                    w.menu.pre = None;
                }
                return;
            }
            tick(w);
        }
        Phase::Leave { t } => {
            if t == 0 {
                // the exit stub's iteration (counts a logic frame)
                w.story.play_frames += 1;
            }
            if t + 1 >= w.menu.tr.leave.max(1) {
                let st = w.menu.tr.fade_in;
                if st == 0 {
                    w.menu.phase = Phase::Idle;
                    w.menu.shown = None;
                    w.menu.resume = w.menu.tr.leave <= 1;
                } else {
                    w.menu.phase = Phase::FadeIn { level: 256u16.saturating_sub(st) };
                }
                w.story.play_frames += 1;
            } else {
                w.menu.phase = Phase::Leave { t: t + 1 };
            }
        }
        Phase::FadeIn { level } => {
            w.story.play_frames += 1;
            let l = level.saturating_sub(w.menu.tr.fade_in.max(1));
            if l == 0 {
                w.menu.phase = Phase::Idle;
                w.menu.shown = None;
            } else {
                w.menu.phase = Phase::FadeIn { level: l };
            }
        }
    }
}

/// one iteration of the mode loop, from the input read: input, step (widgets, page logic), then the next
/// iteration's update (redraw if dirty; its CPU time becomes lag frames)
fn tick(w: &mut World) {
    let held = w.input[0].held;
    let m = &mut w.menu;
    m.pressed = held & !m.held;
    m.released = m.held & !held;
    m.held = held;
    m.ticks = m.ticks.wrapping_add(1);
    w.story.play_frames += 1;
    step_screen(w);
    if w.menu.phase == Phase::Run {
        redraw(w, false);
    }
}

/// update(): the mode's counters, then a redraw of the top screen when its container is not drawn. A redraw
/// costing n frames leaves n - 1 lag frames and the surface on screen in the last of them.
fn redraw(w: &mut World, first: bool) {
    if w.menu.exit_stub.is_some() {
        return;
    }
    let Some(mut top) = w.menu.stack.pop() else { return };
    top.update(w);
    if !top.container_ref().drawn {
        top.before_draw(w);
        top.container().drawn = true;
        let cost = if first { 1 } else { top.redraw_cost().max(1) };
        if let Some(prev) = w.menu.boot_shown.take().filter(|_| first) {
            w.menu.shown = Some(prev);
            w.menu.pending = Some(Box::new(top.clone()));
            w.menu.lag = 1;
        } else if cost == 1 {
            w.menu.shown = Some(Box::new(top.clone()));
            w.menu.pending = None;
            if first {
                w.menu.lag = top.page().enter_lag();
            }
        } else if top.page().show_at_once() {
            w.menu.shown = Some(Box::new(top.clone()));
            w.menu.pending = None;
            w.menu.lag = cost - 1;
        } else {
            w.menu.pending = Some(Box::new(top.clone()));
            w.menu.lag = cost - 1;
        }
        top.after_draw(w);
    }
    w.menu.stack.push(top);
}

/// what the top screen's step asks for
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// replace the top screen (L / R page switch, `0x0803D850`)
    Replace(Screen),
    /// run a nested mode (Skills); its result goes to this screen's `child_result`
    Push(Screen),
    /// end the top mode with a result (`0x0803D876(r)`); the menu closes when it was the last one
    Pop(i32),
    /// close the whole menu
    Close,
    /// close the whole menu from the mode's done check (its step returned non-zero, e.g. START): no exit stub
    /// iteration, the screen goes black in this frame
    Done,
    /// a nested mode's done check returned `r` (`run_mode` returns at once, no exit stub): the parent's handler
    /// continues in this frame
    PopDone(i32),
}

fn step_screen(w: &mut World) {
    let Some(mut top) = w.menu.stack.pop() else { return };
    let mut act = match w.menu.exit_stub.take() {
        // the nested mode's exit stub iteration is over: the parent's handler continues with the result
        Some(r) => match w.menu.stack.pop() {
            Some(mut parent) => {
                let a = parent.child_result(w, r);
                top = parent;
                a
            }
            None => Action::Pop(r),
        },
        None => top.step(w),
    };
    loop {
        match act {
            Action::None => w.menu.stack.push(top),
            Action::Replace(s) => w.menu.stack.push(s),
            Action::Push(s) => {
                w.menu.stack.push(top);
                w.menu.stack.push(s);
            }
            Action::Pop(r) => {
                if !w.menu.stack.is_empty() {
                    // one more iteration of the ended mode (its exit stub) before the parent resumes
                    w.menu.exit_stub = Some(r);
                    w.menu.stack.push(top);
                    break;
                }
                close(w);
                w.menu.result = Some(MenuResult { code: r });
            }
            Action::Close => close(w),
            Action::Done => {
                close(w);
                w.menu.phase = Phase::Leave { t: 1 };
            }
            Action::PopDone(r) => {
                if let Some(mut parent) = w.menu.stack.pop() {
                    act = parent.child_result(w, r);
                    top = parent;
                    continue;
                }
                close(w);
                w.menu.phase = Phase::Leave { t: 1 };
                w.menu.result = Some(MenuResult { code: r });
            }
        }
        break;
    }
}

fn close(w: &mut World) {
    w.menu.stack.clear();
    w.menu.phase = Phase::Leave { t: 0 };
    w.menu.lag = 0;
    w.menu.pending = None;
    w.menu.result = Some(MenuResult { code: 0 });
}

/// the menu screen to show this frame (None while the field is shown: fades, idle)
pub fn view(w: &World) -> Option<MenuView> {
    match w.menu.phase {
        // the closing iteration still shows the page
        Phase::Leave { t: 0 } if w.menu.shown.as_deref().is_some_and(|s| s.page().leave_black()) => {
            return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() })
        }
        Phase::Run | Phase::Leave { t: 0 } => {}
        Phase::Enter { .. } | Phase::Leave { .. } => return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() }),
        Phase::Idle => {
            let o = w.menu.overlay.as_deref().filter(|_| w.frame < w.menu.overlay_until)?;
            let mut v = o.view(w, w.data.menu.as_ref()?);
            // the script's palette fade (`0x08007074`) darkens the picture still on screen; the fade mode writes the
            // palette before the frame is shown (one step ahead of the field's presenter)
            let mut dim = w.scene.screen.dim.clamp(0, 256);
            if let Some(f) = w.scene.screen.fade.as_ref().filter(|f| f.kind == crate::scene::FadeKind::Black && f.out) {
                dim = (256 * (f.t + 1).min(f.len) / f.len.max(1)).clamp(0, 256);
            }
            if dim > 0 {
                v.fade = Some(wf_view::PaletteFade { level: dim as u16, target: 0 });
            }
            return Some(v);
        }
        _ => return None,
    }
    let d = w.data.menu.as_ref()?;
    let s = w.menu.shown.as_deref()?;
    let mut v = s.view(w, d);
    // OBJs: the objects' state at the last update (OAM is written by the update, not the redraw)
    if let Some(top) = w.menu.stack.last() {
        v.objs.extend(top.page().objs(w));
    }
    Some(v)
}

/// A script-opened menu has finished (the fade-in after it is over): its result code (the native's push / effect
/// is decided by the screen). Clears it.
pub fn take_result(w: &mut World) -> Option<i32> {
    if !w.menu.scripted || w.menu.phase != Phase::Idle {
        return None;
    }
    let r = w.menu.result.take()?;
    w.menu.scripted = false;
    Some(r.code)
}

/// the menu was opened during this frame (its first fade frame is the next one): the frame finishes normally
pub fn opened_this_frame(w: &World) -> bool {
    w.menu.phase == Phase::FadeOut { level: 0 }
}

/// the field's palette fade while the menu fades it out / in (level 0..256 toward black)
pub fn field_fade(w: &World) -> Option<u16> {
    match w.menu.phase {
        // `FADE_HOLD`: the last level stays one more frame
        Phase::FadeOut { level } => Some(if level >= 256 { 256 - (w.menu.tr.fade_out & !FADE_HOLD) } else { level }),
        Phase::FadeIn { level } => Some(level),
        Phase::Enter { .. } | Phase::Leave { .. } => None,
        _ => None,
    }
}

/// The rest of an active menu's frame (`Session::step_post`): when the top screen ran scripts through the host this
/// frame (they run after `step`, the game runs them inside its event handler), let it apply their results and redo
/// the iteration's redraw snapshot.
pub fn post(w: &mut World) {
    if !w.menu.after_scripts {
        return;
    }
    w.menu.after_scripts = false;
    let Some(mut top) = w.menu.stack.pop() else { return };
    top.page_mut().after_scripts(w);
    if top.container_ref().drawn {
        top.before_draw(w);
        let snap = Some(Box::new(top.clone()));
        if w.menu.pending.is_some() {
            w.menu.pending = snap;
        } else if w.menu.shown.is_some() {
            w.menu.shown = snap;
        }
    }
    w.menu.stack.push(top);
}
