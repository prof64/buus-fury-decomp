//! The map screen (SELECT in a 2D field area; field main loop `0x08006E3C`): a modal object with three pages
//! (re/buu/menu/NOTES_minigames.md "Map screen"):
//!
//! * **Area map** (ctor `0x080256F0`, vt `0x0804E384` the first time / `0x0804A4E8` when returned to; setup
//!   `0x0802574C`): the record of table `0x0874CA24` holding the current area; its rooms drawn into a 128x128 tile
//!   buffer (visited rooms: rect2 / map2, story flag `0x1C0 + bit`; with game bit 0x40 the unvisited ones as rect1 /
//!   map1), shown as BG2 scrolled by the camera (`0x08025B34`), the D-pad scrolls 1 px per frame inside the rooms'
//!   bounds (`0x08025D48`), a blinking player marker (`0x08026EE6`, the player's position scaled from the area's
//!   map size into the current room), the record's markers (`0x08026C9C`) and four blinking scroll arrows
//!   (`0x08026B4C`) shown while the bounds allow scrolling that way.
//! * **World map** (`0x08025E2F` -> vt `0x0804A994`, enter `0x0802641C`): the 240x160 world picture over the green
//!   grid, the markers of script #15 (one per unlocked place, the current world's blinking).
//! * **Scouter** (`0x080261D4`, vt `0x0804A570`): the live field objects on the grid with a target cursor —
//!   **stand-in** here (the frame, grid and page navigation only; see the notes).
//!
//! Pages: Scouter <-L- Area -R-> World. Common step `0x08024C54`: the page's own step (+0x40), then the fade: EVA
//! (BLDALPHA, EVB 0) rises by 1 per iteration to 16; L / R (when that page exists) start a page change (BLDCNT
//! 0x3E5E: the frame BG0 stays lit, the pressed prompt tiles), B / SELECT / A (area and world pages) close
//! (BLDCNT 0x3F5F); a running change lowers EVA by 1 and at 0 replaces the page (`0x0803D850`) or ends the screen.

use super::kit::Container;
use super::{Action, MenuData, Page, Screen, Transition};
use crate::input::{A, B, DOWN, L, LEFT, R, RIGHT, SELECT, UP};
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wf_view::{MenuBlend, MenuLayer, MenuObj, MenuOp, MenuView};

