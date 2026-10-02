//! Tool overlays drawn over a rendered view (never part of the simulation): training boxes and AI labels read
//! from the wf-core world, the input display, status text. Text uses the game's `menu` font (wf-ui export) when
//! the content has it, the built-in tiny font otherwise.

use wf_content::Packs;
use wf_core::session::Session;
use wf_core::world::Body;
use wf_render::font as tiny;
use wf_render::ui::Font;
use wf_view::buttons as b;

pub const HURT: u32 = 0xFF40_E060;
pub const HURT_INV: u32 = 0xFF2A_6A38;
pub const FOOT: u32 = 0xFF40_B0FF;
pub const SHOT_ENEMY: u32 = 0xFFFF_4848;
pub const SHOT_PLAYER: u32 = 0xFFFF_B030;
pub const ROCK: u32 = 0xFFB0_B0B0;
pub const ACCENT: u32 = 0xFFF0_A030;
pub const WHITE: u32 = 0xFFFF_FFFF;
pub const BAD: u32 = 0xFFFF_6A5B;

/// Text renderer (game font with a 1 px shadow, or the tiny font).
#[derive(Default)]
pub struct Text {
    font: Option<Option<Font>>,
}

impl Text {
    pub fn ensure(&mut self, packs: &Packs) {
        if self.font.is_none() {
            self.font = Some(Font::load(packs, "menu").ok());
        }
    }
    fn font(&self) -> Option<&Font> {
        self.font.as_ref().and_then(|f| f.as_ref())
    }
    pub fn line_h(&self) -> i32 {
        if self.font().is_some() {
            11
        } else {
            tiny::LINE
        }
    }
    pub fn width(&self, s: &str) -> i32 {
        match self.font() {
            Some(f) => f.width(s),
            None => tiny::text_width(s),
        }
    }
    /// `y` = top of the line.
    pub fn draw(&self, fb: &mut [u32], w: usize, h: usize, x: i32, y: i32, s: &str, color: u32) {
        match self.font() {
            Some(f) => {
                f.draw_argb(fb, w, h, s, x + 1, y + 10, 0xFF00_0000);
                f.draw_argb(fb, w, h, s, x, y + 9, color);
            }
            None => {
                tiny::draw_text(fb, w, h, x + 1, y + 1, s, 0xFF00_0000);
                tiny::draw_text(fb, w, h, x, y, s, color);
            }
        }
    }
    /// Text on a dark plate.
    pub fn plate(&self, fb: &mut [u32], w: usize, h: usize, x: i32, y: i32, s: &str, color: u32) {
        let tw = self.width(s);
        dim_rect(fb, w, h, x - 2, y - 1, tw + 5, self.line_h() + 1);
        self.draw(fb, w, h, x, y, s, color);
    }
}

pub fn dim_rect(fb: &mut [u32], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32) {
    for yy in y.max(0)..(y + rh).min(h as i32) {
        for xx in x.max(0)..(x + rw).min(w as i32) {
            let p = &mut fb[yy as usize * w + xx as usize];
            *p = 0xFF00_0000 | ((*p >> 2) & 0x003F_3F3F);
        }
    }
}

/// 1 px rectangle outline (x1 / y1 exclusive); `dashed` skips every other pixel pair.
pub fn outline(fb: &mut [u32], w: usize, h: usize, x0: i32, y0: i32, x1: i32, y1: i32, color: u32, dashed: bool) {
    let mut put = |x: i32, y: i32, k: i32| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && (!dashed || (k >> 1) & 1 == 0) {
            fb[y as usize * w + x as usize] = color;
        }
    };
    let (x1, y1) = (x1 - 1, y1 - 1);
    for x in x0..=x1 {
        put(x, y0, x);
        put(x, y1, x);
    }
    for y in y0..=y1 {
        put(x0, y, y);
        put(x1, y, y);
    }
}

/// Name of the AI node the enemy is currently running: follow the composite cursors from the state's root.
pub fn ai_label(s: &Session, en: &wf_core::enemy::Enemy) -> String {
    let Some(top) = en.ai.stack.last() else { return "-".into() };
    let mut n = 0usize;
    let mut path = Vec::new();
    for _ in 0..12 {
        let Some(node) = top.tree.nodes.get(n) else { break };
        path.push(node.name.as_str());
        let cur = top.rt.get(n).map(|r| r.i).unwrap_or(0);
        match node.kids.get(cur.max(0) as usize) {
            Some(&k) if !node.kids.is_empty() => n = k as usize,
            _ => break,
        }
    }
    let leaf = path.last().copied().unwrap_or("?");
    let in_state = s.world.frame as i64 - en.state_stamp;
    if (0..100_000).contains(&in_state) {
        format!("{:#x} {} {}f", top.state, leaf, in_state)
    } else {
        format!("{:#x} {}", top.state, leaf)
    }
}

