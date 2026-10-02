//! The game's own HUD and dialogue box from `ui/` content (`engine/docs/ui.md`, crate `wf-ui`): the 80x16 HUD
//! picture with its HP / EP / SS / SS3 / experience gauge steps (draw routine `0x0800B99C`) and the 160x64
//! dialogue surface (box picture + `dialog` font, line fitter `0x0803DE6A`, layout `0x0800AEF6`).

use std::collections::HashMap;

use serde_json::Value;
use wf_content::image::{decode_indexed, Indexed};
use wf_content::{Error, Packs, Result};
use wf_view::{DialogPos, DialogView, HudView};

/// HUD OBJs at (8, 8), priority 1 (`hud.json`).
pub const HUD_POS: (i32, i32) = (8, 8);
pub const HUD_PRIORITY: u8 = 1;
/// Dialogue OBJs: x 40 (3 OBJs 64 + 64 + 32 wide), priority 0. y = 8 for top boxes (emulator OAM, intro text);
/// middle / bottom are placed symmetrically (48 / 88), see docs/frontend.md.
pub const DIALOG_X: i32 = 40;
pub const DIALOG_Y: [i32; 3] = [8, 48, 88];
pub const DIALOG_PRIORITY: u8 = 0;

pub struct Glyph {
    pub w: u32,
    pub advance: u32,
    /// set pixels (x, y) relative to the glyph cell top-left
    pub px: Vec<(u8, u8)>,
}

pub struct Font {
    pub ascent: i32,
    pub glyphs: Vec<Option<Glyph>>,
}

impl Font {
    pub fn load(packs: &Packs, id: &str) -> Result<Font> {
        let j: Value = packs.json(&format!("ui/fonts/{id}.json"))?;
        let atlas = decode_indexed(&packs.read(&format!("ui/fonts/{}", j["atlas"].as_str().unwrap_or("x"))).ok_or_else(|| Error(format!("font {id}: no atlas")))?)?;
        let mut glyphs: Vec<Option<Glyph>> = (0..256).map(|_| None).collect();
        if let Some(m) = j["glyphs"].as_object() {
            for (k, g) in m {
                let Ok(c) = k.parse::<usize>() else { continue };
                if c >= 256 {
                    continue;
                }
                let (x, y, w) = (g["x"].as_u64().unwrap_or(0) as u32, g["y"].as_u64().unwrap_or(0) as u32, g["w"].as_u64().unwrap_or(0) as u32);
                let mut px = Vec::new();
                for dy in 0..16 {
                    for dx in 0..16 {
                        let (ax, ay) = (x + dx, y + dy);
                        if ax < atlas.w && ay < atlas.h && atlas.idx[(ay * atlas.w + ax) as usize] != 0 {
                            px.push((dx as u8, dy as u8));
                        }
                    }
                }
                glyphs[c] = Some(Glyph { w, advance: g["advance"].as_u64().unwrap_or(w as u64 + 1) as u32, px });
            }
        }
        Ok(Font { ascent: j["ascent_px"].as_i64().unwrap_or(12) as i32, glyphs })
    }

    /// width + 1 (a code unit without a glyph has width 0)
    pub fn advance(&self, c: u16) -> u32 {
        match self.glyphs.get(c as usize) {
            Some(Some(g)) => g.advance,
            _ => 1,
        }
    }

    /// Width of displayable text: `0x0008, n` colour pairs take no space.
    pub fn measure(&self, s: &[u16]) -> u32 {
        let mut w = 0;
        let mut i = 0;
        while i < s.len() {
            if s[i] == 8 {
                i += 2;
                continue;
            }
            w += self.advance(s[i]);
            i += 1;
        }
        w
    }

    /// Draw a line directly into an ARGB framebuffer (frontend menus); returns the pen x after the line.
    pub fn draw_argb(&self, fb: &mut [u32], w: usize, h: usize, s: &str, x: i32, y: i32, color: u32) -> i32 {
        let mut pen = x;
        let top = y - self.ascent;
        for c in units(s) {
            if let Some(Some(g)) = self.glyphs.get(c as usize) {
                for &(gx, gy) in &g.px {
                    let (px, py) = (pen + gx as i32, top + gy as i32);
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        fb[py as usize * w + px as usize] = color;
                    }
                }
                pen += g.w as i32 + 1;
            } else {
                pen += 1;
            }
        }
        pen
    }

    /// Width of a string in pixels.
    pub fn width(&self, s: &str) -> i32 {
        self.measure(&units(s)) as i32
    }

    /// Draw a line with the pen at (x, y) (glyph tops at y - ascent) into an index surface.
    pub fn draw_line(&self, dst: &mut [u8], dw: usize, dh: usize, text: &[u16], x: i32, y: i32, color: &mut u8) {
        let mut pen = x;
        let top = y - self.ascent;
        let mut i = 0;
        while i < text.len() {
            let c = text[i];
            if c == 8 {
                if let Some(&n) = text.get(i + 1) {
                    *color = n as u8;
                }
                i += 2;
                continue;
            }
            if let Some(Some(g)) = self.glyphs.get(c as usize) {
                for &(gx, gy) in &g.px {
                    let (px, py) = (pen + gx as i32, top + gy as i32);
                    if px >= 0 && py >= 0 && (px as usize) < dw && (py as usize) < dh {
                        dst[py as usize * dw + px as usize] = *color;
                    }
                }
                pen += g.w as i32 + 1;
            } else {
                pen += 1;
            }
            i += 1;
        }
    }
}

