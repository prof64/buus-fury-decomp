//! Menu screens (`RenderView.menu`, `wf_view::MenuView`): the game's software drawing reproduced on 8bpp
//! surfaces — background rows `0x08004F1C`, the panel `0x08004F50`, the glyph routines `0x0803F444` (plain) /
//! `0x0803F5A4` (with the (+1, +1) shadow), picture blits `0x0803F922` (index 0 skipped) — then composed like the
//! PPU (BG layers by priority, OBJs) through the UI palette (`ui/palette.json`).

use std::collections::HashMap;

use wf_content::image::{decode_indexed, Indexed};
use wf_content::Packs;
use wf_view::{MenuFont, MenuLayer, MenuOp, MenuView, PaletteFade};

/// glyph columns of one font: per code unit < 0xFF, the width and the column bit masks (bit k = row k)
#[derive(Clone, Default)]
pub struct ColFont {
    pub glyphs: Vec<Option<(u32, Vec<u32>)>>,
}

impl ColFont {
    pub fn load(packs: &Packs, id: &str) -> Option<ColFont> {
        let j: serde_json::Value = packs.json(&format!("ui/fonts/{id}.json")).ok()?;
        let atlas = decode_indexed(&packs.read(&format!("ui/fonts/{}", j["atlas"].as_str().unwrap_or("x")))?).ok()?;
        let mut glyphs: Vec<Option<(u32, Vec<u32>)>> = vec![None; 256];
        for (k, g) in j["glyphs"].as_object()? {
            let Ok(c) = k.parse::<usize>() else { continue };
            if c >= 256 {
                continue;
            }
            let (x, y, w) = (g["x"].as_u64().unwrap_or(0) as u32, g["y"].as_u64().unwrap_or(0) as u32, g["w"].as_u64().unwrap_or(0) as u32);
            let mut cols = vec![0u32; w as usize];
            for (dx, col) in cols.iter_mut().enumerate() {
                for dy in 0..16u32 {
                    let (ax, ay) = (x + dx as u32, y + dy);
                    if ax < atlas.w && ay < atlas.h && atlas.idx[(ay * atlas.w + ax) as usize] != 0 {
                        *col |= 1 << dy;
                    }
                }
            }
            glyphs[c] = Some((w, cols));
        }
        Some(ColFont { glyphs })
    }
}

/// an 8bpp surface (the game's draw context target)
pub struct Surface {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
}

impl Surface {
    pub fn new(w: i32, h: i32) -> Surface {
        Surface { w, h, px: vec![0; (w * h).max(0) as usize] }
    }
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }
    pub fn hline(&mut self, x: i32, y: i32, n: i32, c: u8) {
        for i in 0..n.max(0) {
            self.set(x + i, y, c);
        }
    }
}

/// `0x08004F1C`: rows alternate 0x01 / 0x30
fn background(s: &mut Surface, x: i32, y: i32, w: i32, rows: i32) {
    let mut c = 0x01u8;
    for r in 0..rows {
        s.hline(x, y + r, w, c);
        c ^= 0x31;
    }
}

