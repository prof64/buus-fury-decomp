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
/// The box object's centre x (`0x08072890` +8 = 120): the box (160) or box + portrait span (224) is centred on it.
pub const DIALOG_CX: i32 = 120;
/// Portrait variant (`0x0800B188`, VERIFIED emulator OAM): a 64x64 8bpp OBJ (index 0 transparent) beside the box, the
/// four OBJs spanning 224 px (x 8..232): portrait right (box OBJs at cx-112, -48, +16, portrait +48) or left
/// (portrait cx-112, box -48, +16, +80). Text is not centred: lines start at the style's (x, y) (top aligned), a
/// `- ` option line at x + 20 with wrap - 20 (the cursor arrow's room); a leading `^` restores the centred layout.
pub const PORTRAIT_W: usize = 64;
pub const OPTION_INDENT: u32 = 0x14;

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
        let (line, lw, next) = fit_line(font, text, i, max_w);
        out.push((line, lw));
        i = next;
    }
    out
}

/// The left-aligned (portrait) layout's line breaking (`0x0800AE00` + `0x0803DE6A`): like [`wrap`], but a line
/// starting with `-` is an option: the `-` and the spaces after it are skipped and the line fits `max_w - indent`.
/// Returns (line, width, option).
pub fn wrap_options(font: &Font, text: &[u16], max_w: u32, indent: u32) -> Vec<(Vec<u16>, u32, bool)> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < text.len() && matches!(text[i], 0x20 | 0x0A | 0x0B) {
            i += 1;
        }
        if i >= text.len() {
            break;
        }
        let opt = text[i] == '-' as u16;
        if opt {
            i += 1;
            while i < text.len() && text[i] == 0x20 {
                i += 1;
            }
        }
        let (line, lw, next) = fit_line(font, text, i, if opt { max_w.saturating_sub(indent) } else { max_w });
        out.push((line, lw, opt));
        i = next;
    }
    out
}

/// one line of the line fitter from `i`: (line, its width, where the next line starts)
fn fit_line(font: &Font, text: &[u16], i: usize, max_w: u32) -> (Vec<u16>, u32, usize) {
    {
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
        (line, lw, next)
    }
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
    /// the stat-points mark's four 16x16 frames side by side (`ui/hud/stat_mark.png`; older content has none)
    stat_mark: Option<Indexed>,
    /// the boss HP bar's 54 fill levels, 64x8 each, stacked (`ui/hud/boss_bar.png`)
    boss_bar: Option<Indexed>,
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
    portraits: HashMap<u16, Option<Vec<u32>>>,
    portrait_paths: Option<HashMap<u16, String>>,
    /// screen positions of the HUD / banner and the skill-icon set (`ui/layout.json`, mods; defaults = the game's)
    pub layout: Layout,
}

/// Where the frontend puts the HUD pieces (`ui/layout.json`, a mod file; the base pack has none and every default is
/// the game's placement). Positions are in the 240x160 GBA screen; in the zoomed-out PC view they anchor to the
/// view's corner given by `anchor` (`top_left` like the game, `top_right`, `bottom_left`, `bottom_right`).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub hud: (i32, i32),
    pub hud_anchor: Anchor,
    pub hud_priority: u8,
    /// never draw the HUD
    pub hud_hidden: bool,
    /// the selected skill's icon inside the 80x16 HUD picture
    pub icon_pos: (i32, i32),
    /// the icon set: `<icon_dir>/skill_NN.png` (+ `_alt`), and per-skill files (`icons: {"25": "path.png"}`; the
    /// `_alt` variant is the same name with `_alt` before `.png`)
    pub icon_dir: String,
    pub icon_files: std::collections::BTreeMap<u8, String>,
    /// the area banner's y (bottom-anchored like the game: 128 = 32 px above the bottom edge)
    pub banner_y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for Layout {
    fn default() -> Self {
        Layout { hud: HUD_POS, hud_anchor: Anchor::TopLeft, hud_priority: HUD_PRIORITY, hud_hidden: false, icon_pos: (0, 0), icon_dir: "icons/skills".into(), icon_files: Default::default(), banner_y: crate::banner::Y }
    }
}