/// The game's line fitter `0x0803DE6A` (same rules as `wf_ui::font::wrap`): leading spaces / line breaks are
/// skipped, a line ends at `\n`, `0x0B` or the end, and breaks at the last space whose prefix fits `max_w` (a
/// longer word is cut where it stops fitting).
pub fn wrap(font: &Font, text: &[u16], max_w: u32) -> Vec<(Vec<u16>, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < text.len() && matches!(text[i], 0x20 | 0x0A | 0x0B) {
            i += 1;
        }
        if i >= text.len() {
            break;
        }
        let start = i;
        let mut w = 0u32;
        let mut last_space: Option<usize> = None;
        let mut end = text.len();
        let mut next = text.len();
        let mut j = i;
        while j < text.len() {
            let c = text[j];
            if c == 0x0A || c == 0x0B {
                end = j;
                next = j + 1;
                break;
            }
            if c == 8 {
                j += 2;
                continue;
            }
            if c == 0x20 {
                last_space = Some(j);
            }
            let nw = w + font.advance(c);
            if nw > max_w {
                match last_space {
                    Some(sp) => {
                        end = sp;
                        next = sp + 1;
                    }
                    None => {
                        end = j;
                        next = j;
                    }
                }
                break;
            }
            w = nw;
            j += 1;
        }
        if j >= text.len() && end == text.len() {
            next = text.len();
        }
        let line = text[start..end].to_vec();
        let lw = font.measure(&line);
        out.push((line, lw));
        i = next;
    }
    out
}

/// Text as UTF-16 code units the fonts index (Latin-1, Œ/œ at 0x8C/0x9C like Windows-1252).
pub fn units(s: &str) -> Vec<u16> {
    s.chars()
        .map(|c| match c {
            'Œ' => 0x8C,
            'œ' => 0x9C,
            c if (c as u32) < 0x100 => c as u16,
            _ => '?' as u16,
        })
        .collect()
}

pub struct DialogStyle {
    pub max_lines: u32,
    pub wrap: u32,
    pub x: i32,
    pub y: i32,
    pub image: Indexed,
}

pub struct Ui {
    frame: Indexed,
    ss: Indexed,
    ss3: Indexed,
    hp: Vec<Vec<(u32, u32)>>,
    ep: Vec<Vec<(u32, u32)>>,
    xp: Vec<Vec<(u32, u32)>>,
    empty: u8,
    icons: HashMap<String, Option<Indexed>>,
    pub dialog_font: Font,
    pub styles: Vec<DialogStyle>,
    text_color: u8,
    line_h: i32,
}

fn steps(v: &Value) -> Vec<Vec<(u32, u32)>> {
    v["pixels"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| s.as_array().into_iter().flatten().map(|p| (p[0].as_u64().unwrap_or(0) as u32, p[1].as_u64().unwrap_or(0) as u32)).collect())
                .collect()
        })
        .unwrap_or_default()
}

fn png(packs: &Packs, path: &str) -> Result<Indexed> {
    decode_indexed(&packs.read(path).ok_or_else(|| Error(format!("missing {path}")))?)
}

impl Ui {
    pub fn load(packs: &Packs) -> Result<Ui> {
        let h: Value = packs.json("ui/hud.json")?;
        let d: Value = packs.json("ui/dialog.json")?;
        let font_id = d["font"].as_str().unwrap_or("fonts/dialog.json").trim_start_matches("fonts/").trim_end_matches(".json").to_string();
        let mut styles = Vec::new();
        for s in d["styles"].as_array().into_iter().flatten() {
            styles.push(DialogStyle {
                max_lines: s["max_lines"].as_u64().unwrap_or(4) as u32,
                wrap: s["wrap_width_px"].as_u64().unwrap_or(144) as u32,
                x: s["text_x"].as_i64().unwrap_or(8) as i32,
                y: s["text_y"].as_i64().unwrap_or(15) as i32,
                image: png(packs, &format!("ui/{}", s["image"].as_str().unwrap_or("")))?,
            });
        }
        Ok(Ui {
            frame: png(packs, "ui/hud/frame.png")?,
            ss: png(packs, "ui/hud/gauge_ss.png")?,
            ss3: png(packs, "ui/hud/gauge_ss3.png")?,
            hp: steps(&h["steps"]["hp"]),
            ep: steps(&h["steps"]["ep"]),
            xp: steps(&h["steps"]["xp"]),
            empty: h["empty_colour"].as_u64().unwrap_or(1) as u8,
            icons: HashMap::new(),
            dialog_font: Font::load(packs, &font_id)?,
            styles,
            text_color: d["text_color"].as_u64().unwrap_or(2) as u8,
            line_h: d["line_height_px"].as_i64().unwrap_or(15) as i32,
        })
    }