/// `0x08004F50` (same transcription as `wf_ui::menu::panel`)
fn panel(s: &mut Surface, x0: i32, y0: i32, x1: i32, y1: i32) {
    let w = x1 - x0;
    let h = y1 - y0;
    let left = [0x1D, 0x6E, 0x3F, 0x01, 0x01, 0x30];
    let right = [0x30, 0x01, 0x1D, 0x6E, 0x3F];
    for r in 0..h {
        let y = y0 + r;
        for (i, &c) in left.iter().enumerate() {
            s.set(x0 + i as i32, y, c);
        }
        for (i, &c) in right.iter().enumerate() {
            s.set(x0 + w - 5 + i as i32, y, c);
        }
        if r >= 3 {
            s.set(x0 + w, y, 0x01);
            s.set(x0 + w + 1, y, 0x01);
            s.set(x0 + w + 2, y, 0x01);
        }
    }
    let row = |r: i32| y0 + r;
    s.hline(x0, row(0), w, 0x1D);
    s.hline(x0 + 1, row(1), w - 2, 0x6E);
    s.hline(x0 + 2, row(2), w - 4, 0x3F);
    s.hline(x0 + 3, row(3), w - 6, 0x01);
    s.hline(x0 + 3, row(4), w - 6, 0x01);
    s.hline(x0 + 5, row(5), w - 10, 0x30);
    s.hline(x0 + 5, row(h - 5), w - 10, 0x30);
    s.hline(x0 + 3, row(h - 4), w - 6, 0x01);
    s.hline(x0 + 2, row(h - 3), w - 4, 0x1D);
    s.hline(x0 + 1, row(h - 2), w - 2, 0x6E);
    s.hline(x0, row(h - 1), w, 0x3F);
    s.hline(x0 + 3, row(h), w, 0x01);
    s.hline(x0 + 3, row(h + 1), w, 0x01);
    s.hline(x0 + 3, row(h + 2), w, 0x01);
}

pub struct MenuRenderer {
    fonts: Vec<Option<ColFont>>,
    images: HashMap<String, Option<Indexed>>,
    /// UI palette, 0xRRGGBB
    pub pal: Vec<u32>,
}

fn font_slot(f: MenuFont) -> usize {
    match f {
        MenuFont::Small => 0,
        MenuFont::Dialog => 1,
        MenuFont::Digits => 2,
        MenuFont::Menu => 3,
        MenuFont::Bold => 4,
    }
}

impl MenuRenderer {
    pub fn load(packs: &Packs) -> MenuRenderer {
        let fonts = [MenuFont::Small, MenuFont::Dialog, MenuFont::Digits, MenuFont::Menu, MenuFont::Bold].iter().map(|f| ColFont::load(packs, f.id())).collect();
        let pal: Vec<u32> = packs
            .json::<serde_json::Value>("ui/palette.json")
            .ok()
            .and_then(|v| v["colors"].as_array().map(|a| a.iter().map(|c| u32::from_str_radix(c.as_str().unwrap_or("#000000").trim_start_matches('#'), 16).unwrap_or(0)).collect()))
            .unwrap_or_else(|| vec![0; 256]);
        MenuRenderer { fonts, images: HashMap::new(), pal }
    }

    /// an indexed picture with the game's raw indices: pictures exported with their own compact palette
    /// (portraits, icons) are mapped back by colour (the UI palette's colours are unique except black: index 1;
    /// the transparent entry -> 0)
    fn image(&mut self, packs: &Packs, path: &str) -> Option<&Indexed> {
        if !self.images.contains_key(path) {
            let mut img = packs.read(path).and_then(|b| decode_indexed(&b).ok());
            if let Some(im) = img.as_mut() {
                let raw = im.palette.len() >= 256 && (1..256).all(|i| im.palette[i] & 0xFF_FFFF == self.pal[i]);
                if !raw {
                    let map: Vec<u8> = im
                        .palette
                        .iter()
                        .map(|&c| {
                            if c >> 24 == 0 {
                                return 0;
                            }
                            let rgb = c & 0xFF_FFFF;
                            (1..256).find(|&k| self.pal[k] == rgb).unwrap_or(0) as u8
                        })
                        .collect();
                    for v in im.idx.iter_mut() {
                        *v = map.get(*v as usize).copied().unwrap_or(0);
                    }
                }
            }
            self.images.insert(path.to_string(), img);
        }
        self.images.get(path).and_then(|o| o.as_ref())
    }

