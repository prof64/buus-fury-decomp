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
    /// the OAM shown: (cursor position, the blips' priority bytes) of the previous update
    pub oam: Option<((i32, i32), Vec<u8>)>,
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
            eprintln!("SCOUT {} db {db} facing {} pos {} {} cam {:?} anim {:?}", e.kind(), a.facing, a.x >> 8, a.y >> 8, cam, a.anim);
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
        self.oam = Some((((self.cx >> 8) - 16, (self.cy >> 8) - 16), self.recs.iter().map(|r| r.prio).collect()));
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
            let Some(st) = steps.get((r.anim.frame.max(0) as usize).min(steps.len().saturating_sub(1))) else { continue };
            let prio = prios.get(k).copied().unwrap_or(r.prio) >> 4;
            let remap = match (r.fx, d.as_ref()) {
                (Some(c), Some(d)) => d.remaps.get((((c + std::env::var("SCO").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0)) >> 2) & 7) as usize).cloned().unwrap_or_default(),
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