/// one room of an area map record
#[derive(Clone, Debug, Default)]
pub struct Room {
    pub world: i32,
    pub area: i32,
    pub bit: i32,
    pub rect1: [i32; 4],
    pub rect2: [i32; 4],
    pub map1: Vec<u16>,
    pub map2: Vec<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub rect: [i32; 4],
    pub attr0: u16,
    pub tile: u16,
    pub shown: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Record {
    pub tiles: String,
    /// the tiles' decompressed size (the setup's CPU time grows with it)
    pub tile_bytes: u32,
    pub objs: Option<String>,
    pub entries: Vec<Entry>,
    pub rooms: Vec<Room>,
}

/// `ui/mapscreen/area_maps.json` (wf-ui mapscreen.rs)
#[derive(Clone, Debug, Default)]
pub struct MapData {
    pub records: Vec<Record>,
    /// scroll arrows: attr0, attr1 per type (0 down, 1 up, 2 left, 3 right)
    pub arrows: [(u16, u16); 4],
    /// the Scouter's database, remaps and info page data (`ui/menu/scouter.json`): scouter.rs
    pub scout: Option<Arc<super::scouter::ScoutData>>,
}

fn arr4(v: &serde_json::Value) -> [i32; 4] {
    let mut o = [0; 4];
    for (k, x) in v.as_array().into_iter().flatten().take(4).enumerate() {
        o[k] = x.as_i64().unwrap_or(0) as i32;
    }
    o
}

impl MapData {
    pub fn load(src: &dyn crate::content::ContentSource) -> Option<Arc<MapData>> {
        let v: serde_json::Value = serde_json::from_slice(&src.read("ui/mapscreen/area_maps.json")?).ok()?;
        let u16s = |v: &serde_json::Value| -> Vec<u16> { v.as_array().into_iter().flatten().map(|x| x.as_u64().unwrap_or(0) as u16).collect() };
        let records = v["records"]
            .as_array()?
            .iter()
            .map(|r| Record {
                tiles: format!("ui/{}", r["tiles"].as_str().unwrap_or("")),
                tile_bytes: r["tile_count"].as_u64().unwrap_or(0) as u32 * 64,
                objs: r["objs"].as_str().map(|s| format!("ui/{s}")),
                entries: r["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| Entry { rect: arr4(&e["rect"]), attr0: e["attr0"].as_u64().unwrap_or(0) as u16, tile: e["tile"].as_u64().unwrap_or(0) as u16, shown: e["shown"].as_u64().unwrap_or(0) != 0 })
                    .collect(),
                rooms: r["rooms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| Room {
                        world: m["world"].as_i64().unwrap_or(0) as i32,
                        area: m["area"].as_i64().unwrap_or(0) as i32,
                        bit: m["bit"].as_i64().unwrap_or(0) as i32,
                        rect1: arr4(&m["rect1"]),
                        rect2: arr4(&m["rect2"]),
                        map1: u16s(&m["map1"]),
                        map2: u16s(&m["map2"]),
                    })
                    .collect(),
            })
            .collect();
        let mut arrows = [(0u16, 0u16); 4];
        for (k, a) in v["arrows"].as_array().into_iter().flatten().take(4).enumerate() {
            arrows[k] = (a["attr0"].as_u64().unwrap_or(0) as u16, a["attr1"].as_u64().unwrap_or(0) as u16);
        }
        Some(Arc::new(MapData { records, arrows, scout: super::scouter::ScoutData::load(src) }))
    }
}

/// script #15 (the world page's markers, `add_map_marker(x, y, get_world() == w)` when the flag is set)
pub const WORLD_MARKERS: [(i32, i32, i32, i32); 13] = [
    (13, 170, 94, 2),
    (15, 163, 74, 3),
    (398, 91, 76, 4),
    (399, 181, 102, 5),
    (400, 56, 67, 6),
    (401, 133, 127, 7),
    (402, 70, 111, 8),
    (403, 201, 123, 9),
    (404, 151, 99, 10),
    (405, 108, 67, 11),
    (406, 64, 56, 13),
    (407, 142, 79, 14),
    (107, 146, 58, 15),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapPage {
    Area,
    World,
    Scouter,
}

/// a blinking marker (`0x08026DE4`, vt `0x0804D634` / `0x0804D594`): top-left position, the animation counter
/// (+0x14) and the tile frame uploaded (+0x18); frames 0 1 2 3 2 1 0.. every 8 updates while it animates,
/// a non-animating one shows frame 2
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Marker {
    pub x: i32,
    pub y: i32,
    pub anim: bool,
    pub t: u32,
    pub frame: u8,
}

impl Marker {
    fn new(x: i32, y: i32, anim: bool) -> Marker {
        Marker { x: x - 4, y: y - 4, anim, t: 0, frame: if anim { 0 } else { 2 } }
    }
    fn update(&mut self) {
        if !self.anim {
            return;
        }
        self.t = self.t.wrapping_add(1);
        let mut f = (self.t >> 3) & 7;
        if f >= 4 {
            f = 7 - f;
        }
        self.frame = f as u8;
    }
}

/// the OAM the screen shows (built by the previous update; the shadow is copied at VBlank)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Oam {
    /// (x, y, priority 3) of the visible markers
    pub markers: Vec<(i32, i32, u8)>,
    /// visible record entries: (entry index, screen x, y)
    pub entries: Vec<(u16, i32, i32)>,
    pub arrows: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MapScreen {
    pub c: Container,
    pub page: MapPage,
    /// the area record (`0x08004B08`), -1 = none
    pub rec: i32,
    /// rooms drawn: (room index, 1 = rect1 / map1, 2 = rect2 / map2), in record order
    pub rooms: Vec<(u16, u8)>,
    /// the rooms' bounds in px
    pub bounds: [i32; 4],
    /// camera (+0x802C, +0x8030), scroll limits (+0xC..+0x18), arrow limits (+0x1C..+0x28)
    pub cam: (i32, i32),
    /// the BG scroll on screen: the camera before this iteration's step (the scroll registers are written at the
    /// iteration's start, `0x08025D2E`)
    pub bg_cam: (i32, i32),
    pub lim: [i32; 4],
    pub arr: [i32; 4],
    pub markers: Vec<Marker>,
    /// scroll arrows: counter (+0x12), frame (+0x11)
    pub arrows: [(u8, u8); 4],
    /// +5 EVA, +8 changing / closing, +6 next page (1 L, 2 R)
    pub eva: u8,
    pub changing: bool,
    pub next: u8,
    /// BLDCNT 0x3E5E (page change: BG0 not blended) instead of 0x3F5F
    pub keep_frame: bool,
    /// the prompt tiles: 0 none, 1 normal, 2 pressed
    pub l_btn: u8,
    pub r_btn: u8,
    pub oam: Oam,
    pub oam_shown: Oam,
    /// the page's first draw is done (its enter's CPU time: [`MapScreen::redraw_cost`])
    pub entered: bool,
    /// the area record's tile bytes (CPU time model)
    pub tile_bytes: u32,
    /// the Scouter page's objects
    #[serde(default)]
    pub scout: Option<Box<super::scouter::Scout>>,
}

/// screen change of the map screen (frame 0 = SELECT): the pause menu's 16-frame fade, black while the world is
/// saved, the page's EVA ramp from the 19th frame; after the closing iteration 24 black frames and the fade-in
pub const MAPSCREEN: Transition = Transition { fade_out: 16, enter: 2, leave: 24, fade_in: 16 };

pub(crate) fn md(w: &World) -> Option<Arc<MapData>> {
    w.data.menu.as_ref().and_then(|d| d.maps.clone())
}

impl MapScreen {
    fn blank(page: MapPage) -> MapScreen {
        MapScreen {
            c: Container::default(),
            page,
            rec: -1,
            rooms: Vec::new(),
            bounds: [0; 4],
            cam: (0, 0),
            bg_cam: (0, 0),
            lim: [0; 4],
            arr: [0; 4],
            markers: Vec::new(),
            arrows: [(0, 0); 4],
            eva: 0,
            changing: false,
            next: 0,
            keep_frame: false,
            l_btn: 0,
            r_btn: 0,
            oam: Oam::default(),
            oam_shown: Oam::default(),
            entered: false,
            tile_bytes: 0,
            scout: None,
        }
    }

    /// `0x080256F0` + `0x0802574C`: the area page
    pub fn new(w: &World) -> MapScreen {
        let mut s = MapScreen::blank(MapPage::Area);
        s.l_btn = 1;
        s.r_btn = 1;
        let Some(d) = md(w) else { return s };
        let (cw, ca) = w.field.as_ref().map(|f| (f.area.world, f.area.area)).unwrap_or((-1, -1));
        // 0x08004B08: the first record with a room of the current area
        let Some(ri) = d.records.iter().position(|r| r.rooms.iter().any(|m| m.world == cw && m.area == ca)) else { return s };
        let rec = &d.records[ri];
        s.rec = ri as i32;
        s.tile_bytes = rec.tile_bytes;
        let reveal = w.story.game_bits & 0x40 != 0;
        let mut b: Option<[i32; 4]> = None;
        let mut cur: Option<&Room> = None;
        for (k, m) in rec.rooms.iter().enumerate() {
            let visited = w.story.flag(0x1C0 + m.bit);
            let (rect, var) = if visited {
                (m.rect2, 2)
            } else if reveal {
                (m.rect1, 1)
            } else {
                continue;
            };
            // the bounds: the first room's rect, then the union (an empty rect does not count)
            if !(rect[0] == rect[2] && rect[1] == rect[3]) {
                b = Some(match b {
                    None => rect,
                    Some(o) if o[0] == o[2] && o[1] == o[3] => rect,
                    Some(o) => [o[0].min(rect[0]), o[1].min(rect[1]), o[2].max(rect[2]), o[3].max(rect[3])],
                });
            }
            if m.world == cw && m.area == ca {
                cur = Some(m);
            }
            s.rooms.push((k as u16, var));
        }
        let b = b.unwrap_or([0; 4]);
        let b = [b[0] * 8, b[1] * 8, b[2] * 8, b[3] * 8];
        s.bounds = b;
        s.lim = [b[0] - 0xDF, b[1] - 0x8F, b[2] - 0x11, b[3] - 0x11];
        s.arr = [b[0] - 0x10, b[1] - 0x10, b[2] - 0xE0, b[3] - 0x90];
        // the player marker: the player's px position scaled from the map into the current room (rect1)
        if let Some(m) = cur {
            let (px, py) = w
                .player_idx(0)
                .and_then(|i| match &w.ents[i].body {
                    crate::world::Body::Player(p) => Some((p.a.x >> 8, p.a.y >> 8)),
                    _ => None,
                })
                .unwrap_or((0, 0));
            let (mw, mh) = w.col.as_ref().map(|c| (c.map.world_w, c.map.world_h)).unwrap_or((240, 160));
            let r = m.rect1;
            let cx = ((r[2] - r[0]) * 8 * px + (mw >> 1)) / mw.max(1) + r[0] * 8;
            let cy = ((r[3] - r[1]) * 8 * py + (mh >> 1)) / mh.max(1) + r[1] * 8;
            s.markers.push(Marker::new(cx, cy, true));
            s.cam = (cx - 0x78, cy - 0x50);
            s.bg_cam = s.cam;
        }
        s.arrows = [(0, 0); 4];
        s
    }

    /// `0x0802641C`: the world page (the markers of script #15)
    pub fn world(w: &World) -> MapScreen {
        let mut s = MapScreen::blank(MapPage::World);
        s.l_btn = 1;
        let cw = w.field.as_ref().map(|f| f.area.world).unwrap_or(-1);
        for &(flag, x, y, wd) in WORLD_MARKERS.iter() {
            if w.story.flag(flag) {
                s.markers.push(Marker::new(x, y, cw == wd));
            }
        }
        s
    }

    /// `0x080261D4`: the scouter page (scouter.rs)
    pub fn scouter(w: &mut World) -> MapScreen {
        let mut s = MapScreen::blank(MapPage::Scouter);
        s.r_btn = 1;
        s.scout = Some(Box::new(super::scouter::Scout::new(w)));
        s
    }

    fn has_prev(&self) -> bool {
        self.page != MapPage::Scouter
    }
    fn has_next(&self) -> bool {
        self.page != MapPage::World
    }

    /// the objects' update + OAM build (`0x08011A1E`, `0x080115B0`)
    fn objects(&mut self, w: &World) {
        if let Some(sc) = self.scout.as_mut() {
            sc.update(w);
        }
        for m in self.markers.iter_mut() {
            m.update();
        }
        let mut oam = Oam::default();
        if self.page == MapPage::Area && self.rec >= 0 {
            for k in 0..4 {
                let (c, f) = &mut self.arrows[k];
                let on = match k {
                    0 => self.cam.1 < self.arr[3],
                    1 => self.cam.1 > self.arr[1],
                    2 => self.cam.0 > self.arr[0],
                    _ => self.cam.0 < self.arr[2],
                };
                let mut fr;
                if on {
                    *c = c.wrapping_add(1);
                    fr = (*c >> 2) & 7;
                    if fr >= 4 {
                        fr = 7 - fr;
                    }
                } else {
                    fr = 3;
                    *c = 0x10;
                }
                *f = fr;
            }
            oam.arrows = true;
            let (cx, cy) = self.cam;
            for m in &self.markers {
                // 0x08026F2A: screen position inside (-7, -7)-(240, 160)
                let (sx, sy) = (m.x - cx, m.y - cy);
                if sx >= -7 && sx < 240 && sy >= -7 && sy < 160 {
                    oam.markers.push((sx, sy, 3));
                }
            }
            if let Some(d) = md(w) {
                if let Some(rec) = d.records.get(self.rec as usize) {
                    for (k, e) in rec.entries.iter().enumerate() {
                        // 0x08026CDA: shown and overlapping the camera rect
                        let r = e.rect;
                        if e.shown && r[0] < cx + 240 && cx < r[2] && r[1] < cy + 160 && cy < r[3] {
                            oam.entries.push((k as u16, r[0] - cx, r[1] - cy));
                        }
                    }
                }
            }
        } else {
            for m in &self.markers {
                if m.x >= -7 && m.x < 240 && m.y >= -7 && m.y < 160 {
                    oam.markers.push((m.x, m.y, 3));
                }
            }
        }
        self.oam = oam;
    }

    fn start_change(&mut self, next: u8) {
        self.next = next;
        self.keep_frame = true;
        self.changing = true;
        if next == 1 {
            self.l_btn = 2;
        } else {
            self.r_btn = 2;
        }
    }

    /// vt +0x1C
    fn close(&mut self) {
        if self.eva != 0 {
            self.keep_frame = false;
            self.changing = true;
        }
    }
}

impl Page for MapScreen {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    /// the update before the VBlank (`0x08024C38`): BG from the camera, the objects' animations, the OAM
    fn before_draw(&mut self, w: &World) {
        self.oam_shown = std::mem::take(&mut self.oam);
        self.objects(w);
    }
    fn after_draw(&mut self, _w: &World) {
        self.entered = true;
    }
    /// a page change's enter (decompression, the room composition) delays the next iteration: the world page
    /// (600 tiles) 4 frames, the area page by its setup's CPU time ([`setup_frames`], started early in the
    /// frame); its picture shows at once
    fn redraw_cost(&self) -> u8 {
        match (self.entered, self.page) {
            (false, MapPage::World) => 4,
            (false, MapPage::Area) => 1 + setup_frames(self.tile_bytes, RETURN_START),
            (false, _) => 2,
            _ => 1,
        }
    }
    fn show_at_once(&self) -> bool {
        true
    }
    fn step(&mut self, w: &mut World) -> Action {
        // every iteration redraws (animations, scrolling, the fade)
        self.c.drawn = false;
        self.bg_cam = self.cam;
        let (held, pressed) = (w.menu.held, w.menu.pressed);
        // the page's step (+0x40)
        match self.page {
            MapPage::Area => {
                if self.rec >= 0 {
                    if held & LEFT != 0 && self.cam.0 > self.lim[0] {
                        self.cam.0 -= 1;
                    }
                    if held & RIGHT != 0 && self.cam.0 < self.lim[2] {
                        self.cam.0 += 1;
                    }
                    if held & UP != 0 && self.cam.1 > self.lim[1] {
                        self.cam.1 -= 1;
                    }
                    if held & DOWN != 0 && self.cam.1 < self.lim[3] {
                        self.cam.1 += 1;
                    }
                }
                if pressed & A != 0 {
                    self.close();
                }
            }
            MapPage::World => {
                if pressed & A != 0 {
                    self.close();
                }
            }
            MapPage::Scouter => {
                if pressed & A != 0 && self.scout.as_ref().is_some_and(|s| s.sel.is_some()) {
                    // 0x08026360: the info page after the fade (BLDCNT 0x3E5E)
                    self.next = 3;
                    self.keep_frame = true;
                    self.changing = true;
                }
            }
        }
        if self.changing {
            self.eva = self.eva.wrapping_sub(1);
            if self.eva == 0 {
                return match self.next {
                    1 => Action::Replace(Screen::MapScreen(self.page_to(w, false))),
                    2 => Action::Replace(Screen::MapScreen(self.page_to(w, true))),
                    3 => Action::Replace(Screen::MapScreen(self.page_to(w, true))),
                    _ => {
                        // the last iteration's registers (EVA 0) stay on screen while the screen ends
                        w.menu.shown = Some(Box::new(Screen::MapScreen(self.clone())));
                        Action::Pop(1)
                    }
                };
            }
            return Action::None;
        }
        if pressed & L != 0 && self.has_prev() {
            self.start_change(1);
        }
        if pressed & R != 0 && self.has_next() {
            self.start_change(2);
        }
        if pressed & (B | SELECT) != 0 {
            self.close();
        }
        if self.eva < 16 {
            self.eva += 1;
        }
        Action::None
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        let _ = d;
        let mut v = MenuView::default();
        // BG3: the grid on the world / scouter pages
        let mut bg3 = Vec::new();
        if self.page != MapPage::Area {
            for ty in 0..20 {
                for tx in 0..30 {
                    bg3.push(MenuOp::Tile { path: "ui/mapscreen/grid.png".into(), tile: 0, x: tx * 8, y: ty * 8, hflip: false, vflip: false });
                }
            }
        }
        v.layers.push(MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 3, ops: bg3 });
        // BG2: the rooms (area) / the world picture
        let mut bg2 = MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 3, ops: Vec::new() };
        let mdat = md(w);
        match self.page {
            MapPage::Area => {
                if let Some(rec) = mdat.as_ref().and_then(|m| m.records.get(self.rec.max(0) as usize)).filter(|_| self.rec >= 0) {
                    let b = self.bounds;
                    let (tx0, ty0, tw, th) = (b[0] / 8, b[1] / 8, (b[2] - b[0]) / 8, (b[3] - b[1]) / 8);
                    let mut grid = vec![0u16; (tw.max(0) * th.max(0)) as usize];
                    for &(k, var) in &self.rooms {
                        let m = &rec.rooms[k as usize];
                        let (r, map) = if var == 2 { (m.rect2, &m.map2) } else { (m.rect1, &m.map1) };
                        let mut it = map.iter();
                        for y in r[1]..r[3] {
                            for x in r[0]..r[2] {
                                let e = it.next().copied().unwrap_or(0);
                                if e != 0 && x >= tx0 && y >= ty0 && x < tx0 + tw && y < ty0 + th {
                                    grid[((y - ty0) * tw + (x - tx0)) as usize] = e;
                                }
                            }
                        }
                    }
                    bg2 = MenuLayer { w: (tw * 8).max(0) as u16, h: (th * 8).max(0) as u16, x: b[0] - self.bg_cam.0, y: b[1] - self.bg_cam.1, priority: 3, ops: Vec::new() };
                    for y in 0..th {
                        for x in 0..tw {
                            let e = grid[(y * tw + x) as usize];
                            if e != 0 {
                                bg2.ops.push(MenuOp::Tile { path: rec.tiles.clone(), tile: e & 0x3FF, x: x * 8, y: y * 8, hflip: e & 0x400 != 0, vflip: e & 0x800 != 0 });
                            }
                        }
                    }
                }
            }
            MapPage::World => {
                bg2.ops.push(MenuOp::Image { path: "ui/mapscreen/world_map.png".into(), frame: 0, fw: 240, fh: 160, x: 0, y: 0, key0: true });
            }
            MapPage::Scouter => {}
        }
        v.layers.push(bg2);
        // BG0: the frame with the L / R prompts
        let bg0 = MenuOp::Image { path: format!("ui/mapscreen/bg0_{}{}.png", self.l_btn, self.r_btn), frame: 0, fw: 240, fh: 160, x: 0, y: 0, key0: true };
        v.layers.push(MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 0, ops: vec![bg0] });
        // OBJs (OAM of the previous update; tile frames current)
        let o = &self.oam_shown;
        if o.arrows {
            if let Some(m) = mdat.as_ref() {
                for (k, &(c0, c1)) in m.arrows.iter().enumerate() {
                    let horiz = k < 2;
                    let (fw, fh) = if horiz { (16, 8) } else { (8, 16) };
                    v.objs.push(MenuObj {
                        path: if horiz { "ui/mapscreen/arrow_h.png".into() } else { "ui/mapscreen/arrow_v.png".into() },
                        frame: self.arrows[k].1 as u16,
                        fw,
                        fh,
                        x: (c1 & 0x1FF) as i32,
                        y: (c0 & 0xFF) as i32,
                        priority: 0,
                        hflip: c1 & 0x1000 != 0,
                        vflip: c1 & 0x2000 != 0,
                        pal: 0,
                        ..Default::default()
                    });
                }
            }
        }
        for (i, &(x, y, p)) in o.markers.iter().enumerate() {
            let f = self.markers.get(i).map(|m| m.frame).unwrap_or(0);
            v.objs.push(MenuObj { path: "ui/mapscreen/marker.png".into(), frame: f as u16, fw: 8, fh: 8, x, y, priority: p, hflip: false, vflip: false, pal: 0, ..Default::default() });
        }
        if let Some(sc) = self.scout.as_ref() {
            sc.objs(w, &mut v.objs);
        }
        if let Some(rec) = mdat.as_ref().and_then(|m| m.records.get(self.rec.max(0) as usize)).filter(|_| self.rec >= 0) {
            if let Some(op) = rec.objs.as_ref() {
                for &(k, x, y) in &o.entries {
                    let e = &rec.entries[k as usize];
                    v.objs.push(MenuObj { path: op.clone(), frame: e.tile / 2, fw: 8, fh: 8, x, y, priority: 3, hflip: false, vflip: false, pal: 0, ..Default::default() });
                }
            }
        }
        let first: u32 = if self.keep_frame { 0b011 | 1 << 31 } else { 0b111 | 1 << 31 };
        let second: u32 = first | 1 << 30;
        v.blend = Some(MenuBlend { first, second, eva: self.eva.min(16), evb: 0 });
        v
    }
}