    /// draw one text line like `0x0803DADA`'s per-unit loop (colour codes switch the colour; units without a
    /// glyph draw nothing and do not advance)
    fn text(&self, s: &mut Surface, font: MenuFont, x: i32, y: i32, colour: u8, shadow: Option<u8>, clip: [i32; 4], text: &[u16]) {
        let Some(Some(f)) = self.fonts.get(font_slot(font)) else { return };
        let mut colour = colour;
        let mut pen = x;
        let top = y - 12;
        let mut i = 0;
        let cx0 = clip[0].max(0);
        let cy0 = clip[1].max(0);
        let cx1 = clip[2].min(s.w);
        let cy1 = clip[3].min(s.h);
        while i < text.len() {
            let c = text[i];
            if c == 8 {
                if let Some(&n) = text.get(i + 1) {
                    colour = n as u8;
                }
                i += 2;
                continue;
            }
            i += 1;
            let Some(Some((w, cols))) = f.glyphs.get(c as usize) else { continue };
            let w = *w as i32;
            match shadow {
                None => {
                    // 0x0803F444: rect (pen, top)-(pen + w, top + 16)
                    for (dx, &col) in cols.iter().enumerate() {
                        let px = pen + dx as i32;
                        if px < cx0 || px >= cx1 {
                            continue;
                        }
                        for dy in 0..16 {
                            let py = top + dy;
                            if col >> dy & 1 != 0 && py >= cy0 && py < cy1 {
                                s.px[(py * s.w + px) as usize] = colour;
                            }
                        }
                    }
                }
                Some(sc) => {
                    // 0x0803F5A4: rect (pen, top)-(pen + w + 1, top + 17); column k: glyph bits, else the bits of
                    // column k - 1 one row lower take the shadow colour
                    for k in 0..=w {
                        let a = if k < w { cols[k as usize] } else { 0 };
                        let sh = if k > 0 { cols[(k - 1) as usize] << 1 } else { 0 };
                        let px = pen + k;
                        if px < cx0 || px >= cx1 {
                            continue;
                        }
                        for dy in 0..17 {
                            let py = top + dy;
                            if py < cy0 || py >= cy1 {
                                continue;
                            }
                            if a >> dy & 1 != 0 {
                                s.px[(py * s.w + px) as usize] = colour;
                            } else if sh >> dy & 1 != 0 {
                                s.px[(py * s.w + px) as usize] = sc;
                            }
                        }
                    }
                }
            }
            pen += w + 1;
        }
    }

    /// rasterise a layer's display list
    pub fn surface(&mut self, packs: &Packs, layer: &MenuLayer) -> Surface {
        let mut s = Surface::new(layer.w as i32, layer.h as i32);
        for op in &layer.ops {
            match op {
                MenuOp::Background { x, y, w, rows } => background(&mut s, *x, *y, *w, *rows),
                MenuOp::Panel { x0, y0, x1, y1 } => panel(&mut s, *x0, *y0, *x1, *y1),
                MenuOp::Fill { x0, y0, x1, y1, colour } => {
                    for y in *y0..*y1 {
                        s.hline(*x0, y, x1 - x0, *colour);
                    }
                }
                MenuOp::Pixel { x, y, colour } => s.set(*x, *y, *colour),
                MenuOp::Text { font, x, y, colour, shadow, clip, text } => self.text(&mut s, *font, *x, *y, *colour, *shadow, *clip, text),
                MenuOp::Image { path, frame, fw, fh, x, y, key0 } => {
                    let (fw, fh) = (*fw as i32, *fh as i32);
                    let Some(img) = self.image(packs, path) else { continue };
                    let (iw, ih) = (img.w as i32, img.h as i32);
                    // frames side by side, else stacked vertically
                    let per_row = (iw / fw.max(1)).max(1);
                    let fx = (*frame as i32 % per_row) * fw;
                    let fy = (*frame as i32 / per_row) * fh;
                    for yy in 0..fh {
                        for xx in 0..fw {
                            let (sx, sy) = (fx + xx, fy + yy);
                            if sx >= iw || sy >= ih {
                                continue;
                            }
                            let c = img.idx[(sy * iw + sx) as usize];
                            if c == 0 && *key0 {
                                continue;
                            }
                            s.set(x + xx, y + yy, c);
                        }
                    }
                }
            }
        }
        s
    }