    fn icon(&mut self, packs: &Packs, skill: u8, alt: bool) -> Option<&Indexed> {
        let key = format!("icons/skills/skill_{skill:02}{}.png", if alt { "_alt" } else { "" });
        if !self.icons.contains_key(&key) {
            let v = packs.read(&key).and_then(|b| decode_indexed(&b).ok());
            self.icons.insert(key.clone(), v);
        }
        self.icons.get(&key).and_then(|x| x.as_ref())
    }

    /// The 80x16 HUD picture as ARGB (alpha 0 = transparent), exactly as `0x0800B99C` composes its OBJ tiles: the
    /// skill icon and the frame are copied first, then the gauge steps overwrite pixels (the experience line
    /// runs through the icon OBJ too).
    pub fn hud(&mut self, packs: &Packs, h: &HudView) -> Vec<u32> {
        let (w, hh) = (self.frame.w as usize, self.frame.h as usize);
        let argb = |pal: &[u32], c: u8| if c == 0 { 0 } else { pal.get(c as usize).copied().unwrap_or(0) | 0xFF00_0000 };
        let mut out: Vec<u32> = self.frame.idx.iter().map(|&c| argb(&self.frame.palette, c)).collect();
        if let Some(s) = h.skill {
            if let Some(ic) = self.icon(packs, s, h.skill_alt).cloned() {
                for y in 0..(ic.h as usize).min(hh) {
                    for x in 0..(ic.w as usize).min(32) {
                        out[y * w + x] = argb(&ic.palette, ic.idx[y * ic.w as usize + x]);
                    }
                }
            }
        }
        let d = |a: i32, b: i32| if b == 0 { 0 } else { (a as i64 * 41 / b as i64).max(0) as usize };
        let hp41 = d(h.hp, h.hp_max);
        let mut ep41 = d(h.ep, h.ep_max);
        let mut ss41 = d(h.ss, h.ep_max);
        let ss3_41 = d(h.ss3, h.ep_max);
        ss41 = ss41.max(ss3_41);
        ep41 = ep41.max(ss41);
        let empty = argb(&self.frame.palette, self.empty);
        let set = |out: &mut Vec<u32>, px: &[(u32, u32)], f: &dyn Fn(usize) -> u32| {
            for &(x, y) in px {
                let o = y as usize * w + x as usize;
                if o < out.len() {
                    out[o] = f(o);
                }
            }
        };
        for i in hp41.min(41)..self.hp.len() {
            set(&mut out, &self.hp[i], &|_| empty);
        }
        let mut i = 0;
        while i < ss3_41.min(self.ep.len()) {
            set(&mut out, &self.ep[i], &|o| argb(&self.ss3.palette, self.ss3.idx[o]));
            i += 1;
        }
        while i < ss41.min(self.ep.len()) {
            set(&mut out, &self.ep[i], &|o| argb(&self.ss.palette, self.ss.idx[o]));
            i += 1;
        }
        for i in i.max(ep41)..self.ep.len() {
            set(&mut out, &self.ep[i], &|_| empty);
        }
        for i in (h.xp72 as usize).min(72)..self.xp.len() {
            set(&mut out, &self.xp[i], &|_| empty);
        }
        out
    }

    /// The 160x64 dialogue surface (ARGB) for page `dialog.page`; `None` past the last page.
    pub fn dialog(&self, dv: &DialogView) -> Option<Vec<u32>> {
        let st = self.styles.get(dv.style as usize).or(self.styles.first())?;
        let text = units(&dv.text);
        let mut pages = Vec::new();
        for chunk in text.split(|&c| c == 0x0B) {
            let lines = wrap(&self.dialog_font, chunk, st.wrap);
            for p in lines.chunks(st.max_lines.max(1) as usize) {
                pages.push(p.to_vec());
            }
        }
        let page = pages.get(dv.page as usize)?;
        let (w, h) = (st.image.w as usize, st.image.h as usize);
        let mut idx = st.image.idx.clone();
        let n = page.len() as i32;
        let mut y = st.y + 7 * (st.max_lines as i32 - n);
        let mut color = self.text_color;
        for (line, lw) in page {
            let x = st.x + (st.wrap as i32 - *lw as i32) / 2;
            self.dialog_font.draw_line(&mut idx, w, h, line, x, y, &mut color);
            y += self.line_h;
        }
        let pal = &st.image.palette;
        Some(idx.iter().map(|&c| if c == 0 { 0 } else { pal.get(c as usize).copied().unwrap_or(0) | 0xFF00_0000 }).collect())
    }

    pub fn dialog_y(pos: DialogPos) -> i32 {
        DIALOG_Y[match pos {
            DialogPos::Top => 0,
            DialogPos::Middle => 1,
            DialogPos::Bottom => 2,
        }]
    }
}