impl Layout {
    /// `ui/layout.json` if the packs have one (every field optional), else the game's layout
    pub fn load(packs: &Packs) -> Layout {
        let mut l = Layout::default();
        let Ok(v) = packs.json::<Value>("ui/layout.json") else { return l };
        let xy = |v: &Value, d: (i32, i32)| match v.as_array() {
            Some(a) if a.len() >= 2 => (a[0].as_i64().unwrap_or(d.0 as i64) as i32, a[1].as_i64().unwrap_or(d.1 as i64) as i32),
            _ => d,
        };
        let h = &v["hud"];
        l.hud = xy(&h["pos"], l.hud);
        l.hud_anchor = match h["anchor"].as_str() {
            Some("top_right") => Anchor::TopRight,
            Some("bottom_left") => Anchor::BottomLeft,
            Some("bottom_right") => Anchor::BottomRight,
            _ => Anchor::TopLeft,
        };
        l.hud_priority = h["priority"].as_u64().map(|p| p.min(3) as u8).unwrap_or(l.hud_priority);
        l.hud_hidden = h["hidden"].as_bool().unwrap_or(false);
        let ic = &v["skill_icons"];
        l.icon_pos = xy(&ic["pos"], l.icon_pos);
        if let Some(d) = ic["dir"].as_str() {
            l.icon_dir = d.trim_end_matches('/').to_string();
        }
        for (k, f) in ic["files"].as_object().into_iter().flatten() {
            if let (Ok(id), Some(f)) = (k.parse::<u8>(), f.as_str()) {
                l.icon_files.insert(id, f.to_string());
            }
        }
        l.banner_y = v["banner"]["y"].as_i64().map(|y| y as i32).unwrap_or(l.banner_y);
        l
    }
    /// the HUD's top-left in a `w` x `h` view
    pub fn hud_at(&self, w: i32, h: i32) -> (i32, i32) {
        let (x, y) = self.hud;
        match self.hud_anchor {
            Anchor::TopLeft => (x, y),
            Anchor::TopRight => (w - (GBA_W_UI - x), y),
            Anchor::BottomLeft => (x, h - (GBA_H_UI - y)),
            Anchor::BottomRight => (w - (GBA_W_UI - x), h - (GBA_H_UI - y)),
        }
    }
    fn icon_path(&self, skill: u8, alt: bool) -> String {
        let sfx = if alt { "_alt" } else { "" };
        match self.icon_files.get(&skill) {
            Some(f) if alt => match f.strip_suffix(".png") {
                Some(stem) => format!("{stem}_alt.png"),
                None => f.clone(),
            },
            Some(f) => f.clone(),
            None => format!("{}/skill_{skill:02}{sfx}.png", self.icon_dir),
        }
    }
}