    /// the 240x160 screen (0xFFRRGGBB), with the game's palette fade (`0x08007074`) and BLDY fade applied
    pub fn render(&mut self, packs: &Packs, mv: &MenuView, fade: Option<PaletteFade>, fade_black: u8) -> Vec<u32> {
        let mut pal: Vec<u32> = (0..512).map(|i| self.pal.get(i % 256).copied().unwrap_or(0)).collect();
        for &(i, c) in &mv.palette {
            if (i as usize) < pal.len() {
                pal[i as usize] = c & 0xFF_FFFF;
            }
        }
        // layers by priority (3 first), equal priority: later layers on top (BG0 is drawn last on the GBA, the view
        // lists layers back to front)
        let mut idx: Vec<u16> = vec![mv.backdrop; 240 * 160];
        let mut order: Vec<usize> = (0..mv.layers.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(mv.layers[i].priority));
        let surfaces: Vec<Surface> = mv.layers.iter().map(|l| self.surface(packs, l)).collect();
        for &li in &order {
            let l = &mv.layers[li];
            let s = &surfaces[li];
            for y in 0..160 {
                for x in 0..240 {
                    let (sx, sy) = (x - l.x, y - l.y);
                    if sx < 0 || sy < 0 || sx >= s.w || sy >= s.h {
                        continue;
                    }
                    let c = s.px[(sy * s.w + sx) as usize];
                    if c != 0 {
                        idx[(y * 240 + x) as usize] = c as u16;
                    }
                }
            }
        }
        // OBJs: last OAM entry first so index 0 ends on top
        for o in mv.objs.iter().rev() {
            let Some(img) = self.image(packs, &o.path) else { continue };
            let (iw, ih) = (img.w as i32, img.h as i32);
            let (fw, fh) = (o.fw as i32, o.fh as i32);
            let per_row = (iw / fw.max(1)).max(1);
            let fx = (o.frame as i32 % per_row) * fw;
            let fy = (o.frame as i32 / per_row) * fh;
            for yy in 0..fh {
                for xx in 0..fw {
                    let sx = fx + if o.hflip { fw - 1 - xx } else { xx };
                    let sy = fy + if o.vflip { fh - 1 - yy } else { yy };
                    if sx >= iw || sy >= ih {
                        continue;
                    }
                    let c = img.idx[(sy * iw + sx) as usize];
                    let (px, py) = (o.x + xx, o.y + yy);
                    if c == 0 || !(0..240).contains(&px) || !(0..160).contains(&py) {
                        continue;
                    }
                    idx[(py * 240 + px) as usize] = c as u16 + o.pal;
                }
            }
        }
        let fb = fade_black.min(16) as u32;
        idx.iter()
            .map(|&i| {
                let mut c = pal.get(i as usize).copied().unwrap_or(0);
                if let Some(f) = fade.filter(|f| f.level > 0) {
                    c = palette_fade5(c, f.level as u32, f.target);
                }
                if fb > 0 {
                    c = bldy_black(c, fb);
                }
                0xFF00_0000 | c
            })
            .collect()
    }
}

/// `0x08007074` on a BGR555 colour (given expanded): `(c * (256 - level) + t * level) >> 8` per 5-bit channel
fn palette_fade5(c: u32, level: u32, target: u32) -> u32 {
    let l = level.min(256);
    let mut out = 0;
    for sh in [16, 8, 0] {
        let v = ((c >> sh) & 0xFF) >> 3;
        let t = ((target >> sh) & 0xFF) >> 3;
        let r = ((v * (256 - l) + t * l) >> 8).min(31);
        out |= ((r << 3) | (r >> 2)) << sh;
    }
    out
}

/// BLDY brightness decrease on a 5-bit channel basis
fn bldy_black(c: u32, y: u32) -> u32 {
    let mut out = 0;
    for sh in [16, 8, 0] {
        let v = ((c >> sh) & 0xFF) >> 3;
        let r = v - ((v * y) >> 4);
        out |= ((r << 3) | (r >> 2)) << sh;
    }
    out
}
