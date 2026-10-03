//! The map screen's **Scouter** page (`0x080261D4`, vt `0x0804A570`; re/buu/menu/NOTES_minigames.md "Scouter").
//!
//! * Construction: message 0x16 to the saved field objects (`0x08011C0C`: the object list last to first;
//!   `0x080102FC`): the actors with a scouter database entry (`0x08027948`, table `0x0874D380`: characters and NPCs
//!   the first entry with the actor's sprite whose stats field is negative, else the first with the sprite; enemies
//!   the entry whose stats field is their type) add a record {db, HP (enemies) / −1, direction, screen position
//!   (position − camera)}.
//! * Enter (`0x08026212`): the grid page; per record a *blip* (`0x08027390`: a plain actor of the entry's sprite in
//!   the record's direction, idle (its enter: tick = rand(255)); the record's box = the idle's first frame record
//!   +4..+7), then the cursor (`0x08026F94`) at (120, 80); DISPCNT 0x1940 (BG0, BG3, OBJ).
//! * Update: the selection (`0x0802627E`: among the records whose box holds the cursor point the nearest box
//!   centre; a change gives the old blip priority byte 0x38 and the new 0x28 with the remap effect (vt
//!   `0x0804AE38`: remap `0x0874D360[(c >> 2) & 7]`, c counting updates)), then the objects: the cursor
//!   (`0x08027020`: A pressed closes the ring (counter +0xC20 up to 4), released opens it (down to 0), the tile frame
//!   c >> 1 uploaded at even counts; D-pad held ±0x80 velocity (24.8), position += velocity, velocity · 3/4
//!   (truncated), clamped to 16..224 × 16..144 (velocity 0)), the blips' idle animations.
//! * Step (+0x40, `0x08026360`): A pressed with a selection: the page change to the info page (`0x08025E50`).

use crate::scene::ActAnim;
use crate::world::{Body, World};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wf_view::MenuObj;

/// a scouter database entry (`ui/menu/scouter.json`, 16 bytes at `0x0874D380`)
#[derive(Clone, Debug, Default)]
pub struct DbEntry {
    pub name: u16,
    pub desc: u16,
    /// sprite def id (content sprite index + 9)
    pub def: i32,
    /// dialog portrait argument (portrait table index + 8), 0 = none
    pub portrait: i32,
    /// >= 0: enemy type; -2 - c: story character c; -1: unknown
    pub stats: i32,
    /// enemy types: HP, STR, POW, END of the type table `0x08711274`
    pub enemy: Option<[i32; 4]>,
}

#[derive(Clone, Debug, Default)]
pub struct ScoutData {
    pub db: Vec<DbEntry>,
    /// the selection's 8 remap tables (`0x0874D360`)
    pub remaps: Vec<Vec<u8>>,
    /// the info page's scroll arrows: OAM attr0 / attr1 (up, down)
    pub arrows: [(u16, u16); 2],
    /// the info page sprite's direction cycle (`0x0874D35C`)
    pub info_dirs: [u8; 4],
}

impl ScoutData {
    pub fn load(src: &dyn crate::content::ContentSource) -> Option<Arc<ScoutData>> {
        let v: serde_json::Value = serde_json::from_slice(&src.read("ui/menu/scouter.json")?).ok()?;
        let i = |x: &serde_json::Value| x.as_i64().unwrap_or(0) as i32;
        let db = v["db"]
            .as_array()?
            .iter()
            .map(|e| DbEntry {
                name: i(&e["name"]) as u16,
                desc: i(&e["desc"]) as u16,
                def: i(&e["def"]),
                portrait: i(&e["portrait"]),
                stats: i(&e["stats"]),
                enemy: e.get("enemy").filter(|x| x.is_object()).map(|x| [i(&x["hp"]), i(&x["str"]), i(&x["pow"]), i(&x["end"])]),
            })
            .collect();
        let remaps = v["remaps"].as_array()?.iter().map(|t| t.as_array().into_iter().flatten().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).collect();
        let mut arrows = [(0u16, 0u16); 2];
        for (k, x) in v["arrows"]["oam"].as_array().into_iter().flatten().take(2).enumerate() {
            let w = x.as_u64().unwrap_or(0) as u32;
            arrows[k] = (w as u16, (w >> 16) as u16);
        }
        let mut info_dirs = [0u8; 4];
        for (k, x) in v["info_dirs"].as_array().into_iter().flatten().take(4).enumerate() {
            info_dirs[k] = x.as_u64().unwrap_or(0) as u8;
        }
        Some(Arc::new(ScoutData { db, remaps, arrows, info_dirs }))
    }

