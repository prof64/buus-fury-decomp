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
    /// the database page's left / right arrows: OAM attr0 / attr1
    pub lr_arrows: [(u16, u16); 2],
    /// the info page sprite's direction cycle (`0x0874D35C`)
    pub info_dirs: [u8; 4],
}

impl ScoutData {
    pub fn load(src: &dyn crate::content::ContentSource) -> Option<Arc<ScoutData>> {
        let v: serde_json::Value =
            serde_json::from_slice(&src.read("ui/menu/scouter.json")?).ok()?;
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
                enemy: e
                    .get("enemy")
                    .filter(|x| x.is_object())
                    .map(|x| [i(&x["hp"]), i(&x["str"]), i(&x["pow"]), i(&x["end"])]),
            })
            .collect();
        let remaps = v["remaps"]
            .as_array()?
            .iter()
            .map(|t| {
                t.as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| x.as_u64().unwrap_or(0) as u8)
                    .collect()
            })
            .collect();
        let mut arrows = [(0u16, 0u16); 2];
        for (k, x) in v["arrows"]["oam"]
            .as_array()
            .into_iter()
            .flatten()
            .take(2)
            .enumerate()
        {
            let w = x.as_u64().unwrap_or(0) as u32;
            arrows[k] = (w as u16, (w >> 16) as u16);
        }
        let mut lr_arrows = [(0u16, 0u16); 2];
        for (k, x) in v["arrows"]["lr_oam"]
            .as_array()
            .into_iter()
            .flatten()
            .take(2)
            .enumerate()
        {
            let w = x.as_u64().unwrap_or(0) as u32;
            lr_arrows[k] = (w as u16, (w >> 16) as u16);
        }
        let mut info_dirs = [0u8; 4];
        for (k, x) in v["info_dirs"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4)
            .enumerate()
        {
            info_dirs[k] = x.as_u64().unwrap_or(0) as u8;
        }
        Some(Arc::new(ScoutData {
            db,
            remaps,
            arrows,
            lr_arrows,
            info_dirs,
        }))
    }

    /// `0x08027948`: the database entry of an actor with sprite def `def` (enemies: of their type)
    pub fn lookup(&self, def: i32, enemy_type: Option<i32>) -> Option<usize> {
        let def = if def == 0x95 { 0x92 } else { def };
        match enemy_type {
            Some(t) => self.db.iter().position(|e| e.def == def && e.stats == t),
            None => self
                .db
                .iter()
                .position(|e| e.def == def && e.stats < 0)
                .or_else(|| self.db.iter().position(|e| e.def == def)),
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
        let mut s = Scout {
            cx: 120 << 8,
            cy: 80 << 8,
            ..Default::default()
        };
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
            let Some(db) = d.lookup(si + 9, ety) else {
                continue;
            };
            found.push((db, hp, a.facing, (a.x >> 8) - cam.x0, (a.y >> 8) - cam.y0));
        }
        for (db, hp, dir, x, y) in found {
            let sprite = d.db[db].def - 9;
            let mut anim = ActAnim::of(0);
            anim.tick = w.rng.rand(255);
            let fb = w
                .data
                .sprite(sprite)
                .and_then(|sp| sp.frame_box(anim.raw, dir, 0))
                .unwrap_or([0; 4]);
            let rect = [x + fb[0], y + fb[1], x + fb[0] + fb[2], y + fb[1] + fb[3]];
            s.recs.push(Rec {
                db: db as u16,
                hp,
                dir,
                x,
                y,
                rect,
                sprite,
                anim,
                prio: 0x38,
                fx: None,
            });
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
        self.oam = Some((
            ((self.cx >> 8) - 16, (self.cy >> 8) - 16),
            self.recs
                .iter()
                .map(|r| (r.prio, r.anim.frame, r.fx))
                .collect(),
        ));
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
        let Some((cur, prios)) = self.oam.as_ref() else {
            return;
        };
        let d = data(w);
        out.push(MenuObj {
            path: "ui/menu/scouter_cursor.png".into(),
            frame: self.ring_frame as u16,
            fw: 32,
            fh: 32,
            x: cur.0,
            y: cur.1,
            priority: 1,
            ..Default::default()
        });
        for (k, r) in self.recs.iter().enumerate().rev() {
            let Some(sp) = w.data.sprite(r.sprite) else {
                continue;
            };
            let Some(an) = sp.anim_raw(r.anim.raw) else {
                continue;
            };
            let steps = an.steps(r.dir);
            let (prio, frame, fx) = prios
                .get(k)
                .copied()
                .unwrap_or((r.prio, r.anim.frame, r.fx));
            let Some(st) = steps.get((frame.max(0) as usize).min(steps.len().saturating_sub(1)))
            else {
                continue;
            };
            let prio = prio >> 4;
            let remap = match (fx, d.as_ref()) {
                (Some(c), Some(d)) => d
                    .remaps
                    .get(((c >> 2) & 7) as usize)
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            if let Some((hx, hy)) = st.attach {
                out.push(MenuObj {
                    sheet: "effects/halo".into(),
                    x: r.x + hx,
                    y: r.y + hy,
                    priority: prio,
                    ..Default::default()
                });
            }
            out.push(MenuObj {
                sheet: format!("sprites/{}", sp.id),
                frame: st.frame,
                hflip: st.flip,
                x: r.x,
                y: r.y,
                priority: prio,
                remap,
                ..Default::default()
            });
            if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                out.push(MenuObj {
                    sheet: format!("effects/{sh}"),
                    x: r.x + (sp.shadow_off.0 as i8) as i32,
                    y: r.y + (sp.shadow_off.1 as i8) as i32,
                    priority: prio,
                    ..Default::default()
                });
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
/// labels (strings 190..193) at x 0x70, y 0x16 0x20 0x2A 0x34, the values right-aligned at x 0xA0 (`%d`,
/// `??` for −1; characters: the record's HP and RecalcStats STR / POW / END, enemies: the record's HP else the
/// type's, the type's STR / POW / END), the portrait at (168, 8) or "No Portrait Data" (string 194) at (0xC8,
/// 0x22)); BG2 the description window (15, 11)-(28, 18) (the description in the dialog font wrapped to 104 px,
/// y 0xE, 16 px lines, rows scroll..scroll + 56); the database sprite (`0x080272EC`: idle (its enter: rand(255)),
/// then the walk state ticking the idle's cels (vt+0x28(2)); every update the direction `0x0874D35C[(c >> 7) & 3]` and the
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
        let mut s = Info {
            db: rec.db,
            hp: rec.hp,
            ..Default::default()
        };
        // the scouted bit (GD +0xC5 + db / 8: story flag 0x380 + db)
        w.story.set_flag(0x380 + rec.db as i32, true);
        let Some(d) = data(w) else { return s };
        let e = d.db[rec.db as usize].clone();
        if let Some(md) = w.data.menu.clone() {
            let lines = super::text::count_lines(
                md.text.font(wf_view::MenuFont::Dialog),
                md.text.s(e.desc as usize),
                DESC_W,
            );
            s.max = (lines * 16 - DESC_H).max(0);
        }
        s.set_sprite(w, &e, true);
        s.arrows = [
            Arrow {
                active: s.scroll > 0,
                ..Default::default()
            },
            Arrow {
                active: s.scroll < s.max,
                ..Default::default()
            },
        ];
        s
    }

    /// the database sprite `0x080272EC`: the actor's idle enter (rand(255)), then (`walk`) vt+0x28(2)
    /// (`0x0802735C`): the walk state (0x03001B58) when the sprite has a walk; the state ticks (8 per cel, looping)
    /// the idle's cels, the picture stays the idle's
    fn set_sprite(&mut self, w: &mut World, e: &DbEntry, walk: bool) {
        self.sprite = e.def - 9;
        let mut anim = ActAnim::of(0);
        anim.tick = w.rng.rand(255);
        if walk
            && w.data
                .sprite(self.sprite)
                .is_some_and(|sp| sp.anim_raw(2).is_some())
        {
            anim = ActAnim {
                raw: 1,
                id: 1,
                ..Default::default()
            };
        }
        self.anim = anim;
        self.place(w);
    }

    /// the database page's sprite of entry `db` (no scouted bit, no description state)
    pub fn sprite(w: &mut World, db: u16, walk: bool) -> Info {
        let mut s = Info {
            db,
            hp: -1,
            ..Default::default()
        };
        if let Some(e) = data(w).and_then(|d| d.db.get(db as usize).cloned()) {
            s.set_sprite(w, &e, walk);
        }
        s
    }

    /// `0x080272AC`: the position from the current cel's frame record
    fn place(&mut self, w: &World) {
        let fb = w
            .data
            .sprite(self.sprite)
            .and_then(|sp| sp.frame_box(self.anim.raw, self.dir, self.anim.frame))
            .unwrap_or([0; 4]);
        self.x = 0x38 - fb[2] / 2 - fb[0];
        self.y = if fb[3] > 32 {
            0x38 - fb[3] / 2 - fb[1]
        } else {
            0x40
        };
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
    fn values(w: &World, e: &DbEntry, hp: i32) -> [i32; 4] {
        if e.stats >= 0 {
            let t = e.enemy.unwrap_or([-1; 4]);
            return [if hp == -1 { t[0] } else { hp }, t[1], t[2], t[3]];
        }
        if e.stats == -1 {
            return [hp, -1, -1, -1];
        }
        let ci = (-2 - e.stats) as usize;
        let Some(cs) = w.story.chars.get(ci) else {
            return [-1; 4];
        };
        let eff = super::status::effective(w, cs);
        [
            if hp == -1 { cs.rec.hp } else { hp },
            eff[0],
            eff[1],
            eff[2],
        ]
    }

    /// BG3: the picture `0x08732384` with the content of `0x08024D94` (entry `e`, none: the labels with `??` and
    /// "No Portrait Data")
    pub fn picture(w: &World, md: &super::MenuData, bg: &str, e: Option<&DbEntry>, hp: i32) -> wf_view::MenuLayer {
        use super::text::{Arg, Canvas, Pen};
        use wf_view::{MenuFont, MenuLayer};
        let t = &md.text;
        let none = DbEntry::default();
        let mut cv = Canvas::new();
        cv.image(bg, 0, 240, 160, 0, 0);
        if let Some(e) = e {
            let mut p = Pen::new(MenuFont::Small, 0x38, 0x90, 1, 2, Some(1));
            p.line_h = 8;
            cv.text(t, &mut p, t.s(e.name as usize));
        }
        for (s, y) in [(190usize, 0x16), (191, 0x20), (192, 0x2A), (193, 0x34)] {
            let mut p = Pen::new(MenuFont::Small, 0x70, y, 0, 2, Some(1));
            p.line_h = 8;
            cv.text(t, &mut p, t.s(s));
        }
        let vals = e.map(|e| Info::values(w, e, hp)).unwrap_or([-1; 4]);
        let e = e.unwrap_or(&none);
        for (k, y) in [0x16, 0x20, 0x2A, 0x34].into_iter().enumerate() {
            let mut p = Pen::new(MenuFont::Small, 0xA0, y, 2, 2, Some(1));
            p.line_h = 8;
            if vals[k] == -1 {
                cv.text(t, &mut p, &[0x3F, 0x3F]);
            } else {
                cv.textf(t, &mut p, &[0x25, 0x64], &[Arg::I(vals[k])]);
            }
        }
        match md
            .portraits
            .get((e.portrait - 8).max(0) as usize)
            .filter(|s| e.portrait >= 8 && !s.is_empty())
        {
            Some(path) => cv.image(path, 0, 64, 64, 168, 8),
            None => {
                let mut p = Pen::new(MenuFont::Small, 0xC8, 0x22, 1, 2, Some(1));
                p.line_h = 8;
                cv.text_wrapped(t, &mut p, 0x38, t.s(194));
            }
        }
        MenuLayer {
            w: 240,
            h: 160,
            x: 0,
            y: 0,
            priority: 3,
            ops: cv.ops,
        }
    }

    pub fn view(&self, w: &World, md: &super::MenuData) -> (Vec<wf_view::MenuLayer>, Vec<MenuObj>) {
        use super::text::{Canvas, Pen};
        use wf_view::{MenuFont, MenuLayer};
        let t = &md.text;
        let d = data(w);
        let e = d
            .as_ref()
            .and_then(|d| d.db.get(self.db as usize).cloned())
            .unwrap_or_default();
        let bg3 = Info::picture(w, md, "ui/menu/scouter_info.png", Some(&e), self.hp);
        let mut dc = Canvas::new();
        let mut p = Pen::new(MenuFont::Dialog, 0, 0xE - self.scroll, 0, 2, None);
        p.line_h = 16;
        p.clip = [0, 0, DESC_W, DESC_H];
        dc.text_wrapped(t, &mut p, DESC_W, t.s(e.desc as usize));
        let bg2 = MenuLayer {
            w: DESC_W as u16,
            h: DESC_H as u16,
            x: 120,
            y: 88,
            priority: 3,
            ops: dc.ops,
        };
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
        self.sprite_objs(w, &mut objs);
        (vec![bg3, bg2], objs)
    }

    /// the database sprite's OBJs (halo, body, shadow) as the screen shows them
    pub fn sprite_objs(&self, w: &World, objs: &mut Vec<MenuObj>) {
        if let (Some((x, y, dir, frame)), Some(sp)) = (self.oam, w.data.sprite(self.sprite)) {
            if let Some(an) = sp.anim_raw(self.anim.raw) {
                let steps = an.steps(dir);
                if let Some(st) =
                    steps.get((frame.max(0) as usize).min(steps.len().saturating_sub(1)))
                {
                    if let Some((hx, hy)) = st.attach {
                        objs.push(MenuObj {
                            sheet: "effects/halo".into(),
                            x: x + hx,
                            y: y + hy,
                            priority: 3,
                            ..Default::default()
                        });
                    }
                    objs.push(MenuObj {
                        sheet: format!("sprites/{}", sp.id),
                        frame: st.frame,
                        hflip: st.flip,
                        x,
                        y,
                        priority: 3,
                        ..Default::default()
                    });
                    if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                        objs.push(MenuObj {
                            sheet: format!("effects/{sh}"),
                            x: x + (sp.shadow_off.0 as i8) as i32,
                            y: y + (sp.shadow_off.1 as i8) as i32,
                            priority: 3,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the Computer Room's database

/// The Scouter database (the Computer Room's computer, 4-22: ctor `0x08024F3C`, vt `0x0804A950`, enter
/// `0x080255C4`): the info page's picture and content (`0x08024D94`, the HP from the type) of the selected entry,
/// its sprite, and in the description window (BG2, (120, 88) 104x56) either the list of the scouted names or the
/// entry's description.
/// * Construction: the scouted db ids (GD +0xC5 bits, db 0 .. 0xE7) in db order; groups = runs of consecutive ids
///   with the same name string (list +0x10 / +0x14.., groups +0x3B4); the first group, its first entry (-1: none).
/// * Content (`0x08025056`, after every change): the picture with the entry's content; the sprite made again
///   (`0x080272EC`: idle, rand(255); in the description the walk state, vt+0x28(2)); the list (7 rows from
///   max(0, group - 3 or n - 7), names centred at x 0x34, y 8k + 7, the selected group colour 2, the others 0x75;
///   empty: "Database Empty" (string 195) at y 0x20, colour 3) or the description (`0x08025004`: the dialog font
///   wrapped to 104 px, rows scroll .. scroll + 56); the arrows: up = group > 2 / scroll > 0, down =
///   group < n - 4 / scroll < max, left / right = the group has more than one entry (list mode).
/// * Step (`0x08025510`): LEFT / RIGHT pressed cycle the group's entries (wrapping; sound 39); then the list
///   (`0x08025404`: UP / DOWN pressed move the group (sound 39); A starts the switch to the description: the fade
///   with BLDCNT 0x3E41 - only BG0, which is off: a pause while EVA runs down) or the description (`0x080254A8`:
///   UP / DOWN held scroll 1 px; A = the close, vt +0x1C `0x080253EA`: in the description the switch back, else
///   the screen's close). The done check (`0x08025354`) switches the mode when the switch's fade ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Database {
    pub list: Vec<u16>,
    pub groups: Vec<(i32, i32)>,
    pub group: i32,
    pub entry: i32,
    /// +0xE: the description shown
    pub desc: bool,
    /// +0xD: a mode switch pending (kept through the description: its close switches back)
    pub toggle: bool,
    pub scroll: i32,
    pub max: i32,
    /// the entry's sprite (the info page's sprite state)
    pub spr: Option<Box<Info>>,
    /// the sprite replaced by this iteration's content: its removal (and the new one's addition) shows from the
    /// next update - the screen keeps the old one this frame
    #[serde(default)]
    pub prev: Option<Box<Info>>,
    #[serde(default)]
    pub shown_prev: Option<Box<Info>>,
    /// the first content's sprite is made at the page's enter (after the fade-out, the world saved)
    #[serde(default)]
    pub entered: bool,
    /// down (type 0), up (1), left (2), right (3)
    pub arrows: [Arrow; 4],
}

impl Database {
    pub fn new(w: &mut World) -> Database {
        let mut s = Database::default();
        let Some(d) = data(w) else { return s };
        for db in 0..d.db.len().min(0xE8) {
            if w.story.flag(0x380 + db as i32) {
                s.list.push(db as u16);
            }
        }
        let mut name = 0u16;
        for (k, &db) in s.list.iter().enumerate() {
            let n = d.db[db as usize].name;
            if k == 0 || n != name {
                name = n;
                s.groups.push((k as i32, k as i32));
            } else if let Some(g) = s.groups.last_mut() {
                g.1 = k as i32;
            }
        }
        s.entry = s.groups.first().map(|g| g.0).unwrap_or(-1);
        // the arrows start active (+0x20 = 1); the content sets them
        s.arrows = std::array::from_fn(|_| Arrow {
            active: true,
            ..Default::default()
        });
        s.content(w, false);
        s
    }

    /// the enter's content (`0x080255C4` -> `0x08025056`): the first sprite
    pub fn enter(&mut self, w: &mut World) {
        if !self.entered {
            self.entered = true;
            // the four arrows' tiles (0x40 each, types 0..3), then the sprite's
            for k in 0..4 {
                crate::vram::acquire(w, crate::vram::Owner::DbArrow(k), 0x40);
            }
            self.spr = match self.db() {
                Some(db) => Some(Box::new(Info::sprite(w, db, self.desc))),
                None => None,
            };
            if let Some(sp) = self.spr.as_ref().map(|s| s.sprite) {
                crate::vram::figure_created(w, 0, sp);
            }
            // its first update runs in this iteration; it is in the OAM from the next one
            if let Some(s) = self.spr.as_mut() {
                s.update(w);
                s.oam = None;
            }
        }
    }

    fn db(&self) -> Option<u16> {
        if self.entry < 0 {
            return None;
        }
        self.list.get(self.entry as usize).copied()
    }

    /// `0x08025056`
    pub fn redraw(&mut self, w: &mut World) {
        self.content(w, true);
    }

    fn content(&mut self, w: &mut World, sprite: bool) {
        let n = self.groups.len() as i32;
        if !self.desc {
            if n == 0 {
                self.arrows[2].active = false;
                self.arrows[3].active = false;
            }
            self.arrows[1].active = self.group > 2;
            self.arrows[0].active = n - 4 > self.group;
        }
        if sprite {
            let new = match self.db() {
                Some(db) => Some(Box::new(Info::sprite(w, db, self.desc))),
                None => None,
            };
            // the old object goes (`0x08011944`), the new one takes its tiles (`0x080272EC`)
            if let Some(sp) = self.spr.as_ref().map(|s| s.sprite) {
                crate::vram::figures_gone(w, &[sp]);
            }
            if let Some(sp) = new.as_ref().map(|s| s.sprite) {
                crate::vram::figure_created(w, 0, sp);
            }
            let old = std::mem::replace(&mut self.spr, new);
            if self.prev.is_none() {
                self.prev = old;
            }
        }
        if self.desc {
            self.desc_arrows();
        } else {
            let more = self
                .groups
                .get(self.group.max(0) as usize)
                .is_some_and(|g| g.0 != g.1);
            self.arrows[2].active = more;
            self.arrows[3].active = more;
        }
    }

    /// `0x08025004`'s arrows
    fn desc_arrows(&mut self) {
        self.arrows[1].active = self.scroll > 0;
        self.arrows[0].active = self.scroll < self.max;
    }

    pub fn update(&mut self, w: &World) {
        self.shown_prev = self.prev.take().map(|mut p| {
            p.update(w);
            p
        });
        if let Some(s) = self.spr.as_mut() {
            s.update(w);
        }
        for a in self.arrows.iter_mut() {
            a.update();
        }
    }

    /// `0x08025510` (the page's step); true = the mode switch starts (the screen's fade, BLDCNT 0x3E41)
    pub fn step(&mut self, w: &mut World) -> bool {
        use crate::input::{A, DOWN, LEFT, RIGHT, UP};
        let (pressed, held) = (w.menu.pressed, w.menu.held);
        if let Some(&(f, l)) = self.groups.get(self.group.max(0) as usize) {
            if f != l {
                if pressed & LEFT != 0 {
                    self.entry -= 1;
                    if f > self.entry {
                        self.entry = l;
                    }
                    self.redraw(w);
                    w.sfx(super::SFX_CURSOR, 64);
                }
                if pressed & RIGHT != 0 {
                    self.entry += 1;
                    if l < self.entry {
                        self.entry = f;
                    }
                    self.redraw(w);
                    w.sfx(super::SFX_CURSOR, 64);
                }
            }
        }
        if self.desc {
            if held & UP != 0 && self.scroll > 0 {
                self.scroll -= 1;
                self.desc_arrows();
            }
            if held & DOWN != 0 && self.scroll < self.max {
                self.scroll += 1;
                self.desc_arrows();
            }
            return pressed & A != 0;
        }
        let n = self.groups.len() as i32;
        if pressed & UP != 0 && self.group != 0 {
            self.group -= 1;
            self.entry = self.groups[self.group as usize].0;
            self.redraw(w);
            w.sfx(super::SFX_CURSOR, 64);
        }
        if pressed & DOWN != 0 && n - 1 > self.group {
            self.group += 1;
            self.entry = self.groups[self.group as usize].0;
            self.redraw(w);
            w.sfx(super::SFX_CURSOR, 64);
        }
        if n != 0 && pressed & A != 0 {
            self.toggle = true;
            return true;
        }
        false
    }

    /// the done check (`0x08025354`) when a fade ended: false = no switch pending (the screen ends)
    pub fn switch(&mut self, w: &mut World) -> bool {
        if !self.toggle {
            return false;
        }
        if self.desc {
            self.desc = false;
            self.toggle = false;
        } else {
            let desc = self
                .db()
                .and_then(|db| data(w).map(|d| d.db[db as usize].desc))
                .unwrap_or(0);
            let lines = w
                .data
                .menu
                .as_ref()
                .map(|md| {
                    super::text::count_lines(
                        md.text.font(wf_view::MenuFont::Dialog),
                        md.text.s(desc as usize),
                        DESC_W,
                    )
                })
                .unwrap_or(0);
            self.max = (lines * 16 - DESC_H).max(0);
            self.scroll = 0;
            self.desc = true;
        }
        self.redraw(w);
        true
    }

    pub fn view(&self, w: &World, md: &super::MenuData) -> (Vec<wf_view::MenuLayer>, Vec<MenuObj>) {
        use super::text::{Canvas, Pen};
        use wf_view::{MenuFont, MenuLayer};
        let t = &md.text;
        let d = data(w);
        let e = self
            .db()
            .and_then(|db| d.as_ref().and_then(|d| d.db.get(db as usize).cloned()));
        let bg3 = Info::picture(w, md, "ui/menu/scouter_db.png", e.as_ref(), -1);
        let mut dc = Canvas::new();
        if self.desc {
            let mut p = Pen::new(MenuFont::Dialog, 0, 0xE - self.scroll, 0, 2, None);
            p.line_h = 16;
            p.clip = [0, 0, DESC_W, DESC_H];
            let s = e.as_ref().map(|e| e.desc).unwrap_or(0) as usize;
            dc.text_wrapped(t, &mut p, DESC_W, t.s(s));
        } else if self.groups.is_empty() {
            let mut p = Pen::new(MenuFont::Small, 0x34, 0x20, 1, 3, Some(1));
            p.line_h = 8;
            dc.text(t, &mut p, t.s(195));
        } else if let Some(d) = d.as_ref() {
            let n = self.groups.len() as i32;
            let first = if n > self.group + 4 { self.group - 3 } else { n - 7 }.max(0);
            for k in 0..7 {
                let g = first + k;
                if g >= n {
                    break;
                }
                let db = self.list[self.groups[g as usize].0 as usize];
                let colour = if g == self.group { 2 } else { 0x75 };
                let mut p = Pen::new(MenuFont::Small, 0x34, 8 * k + 7, 1, colour, Some(1));
                p.line_h = 8;
                dc.text(t, &mut p, t.s(d.db[db as usize].name as usize));
            }
        }
        let bg2 = MenuLayer {
            w: DESC_W as u16,
            h: DESC_H as u16,
            x: 120,
            y: 88,
            priority: 3,
            ops: dc.ops,
        };
        let mut objs = Vec::new();
        if let Some(d) = d.as_ref() {
            for (k, a) in self.arrows.iter().enumerate() {
                let ((a0, a1), frame) = match k {
                    0 | 1 => (d.arrows[k], a.frame),
                    _ => (d.lr_arrows[k - 2], 2 + a.frame),
                };
                objs.push(MenuObj {
                    path: "ui/menu/scouter_arrows.png".into(),
                    frame: frame as u16,
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
        if let Some(s) = self.shown_prev.as_ref().or(self.spr.as_ref()) {
            s.sprite_objs(w, &mut objs);
        }
        (vec![bg3, bg2], objs)
    }
}