/// Hitbox / hurtbox / footprint boxes and enemy AI labels for the view rectangle at world (`vx`, `vy`).
pub fn training(text: &Text, s: &Session, fb: &mut [u32], w: usize, h: usize, vx: i32, vy: i32) {
    let wd = &s.world;
    for (i, e) in wd.ents.iter().enumerate() {
        if e.removed {
            continue;
        }
        match &e.body {
            Body::Player(p) => {
                let bb = p.a.body_box();
                let fbx = p.a.foot_box();
                outline(fb, w, h, fbx.x0 - vx, fbx.y0 - vy, fbx.x1 - vx, fbx.y1 - vy, FOOT, false);
                outline(fb, w, h, bb.x0 - vx, bb.y0 - vy, bb.x1 - vx, bb.y1 - vy, if p.a.hittable { HURT } else { HURT_INV }, !p.a.hittable);
                let o = wf_core::observe::player_obs(wd, i);
                text.draw(fb, w, h, bb.x0 - vx, bb.y0 - vy - text.line_h() - 1, &format!("P{} {}", p.slot + 1, o.state), WHITE);
            }
            Body::Enemy(en) => {
                if en.defeated {
                    continue;
                }
                let bb = en.a.body_box();
                let fbx = en.a.foot_box();
                outline(fb, w, h, fbx.x0 - vx, fbx.y0 - vy, fbx.x1 - vx, fbx.y1 - vy, FOOT, false);
                outline(fb, w, h, bb.x0 - vx, bb.y0 - vy, bb.x1 - vx, bb.y1 - vy, if en.a.hittable { HURT } else { HURT_INV }, !en.a.hittable);
                if !en.part {
                    let name = wd.data.enemy(en.ty).map(|t| t.name.clone()).unwrap_or_else(|| format!("t{}", en.ty));
                    let y = bb.y0 - vy - 2 * text.line_h() - 2;
                    text.draw(fb, w, h, bb.x0 - vx, y, &format!("{name} {}/{}", en.hp(), en.max_hp1024 / 1024), WHITE);
                    text.draw(fb, w, h, bb.x0 - vx, y + text.line_h(), &ai_label(s, en), ACCENT);
                }
            }
            Body::Hand(hd) => {
                let bb = hd.a.body_box();
                outline(fb, w, h, bb.x0 - vx, bb.y0 - vy, bb.x1 - vx, bb.y1 - vy, HURT, false);
            }
            Body::Shot(sh) => {
                let r = wf_core::shots::rect(wd, i);
                if r.x1 > r.x0 && r.y1 > r.y0 {
                    outline(fb, w, h, r.x0 - vx, r.y0 - vy, r.x1 - vx, r.y1 - vy, if sh.team == 1 { SHOT_PLAYER } else { SHOT_ENEMY }, false);
                }
            }
            Body::Rock { x, y, .. } => outline(fb, w, h, x + 16 - vx, y + 16 - vy, x + 48 - vx, y + 48 - vy, ROCK, true),
            _ => {}
        }
    }
}

/// A small controller per player at the bottom-left of the view: lit buttons are held this frame.
pub fn input_display(text: &Text, inputs: &[u16], fb: &mut [u32], w: usize, h: usize) {
    for (p, &v) in inputs.iter().enumerate() {
        let ox = 6 + p as i32 * 92;
        let oy = h as i32 - 34;
        dim_rect(fb, w, h, ox - 3, oy - 3, 88, 32);
        let key = |fb: &mut [u32], x: i32, y: i32, bw: i32, bit: u16, label: &str| {
            let on = v & bit != 0;
            tiny::fill_rect(fb, w, h, ox + x, oy + y, bw, 9, if on { ACCENT } else { 0xFF3A_3A44 });
            if !label.is_empty() {
                tiny::draw_text(fb, w, h, ox + x + (bw - tiny::text_width(label)) / 2 + 1, oy + y + 2, label, if on { 0xFF10_1010 } else { 0xFFB8_B8B8 });
            }
        };
        key(fb, 9, 0, 8, b::UP, "");
        key(fb, 0, 9, 8, b::LEFT, "");
        key(fb, 18, 9, 8, b::RIGHT, "");
        key(fb, 9, 18, 8, b::DOWN, "");
        key(fb, 30, 0, 10, b::L, "L");
        key(fb, 72, 0, 10, b::R, "R");
        key(fb, 58, 13, 10, b::B, "B");
        key(fb, 71, 13, 10, b::A, "A");
        key(fb, 30, 18, 12, b::SELECT, "SE");
        key(fb, 44, 18, 12, b::START, "ST");
        if inputs.len() > 1 {
            text.draw(fb, w, h, ox + 30, oy + 6, &format!("P{}", p + 1), WHITE);
        }
    }
}