    /// `0x08027948`: the database entry of an actor with sprite def `def` (enemies: of their type)
    pub fn lookup(&self, def: i32, enemy_type: Option<i32>) -> Option<usize> {
        let def = if def == 0x95 { 0x92 } else { def };
        match enemy_type {
            Some(t) => self.db.iter().position(|e| e.def == def && e.stats == t),
            None => self.db.iter().position(|e| e.def == def && e.stats < 0).or_else(|| self.db.iter().position(|e| e.def == def)),
        }
    }
}

/// one scouted object and its blip
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rec {
    pub db: u16,
    /// record +4: the enemy's HP, −1 = none
    pub hp: i32,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
    pub rect: [i32; 4],
    /// the blip: content sprite index, its idle animation
    pub sprite: i32,
    pub anim: ActAnim,
    /// the blip's priority byte (+8): 0x38, selected 0x28
    pub prio: u8,
    /// the selection effect's counter, None = no effect
    pub fx: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scout {
    pub recs: Vec<Rec>,
    /// the cursor: position, velocity (24.8; +0xC10..+0xC1C), ring counter (+0xC20), closing (+0xC24), the tile
    /// frame uploaded
    pub cx: i32,
    pub cy: i32,
    pub vx: i32,
    pub vy: i32,
    pub ring: i32,
    pub closing: bool,
    pub ring_frame: u8,
    /// the selected record (+0x518)
    pub sel: Option<u16>,
    /// what the screen shows: the previous update's OAM (cursor position, the blips' priority bytes) and blip tiles
    /// (cel, remap counter; uploaded at the next VBlank)
    pub oam: Option<((i32, i32), Vec<(u8, i32, Option<u32>)>)>,
}

pub fn data(w: &World) -> Option<Arc<ScoutData>> {
    super::mapscreen::md(w).and_then(|m| m.scout.clone())
}

impl Scout {
    /// construction (the records) + enter (the blips, the cursor)
    pub fn new(w: &mut World) -> Scout {
        let mut s = Scout { cx: 120 << 8, cy: 80 << 8, ..Default::default() };
        let Some(d) = data(w) else { return s };
        let cam = w.cameras[0].rect;
        let mut found = Vec::new();
        for e in w.ents.iter().rev() {
            if e.removed {
                continue;
            }
            let (a, ety, hp) = match &e.body {
                Body::Player(p) => (&p.a, None, -1),
                Body::Npc(n) => (&n.a, None, -1),
                Body::Enemy(en) => (&en.a, Some(en.ty), (en.hp1024 >> 10) as i32),
                _ => continue,
            };
            let Some(si) = a.sprite else { continue };
            let Some(db) = d.lookup(si + 9, ety) else { continue };
            found.push((db, hp, a.facing, (a.x >> 8) - cam.x0, (a.y >> 8) - cam.y0));
        }
        for (db, hp, dir, x, y) in found {
            let sprite = d.db[db].def - 9;
            let mut anim = ActAnim::of(0);
            anim.tick = w.rng.rand(255);
            let fb = w.data.sprite(sprite).and_then(|sp| sp.frame_box(anim.raw, dir, 0)).unwrap_or([0; 4]);
            let rect = [x + fb[0], y + fb[1], x + fb[0] + fb[2], y + fb[1] + fb[3]];
            s.recs.push(Rec { db: db as u16, hp, dir, x, y, rect, sprite, anim, prio: 0x38, fx: None });
        }
        s
    }

    /// `0x0802627E`: the selection under the cursor point
    fn select(&mut self) {
        let (px, py) = (self.cx >> 8, self.cy >> 8);
        let mut best: Option<(i64, usize)> = None;
        for (k, r) in self.recs.iter().enumerate() {
            let b = r.rect;
            if px >= b[0] && px < b[2] && py >= b[1] && py < b[3] {
                let (mx, my) = ((b[0] + b[2]) / 2, (b[1] + b[3]) / 2);
                let d = ((px - mx) as i64).pow(2) + ((py - my) as i64).pow(2);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, k));
                }
            }
        }
        let new = best.map(|b| b.1 as u16);
        if new != self.sel {
            if let Some(o) = self.sel {
                let r = &mut self.recs[o as usize];
                r.prio = 0x38;
                r.fx = None;
            }
            if let Some(n) = new {
                let r = &mut self.recs[n as usize];
                r.prio = 0x28;
                r.fx = Some(0);
            }
            self.sel = new;
        }
    }

    /// the update before the VBlank: the selection (+0x3C), then the objects
    pub fn update(&mut self, w: &World) {
        use crate::input::{A, DOWN, LEFT, RIGHT, UP};
        self.oam = Some((((self.cx >> 8) - 16, (self.cy >> 8) - 16), self.recs.iter().map(|r| (r.prio, r.anim.frame, r.fx)).collect()));
        self.select();
        let (held, pressed, released) = (w.menu.held, w.menu.pressed, w.menu.released);
        if pressed & A != 0 {
            self.closing = true;
            self.ring = 0;
        }
        if released & A != 0 {
            self.closing = false;
        }
        if self.closing {
            if self.ring < 4 {
                self.ring += 1;
                if self.ring & 1 == 0 {
                    self.ring_frame = (self.ring >> 1) as u8;
                }
            }
        } else if self.ring != 0 {
            self.ring -= 1;
            if self.ring & 1 == 0 {
                self.ring_frame = (self.ring >> 1) as u8;
            }
        }
        if held & LEFT != 0 {
            self.vx -= 0x80;
        }
        if held & RIGHT != 0 {
            self.vx += 0x80;
        }
        if held & UP != 0 {
            self.vy -= 0x80;
        }
        if held & DOWN != 0 {
            self.vy += 0x80;
        }
        self.cx += self.vx;
        self.cy += self.vy;
        self.vx = self.vx * 3 / 4;
        self.vy = self.vy * 3 / 4;
        if self.cx < 0x1000 {
            self.cx = 0x1000;
            self.vx = 0;
        }
        if self.cx > 0xE000 {
            self.cx = 0xE000;
            self.vx = 0;
        }
        if self.cy < 0x1000 {
            self.cy = 0x1000;
            self.vy = 0;
        }
        if self.cy > 0x9000 {
            self.cy = 0x9000;
            self.vy = 0;
        }
        for r in self.recs.iter_mut() {
            let _ = r.anim.tick(&w.data, Some(r.sprite), r.dir);
            if let Some(c) = r.fx.as_mut() {
                *c = c.wrapping_add(1);
            }
        }
    }

    /// the OBJs (OAM order: the cursor, then the blips last to first)
    pub fn objs(&self, w: &World, out: &mut Vec<MenuObj>) {
        let Some((cur, prios)) = self.oam.as_ref() else { return };
        let d = data(w);
        out.push(MenuObj { path: "ui/menu/scouter_cursor.png".into(), frame: self.ring_frame as u16, fw: 32, fh: 32, x: cur.0, y: cur.1, priority: 1, ..Default::default() });
        for (k, r) in self.recs.iter().enumerate().rev() {
            let Some(sp) = w.data.sprite(r.sprite) else { continue };
            let Some(an) = sp.anim_raw(r.anim.raw) else { continue };
            let steps = an.steps(r.dir);
            let (prio, frame, fx) = prios.get(k).copied().unwrap_or((r.prio, r.anim.frame, r.fx));
            let Some(st) = steps.get((frame.max(0) as usize).min(steps.len().saturating_sub(1))) else { continue };
            let prio = prio >> 4;
            let remap = match (fx, d.as_ref()) {
                (Some(c), Some(d)) => d.remaps.get(((c >> 2) & 7) as usize).cloned().unwrap_or_default(),
                _ => Vec::new(),
            };
            if let Some((hx, hy)) = st.attach {
                out.push(MenuObj { sheet: "effects/halo".into(), x: r.x + hx, y: r.y + hy, priority: prio, ..Default::default() });
            }
            out.push(MenuObj { sheet: format!("sprites/{}", sp.id), frame: st.frame, hflip: st.flip, x: r.x, y: r.y, priority: prio, remap, ..Default::default() });
            if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                out.push(MenuObj { sheet: format!("effects/{sh}"), x: r.x, y: r.y, priority: prio, ..Default::default() });
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the info page

/// the info page's scroll arrow (`0x080271D0`, vt `0x0804D60C`): while active (+0x20) the counter (+0x14) runs and
/// the tile frame is its bit 4, else counter 0 / frame 0; OAM word `0x0874D248[1 + type]`, priority 0
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Arrow {
    pub active: bool,
    pub t: u32,
    pub frame: u8,
}

impl Arrow {
    fn update(&mut self) {
        if self.active {
            self.t = self.t.wrapping_add(1);
            self.frame = ((self.t >> 4) & 1) as u8;
        } else {
            self.t = 0;
            self.frame = 0;
        }
    }
}

/// The info page (`0x08025E50`, vt `0x0804A52C`, enter `0x08026048`; the scouted bit GD +0xC5 + db / 8 set):
/// BG3 the 240x160 picture `0x08732384` with the content (`0x08024D94`: the name centred at (0x38, 0x90), the
/// labels (strings 760 764 768 772) at x 0x70, y 0x16 0x20 0x2A 0x34, the values right-aligned at x 0xA0 (`%d`,
/// `??` for −1; characters: the record's HP and RecalcStats STR / POW / END, enemies: the record's HP else the
/// type's, the type's STR / POW / END), the portrait at (168, 8) or "No Portrait Data" (string 776) at (0xC8,
/// 0x22)); BG2 the description window (15, 11)-(28, 18) (the description in the dialog font wrapped to 104 px,
/// y 0xE, 16 px lines, rows scroll..scroll + 56); the database sprite (`0x080272EC`: idle (its enter: rand(255)),
/// then walking when it has a walk (vt+0x28(2)); every update the direction `0x0874D35C[(c >> 7) & 3]` and the
/// position from the cel's frame record: x = 0x38 − w / 2 − ox, y = 0x38 − h / 2 − oy (h > 32) or 0x40); the two
/// arrows. Step (`0x08025FAC`): UP / DOWN held scroll 1 px within 0..max (lines · 16 − 56); B: back to a new
/// Scouter after the fade (+0xC, `0x0802601A`); A: close.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Info {
    pub db: u16,
    /// the record's HP (−1: none)
    pub hp: i32,
    pub scroll: i32,
    pub max: i32,
    pub back: bool,
    pub sprite: i32,
    pub anim: ActAnim,
    pub dir: u8,
    pub c: u32,
    pub x: i32,
    pub y: i32,
    /// up (+0x24), down (+0x28)
    pub arrows: [Arrow; 2],
    /// the sprite as the screen shows it (the previous update): x, y, direction, cel
    pub oam: Option<(i32, i32, u8, i32)>,
}

const DESC_W: i32 = 0x68;
const DESC_H: i32 = 0x38;

impl Info {
    pub fn new(w: &mut World, rec: &Rec) -> Info {
        let mut s = Info { db: rec.db, hp: rec.hp, ..Default::default() };
        // the scouted bit (GD +0xC5 + db / 8: story flag 0x380 + db)
        w.story.set_flag(0x380 + rec.db as i32, true);
        let Some(d) = data(w) else { return s };
        let e = d.db[rec.db as usize].clone();
        if let Some(md) = w.data.menu.clone() {
            let lines = super::text::count_lines(md.text.font(wf_view::MenuFont::Dialog), md.text.s(e.desc as usize), DESC_W);
            s.max = (lines * 16 - DESC_H).max(0);
        }
        s.sprite = e.def - 9;
        // the actor's idle enter, then vt+0x28(2): the walk when the sprite has one
        let mut anim = ActAnim::of(0);
        anim.tick = w.rng.rand(255);
        if w.data.sprite(s.sprite).is_some_and(|sp| sp.anim_raw(2).is_some()) {
            anim = ActAnim::of(1);
        }
        s.anim = anim;
        s.place(w);
        s.arrows = [Arrow { active: s.scroll > 0, ..Default::default() }, Arrow { active: s.scroll < s.max, ..Default::default() }];
        s
    }

    /// `0x080272AC`: the position from the current cel's frame record
    fn place(&mut self, w: &World) {
        let fb = w.data.sprite(self.sprite).and_then(|sp| sp.frame_box(self.anim.raw, self.dir, self.anim.frame)).unwrap_or([0; 4]);
        self.x = 0x38 - fb[2] / 2 - fb[0];
        self.y = if fb[3] > 32 { 0x38 - fb[3] / 2 - fb[1] } else { 0x40 };
    }

    pub fn update(&mut self, w: &World) {
        self.oam = Some((self.x, self.y, self.dir, self.anim.frame));
        let _ = self.anim.tick(&w.data, Some(self.sprite), self.dir);
        self.c = self.c.wrapping_add(1);
        if let Some(d) = data(w) {
            self.dir = d.info_dirs[((self.c >> 7) & 3) as usize];
        }
        self.place(w);
        for a in self.arrows.iter_mut() {
            a.update();
        }
    }

    /// `0x08025FAC` (the page's part): scrolling, B
    pub fn step(&mut self, w: &World) {
        use crate::input::{B, DOWN, UP};
        let held = w.menu.held;
        if held & UP != 0 && self.scroll > 0 {
            self.scroll -= 1;
        }
        if held & DOWN != 0 && self.scroll < self.max {
            self.scroll += 1;
        }
        self.arrows[0].active = self.scroll > 0;
        self.arrows[1].active = self.scroll < self.max;
        if w.menu.pressed & B != 0 {
            self.back = true;
        }
    }

    /// the values: HP, STR, POW, END (−1 = `??`)
    fn values(&self, w: &World, e: &DbEntry) -> [i32; 4] {
        if e.stats >= 0 {
            let t = e.enemy.unwrap_or([-1; 4]);
            return [if self.hp == -1 { t[0] } else { self.hp }, t[1], t[2], t[3]];
        }
        if e.stats == -1 {
            return [self.hp, -1, -1, -1];
        }
        let ci = (-2 - e.stats) as usize;
        let Some(cs) = w.story.chars.get(ci) else { return [-1; 4] };
        let eff = super::status::effective(w, cs);
        [if self.hp == -1 { cs.rec.hp } else { self.hp }, eff[0], eff[1], eff[2]]
    }

    pub fn view(&self, w: &World, md: &super::MenuData) -> (Vec<wf_view::MenuLayer>, Vec<MenuObj>) {
        use super::text::{Arg, Canvas, Pen};
        use wf_view::{MenuFont, MenuLayer};
        let t = &md.text;
        let d = data(w);
        let e = d.as_ref().and_then(|d| d.db.get(self.db as usize).cloned()).unwrap_or_default();
        let mut cv = Canvas::new();
        cv.image("ui/menu/scouter_info.png", 0, 240, 160, 0, 0);
        let mut p = Pen::new(MenuFont::Small, 0x38, 0x90, 1, 2, Some(1));
        p.line_h = 8;
        cv.text(t, &mut p, t.s(e.name as usize));
        for (s, y) in [(760usize, 0x16), (764, 0x20), (768, 0x2A), (772, 0x34)] {
            let mut p = Pen::new(MenuFont::Small, 0x70, y, 0, 2, Some(1));
            p.line_h = 8;
            cv.text(t, &mut p, t.s(s));
        }
        let vals = self.values(w, &e);
        for (k, y) in [0x16, 0x20, 0x2A, 0x34].into_iter().enumerate() {
            let mut p = Pen::new(MenuFont::Small, 0xA0, y, 2, 2, Some(1));
            p.line_h = 8;
            if vals[k] == -1 {
                cv.text(t, &mut p, &[0x3F, 0x3F]);
            } else {
                cv.textf(t, &mut p, &[0x25, 0x64], &[Arg::I(vals[k])]);
            }
        }
        match md.portraits.get((e.portrait - 8).max(0) as usize).filter(|s| e.portrait >= 8 && !s.is_empty()) {
            Some(path) => cv.image(path, 0, 64, 64, 168, 8),
            None => {
                let mut p = Pen::new(MenuFont::Small, 0xC8, 0x22, 1, 2, Some(1));
                p.line_h = 8;
                cv.text_wrapped(t, &mut p, 0x38, t.s(776));
            }
        }
        let bg3 = MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 3, ops: cv.ops };
        let mut dc = Canvas::new();
        let mut p = Pen::new(MenuFont::Dialog, 0, 0xE - self.scroll, 0, 2, None);
        p.line_h = 16;
        p.clip = [0, 0, DESC_W, DESC_H];
        dc.text_wrapped(t, &mut p, DESC_W, t.s(e.desc as usize));
        let bg2 = MenuLayer { w: DESC_W as u16, h: DESC_H as u16, x: 120, y: 88, priority: 3, ops: dc.ops };
        let mut objs = Vec::new();
        if let Some(d) = d.as_ref() {
            // the arrows (OAM words 0x0874D248 +8 up, +4 down; frames of the up tiles)
            for (k, a) in self.arrows.iter().enumerate() {
                let (a0, a1) = d.arrows[1 - k];
                objs.push(MenuObj {
                    path: "ui/menu/scouter_arrows.png".into(),
                    frame: a.frame as u16,
                    fw: 8,
                    fh: 8,
                    x: (a1 & 0x1FF) as i32,
                    y: (a0 & 0xFF) as i32,
                    priority: 0,
                    hflip: a1 & 0x1000 != 0,
                    vflip: a1 & 0x2000 != 0,
                    ..Default::default()
                });
            }
        }
        if let (Some((x, y, dir, frame)), Some(sp)) = (self.oam, w.data.sprite(self.sprite)) {
            if let Some(an) = sp.anim_raw(self.anim.raw) {
                let steps = an.steps(dir);
                if let Some(st) = steps.get((frame.max(0) as usize).min(steps.len().saturating_sub(1))) {
                    if let Some((hx, hy)) = st.attach {
                        objs.push(MenuObj { sheet: "effects/halo".into(), x: x + hx, y: y + hy, priority: 3, ..Default::default() });
                    }
                    objs.push(MenuObj { sheet: format!("sprites/{}", sp.id), frame: st.frame, hflip: st.flip, x, y, priority: 3, ..Default::default() });
                    if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                        objs.push(MenuObj { sheet: format!("effects/{sh}"), x, y, priority: 3, ..Default::default() });
                    }
                }
            }
        }
        (vec![bg3, bg2], objs)
    }
}