impl MapScreen {
    /// the page L (`right` false) / R of this one; the new page keeps the blend mode (its enter does not touch
    /// BLDCNT) and starts at EVA 0
    fn page_to(&self, w: &mut World, right: bool) -> MapScreen {
        let mut s = match (self.page, right) {
            (MapPage::Area, false) => MapScreen::scouter(w),
            (MapPage::Area, true) => MapScreen::world(w),
            _ => MapScreen::new(w),
        };
        s.keep_frame = self.keep_frame;
        s
    }
}

/// Field main loop `0x08006E3C` (2D maps): SELECT pressed with the current character's HP != 0 opens the map
/// screen (sound 40, the 16-frame fade). Called right after `menu::check_pause` in the same place.
pub fn check_select(w: &mut World) -> bool {
    if super::active(w) || w.menu.scripted {
        return false;
    }
    let Some(f) = w.field.as_ref() else { return false };
    if f.busy || f.item_get.is_some() || f.held || f.flight.is_some() || !matches!(f.mode, crate::field::Mode::Normal) {
        return false;
    }
    if crate::scene::frozen(w) || w.scene.depth > 0 {
        return false;
    }
    if w.input[0].pressed & SELECT == 0 || w.data.menu.is_none() {
        return false;
    }
    let hp = w.player_idx(0).and_then(|i| match &w.ents[i].body {
        crate::world::Body::Player(p) => Some(p.rec.hp),
        _ => None,
    });
    if hp.unwrap_or(0) == 0 {
        return false;
    }
    w.sfx(super::SFX_MENU, 64);
    let s = MapScreen::new(w);
    let tr = Transition { enter: setup_frames(s.tile_bytes, OPEN_START).max(1) as u16, ..MAPSCREEN };
    super::open(w, Screen::MapScreen(s), tr, false);
    true
}

/// cycles into the frame where the area page's setup starts: after SELECT's fade (16th frame: the world save and
/// the constructor) / at a page change (the step in the VBlank); measured 184k..205k / ~10k
pub const OPEN_START: u32 = 195_000;
pub const RETURN_START: u32 = 10_000;

/// Frames from the setup's frame to the page's first iteration (CPU time model, `re/buu/menu/scratch/t6.py`:
/// setup = 200k + 25 cycles per decompressed tile byte, ±50k with the sound IRQ load; when it ends within the last
/// 20k cycles of a frame the first update misses that VBlank). Measured: Snakeway 6 080 bytes -> 2, Southwest
/// Forest 33 856 -> 4.
pub fn setup_frames(tile_bytes: u32, start: u32) -> u8 {
    const FRAME: u32 = 280_896;
    let end = start + 200_000 + 25 * tile_bytes;
    let n = end / FRAME;
    let late = end % FRAME > FRAME - 20_000;
    (n + late as u32).min(30) as u8
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "mapscreen"}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    if v["kind"].as_str()? != "mapscreen" {
        return None;
    }
    Some((Screen::MapScreen(MapScreen::new(w)), MAPSCREEN, false))
}