const GBA_W_UI: i32 = 240;
const GBA_H_UI: i32 = 160;

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
            stat_mark: png(packs, "ui/hud/stat_mark.png").ok(),
            boss_bar: png(packs, "ui/hud/boss_bar.png").ok(),
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
            line_h: d["line_height_px"].as_i64().unwrap_or(14) as i32,
            portraits: HashMap::new(),
            portrait_paths: None,
            layout: Layout::load(packs),
        })
    }

    fn icon(&mut self, packs: &Packs, skill: u8, alt: bool) -> Option<&Indexed> {
        let key = self.layout.icon_path(skill, alt);
        if !self.icons.contains_key(&key) {
            // a layout's icon set falls back to the game's icon of a skill it does not draw
            let default = format!("icons/skills/skill_{skill:02}{}.png", if alt { "_alt" } else { "" });
            let v = packs.read(&key).or_else(|| packs.read(&default)).and_then(|b| decode_indexed(&b).ok());
            self.icons.insert(key.clone(), v);
        }
        self.icons.get(&key).and_then(|x| x.as_ref())
    }

    /// The boss HP bar at fill `k` (0..=53) as 64x8 ARGB (alpha 0 = transparent)
    pub fn boss_bar(&self, k: u8) -> Option<Vec<u32>> {
        let im = self.boss_bar.as_ref()?;
        let mut out = vec![0u32; 64 * 8];
        for y in 0..8usize {
            for x in 0..64usize {
                let c = *im.idx.get((k as usize * 8 + y) * im.w as usize + x)?;
                if c != 0 {
                    out[y * 64 + x] = (im.palette.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (c as u32) << 24;
                }
            }
        }
        Some(out)
    }

    /// The stat-points mark's frame `k` as 16x16 ARGB (alpha 0 = transparent; OBJ palette order like the HUD)
    pub fn stat_mark(&self, k: u8) -> Option<Vec<u32>> {
        let im = self.stat_mark.as_ref()?;
        let mut out = vec![0u32; 16 * 16];
        for y in 0..16usize {
            for x in 0..16usize {
                let c = *im.idx.get(y * im.w as usize + k as usize * 16 + x)?;
                if c != 0 {
                    out[y * 16 + x] = (im.palette.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (c as u32) << 24;
                }
            }
        }
        Some(out)
    }

    /// The 80x16 HUD picture as ARGB (alpha 0 = transparent), exactly as `0x0800B99C` composes its OBJ tiles: the
    /// skill icon and the frame are copied first, then the gauge steps overwrite pixels (the experience line
    /// runs through the icon OBJ too).
    pub fn hud(&mut self, packs: &Packs, h: &HudView) -> Vec<u32> {
        let (w, hh) = (self.frame.w as usize, self.frame.h as usize);
        // top byte = OBJ palette index (every HUD picture / icon is indexed in OBJ palette order); 0 = transparent
        let argb = |pal: &[u32], c: u8| if c == 0 { 0 } else { (pal.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (c as u32) << 24 };
        let mut out: Vec<u32> = self.frame.idx.iter().map(|&c| argb(&self.frame.palette, c)).collect();
        if let Some(s) = h.skill {
            if let Some(ic) = self.icon(packs, s, h.skill_alt).cloned() {
                // (a mod layout may move the icon inside the picture; the game's is at (0, 0))
                let (ix, iy) = self.layout.icon_pos;
                for y in 0..(ic.h as usize).min(hh) {
                    for x in 0..(ic.w as usize).min(32) {
                        let (ox, oy) = (x as i32 + ix, y as i32 + iy);
                        if ox >= 0 && oy >= 0 && (ox as usize) < w && (oy as usize) < hh {
                            out[oy as usize * w + ox as usize] = argb(&ic.palette, ic.idx[y * ic.w as usize + x]);
                        }
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
        self.dialog_typed(dv, None)
    }

    /// Like [`Ui::dialog`], drawing only the first `typed` visible characters of the page (typewriter).
    pub fn dialog_typed(&self, dv: &DialogView, typed: Option<u32>) -> Option<Vec<u32>> {
        let st = self.styles.get(dv.style as usize).or(self.styles.first())?;
        let mut text = units(&dv.text);
        // `^` first: consumed, the text stays centred even beside a portrait (0x0800B16A)
        let caret = text.first() == Some(&('^' as u16));
        if caret {
            text.remove(0);
        }
        let centred = dv.portrait.is_none() || caret;
        // (line, width, option): the centred layout keeps an option's "- " in the line (drawn), the portrait
        // layout skips it and indents the line (0x0800AFDE / 0x0800B02E)
        let mut pages: Vec<Vec<(Vec<u16>, u32, bool)>> = Vec::new();
        for chunk in text.split(|&c| c == 0x0B) {
            let lines: Vec<(Vec<u16>, u32, bool)> = if centred {
                wrap(&self.dialog_font, chunk, st.wrap).into_iter().map(|(l, w)| (l, w, false)).collect()
            } else {
                wrap_options(&self.dialog_font, chunk, st.wrap, OPTION_INDENT)
            };
            for p in lines.chunks(st.max_lines.max(1) as usize) {
                pages.push(p.to_vec());
            }
        }
        let page = pages.get(dv.page as usize)?;
        let (w, h) = (st.image.w as usize, st.image.h as usize);
        let mut idx = st.image.idx.clone();
        let n = page.len() as i32;
        let mut y = if centred { st.y + 7 * (st.max_lines as i32 - n) } else { st.y };
        let mut color = self.text_color;
        let mut left = typed.unwrap_or(u32::MAX);
        for (line, lw, opt) in page {
            // lines keep their full-width centring while they type in
            let x = if centred {
                st.x + (st.wrap as i32 - *lw as i32) / 2
            } else {
                st.x + if *opt { OPTION_INDENT as i32 } else { 0 }
            };
            let mut shown: Vec<u16> = Vec::with_capacity(line.len());
            let mut k = 0;
            if line.len() >= 2 && line[0] == '-' as u16 && line[1] == ' ' as u16 {
                shown.extend_from_slice(&line[..2]);
                k = 2;
            }
            while k < line.len() {
                if line[k] == 8 {
                    shown.extend_from_slice(&line[k..(k + 2).min(line.len())]);
                    k += 2;
                    continue;
                }
                if left == 0 {
                    break;
                }
                left -= 1;
                shown.push(line[k]);
                k += 1;
            }
            self.dialog_font.draw_line(&mut idx, w, h, &shown, x, y, &mut color);
            y += self.line_h;
        }
        let pal = &st.image.palette;
        Some(idx.iter().map(|&c| if c == 0 { 0 } else { (pal.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (c as u32) << 24 }).collect())
    }

    /// Portrait `index` (`portraits/index.json` portrait_index) as 64x64 ARGB (top byte 0 = transparent): the
    /// file named by index.json's `png`, else any `portraits/portrait_NNN*.png` (mods add portraits by listing).
    pub fn portrait(&mut self, packs: &Packs, index: u16) -> Option<&Vec<u32>> {
        if self.portrait_paths.is_none() {
            let mut m: HashMap<u16, String> = HashMap::new();
            for f in packs.list() {
                let Some(name) = f.strip_prefix("portraits/").and_then(|n| n.strip_suffix(".png")) else { continue };
                let Some(num) = name.strip_prefix("portrait_") else { continue };
                let digits: String = num.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(i) = digits.parse::<u16>() {
                    m.entry(i).or_insert(f.clone());
                }
            }
            if let Ok(Value::Array(a)) = packs.value("portraits/index.json") {
                for e in a {
                    if let (Some(i), Some(png)) = (e["portrait_index"].as_u64(), e["png"].as_str().or(e["id"].as_str())) {
                        let path = format!("portraits/{}", if png.ends_with(".png") { png.to_string() } else { format!("{png}.png") });
                        if packs.read(&path).is_some() {
                            m.insert(i as u16, path);
                        }
                    }
                }
            }
            self.portrait_paths = Some(m);
        }
        if !self.portraits.contains_key(&index) {
            let px = self.portrait_paths.as_ref().and_then(|m| m.get(&index)).and_then(|p| packs.read(p)).and_then(|b| wf_content::image::decode_rgba(&b).ok()).map(|im| {
                let mut out = vec![0u32; PORTRAIT_W * PORTRAIT_W];
                for y in 0..(im.h as usize).min(PORTRAIT_W) {
                    for x in 0..(im.w as usize).min(PORTRAIT_W) {
                        let c = im.px[y * im.w as usize + x];
                        if c >> 24 >= 0x80 {
                            out[y * PORTRAIT_W + x] = c | 0xFF00_0000;
                        }
                    }
                }
                out
            });
            self.portraits.insert(index, px);
        }
        self.portraits.get(&index).and_then(|p| p.as_ref())
    }

    /// The whole dialogue span: (width, ARGB 64 rows, OBJ pieces (x, width) in OAM order). Without a portrait the
    /// 160-px box (3 OBJs); with one the 224-px box + portrait (`PORTRAIT_W`) span. Centred on [`DIALOG_CX`].
    pub fn dialog_span(&mut self, packs: &Packs, dv: &DialogView, typed: Option<u32>) -> Option<(usize, Vec<u32>, Vec<(i32, i32)>)> {
        let bx = self.dialog_typed(dv, typed)?;
        let Some(pi) = dv.portrait else { return Some((160, bx, vec![(0, 64), (64, 64), (128, 32)])) };
        let pic = self.portrait(packs, pi).cloned().unwrap_or_else(|| vec![0; PORTRAIT_W * PORTRAIT_W]);
        let sw = 160 + PORTRAIT_W;
        let (box_x, pic_x) = if dv.portrait_left { (PORTRAIT_W, 0) } else { (0, 160) };
        let mut out = vec![0u32; sw * 64];
        for y in 0..64 {
            out[y * sw + box_x..y * sw + box_x + 160].copy_from_slice(&bx[y * 160..y * 160 + 160]);
            out[y * sw + pic_x..y * sw + pic_x + PORTRAIT_W].copy_from_slice(&pic[y * PORTRAIT_W..(y + 1) * PORTRAIT_W]);
        }
        let b = box_x as i32;
        Some((sw, out, vec![(b, 64), (b + 64, 64), (b + 128, 32), (pic_x as i32, PORTRAIT_W as i32)]))
    }

    /// The box zooming about its centre: the three OBJs (64 + 64 + 32 wide) at scale `n`/8, each an affine OBJ with
    /// PA = PD = 2048 / n around its own centre, the OBJ centres pulled toward the box centre (emulator OAM).
    pub fn scale_box(full: &[u32], n: u32) -> Vec<u32> {
        Self::scale_span(full, 160, &[(0, 64), (64, 64), (128, 32)], n)
    }

    /// [`Ui::scale_box`] for any span (`dialog_span`): every OBJ piece (x, width; OAM order, the first on top)
    /// zooms about its own centre, its centre pulled toward the span's centre (emulator OAM, portrait boxes too).
    pub fn scale_span(full: &[u32], bw: usize, pieces: &[(i32, i32)], n: u32) -> Vec<u32> {
        let n = n.clamp(1, 8) as i32;
        if n == 8 {
            return full.to_vec();
        }
        let (bw, bh) = (bw as i32, 64i32);
        let pa = 2048 / n;
        let mut out = vec![0u32; (bw * bh) as usize];
        // OAM order: OBJ 0 on top where the zoomed rectangles overlap -> draw it last
        for &(x0, w) in pieces.iter().rev() {
            let cx = x0 + w / 2;
            let sx0 = bw / 2 + (cx - bw / 2) * n / 8 - w / 2;
            for j in 0..bh {
                // GBA affine OBJ: texel = (PA * (pixel - half)) >> 8 + half (floor)
                let ty = ((pa * (j - bh / 2)) >> 8) + bh / 2;
                if !(0..bh).contains(&ty) {
                    continue;
                }
                for i in 0..w {
                    let tx = ((pa * (i - w / 2)) >> 8) + w / 2;
                    if !(0..w).contains(&tx) {
                        continue;
                    }
                    let (ox, oy) = (sx0 + i, j);
                    if (0..bw).contains(&ox) {
                        let c = full[(ty * bw + x0 + tx) as usize];
                        if c >> 24 != 0 {
                            out[(oy * bw + ox) as usize] = c;
                        }
                    }
                }
            }
        }
        out
    }

    pub fn dialog_y(pos: DialogPos) -> i32 {
        DIALOG_Y[match pos {
            DialogPos::Top => 0,
            DialogPos::Middle => 1,
            DialogPos::Bottom => 2,
        }]
    }
}
