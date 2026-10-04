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
            // the glyph's own columns when listed: a glyph wider than its 16-px atlas cell (the title font's A, M,
            // O, Q, W) runs into the next cell there
            if let Some(js) = g["cols"].as_array().filter(|a| a.len() == w as usize) {
                for (col, v) in cols.iter_mut().zip(js) {
                    *col = v.as_u64().unwrap_or(0) as u32 & 0xFFFF;
                }
                glyphs[c] = Some((w, cols));
                continue;
            }
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
    sheets: HashMap<String, Option<std::sync::Arc<wf_content::sprite::Sheet>>>,
    /// colour -> the in-game OBJ palette's first index with it (`maps/palette.json`)
    obj_rev: Option<HashMap<u32, u8>>,
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
        MenuFont::Timer => 5,
        MenuFont::Title => 6,
    }
}

impl MenuRenderer {
    pub fn load(packs: &Packs) -> MenuRenderer {
        let fonts = [MenuFont::Small, MenuFont::Dialog, MenuFont::Digits, MenuFont::Menu, MenuFont::Bold, MenuFont::Timer, MenuFont::Title].iter().map(|f| ColFont::load(packs, f.id())).collect();
        let pal: Vec<u32> = packs
            .json::<serde_json::Value>("ui/palette.json")
            .ok()
            .and_then(|v| v["colors"].as_array().map(|a| a.iter().map(|c| u32::from_str_radix(c.as_str().unwrap_or("#000000").trim_start_matches('#'), 16).unwrap_or(0)).collect()))
            .unwrap_or_else(|| vec![0; 256]);
        MenuRenderer { sheets: HashMap::new(), obj_rev: None, fonts, images: HashMap::new(), pal }
    }

    /// an indexed picture with the game's raw indices: pictures exported with their own compact palette
    /// (portraits, icons) are mapped back by colour (the UI palette's colours are unique except black: index 1;
    /// the transparent entry -> 0)
    fn sheet(&mut self, packs: &Packs, key: &str) -> Option<std::sync::Arc<wf_content::sprite::Sheet>> {
        if !self.sheets.contains_key(key) {
            let sh = wf_content::sprite::Sheet::load(packs, key).ok().map(std::sync::Arc::new);
            self.sheets.insert(key.to_string(), sh);
        }
        self.sheets.get(key).cloned().flatten()
    }

    /// an RGBA sprite pixel's OBJ palette index: the sheets are rendered with the UI palette (`0x0805652C`,
    /// wf-extract sprites.rs), whose colours are unique except black (0 / 1: index 0 is transparent -> 1)
    fn obj_index(&mut self, _packs: &Packs, rgb: u32) -> u8 {
        if self.obj_rev.is_none() {
            let mut m = HashMap::new();
            for k in 1..256usize {
                m.entry(self.pal[k]).or_insert(k as u8);
            }
            self.obj_rev = Some(m);
        }
        self.obj_rev.as_ref().and_then(|m| m.get(&rgb).copied()).unwrap_or(1)
    }

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

    /// `0x0803F70C` per glyph: columns x - 1 ..= x + w, rows top .. top + 17; glyph bit -> colour, else a bit of the
    /// 4-neighbourhood (columns k - 1 / k + 1, rows above / below in column k) -> outline colour
    fn text_outline(&self, s: &mut Surface, font: MenuFont, x: i32, y: i32, colour: u8, outline: u8, clip: [i32; 4], text: &[u16]) {
        let Some(Some(f)) = self.fonts.get(font_slot(font)) else { return };
        let mut colour = colour;
        let mut pen = x;
        let top = y - 12;
        let (cx0, cy0, cx1, cy1) = (clip[0].max(0), clip[1].max(0), clip[2].min(s.w), clip[3].min(s.h));
        let mut i = 0;
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
            let col = |k: i32| -> u32 { if k >= 0 && k < w { cols[k as usize] } else { 0 } };
            for k in -1..=w {
                let main = col(k);
                let ol = col(k - 1) | col(k + 1) | (col(k) >> 1) | (col(k) << 1);
                let px = pen + k;
                if px < cx0 || px >= cx1 {
                    continue;
                }
                for dy in 0..17 {
                    let py = top + dy;
                    if py < cy0 || py >= cy1 {
                        continue;
                    }
                    if main >> dy & 1 != 0 {
                        s.px[(py * s.w + px) as usize] = colour;
                    } else if ol >> dy & 1 != 0 {
                        s.px[(py * s.w + px) as usize] = outline;
                    }
                }
            }
            pen += w + 1;
        }
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
                MenuOp::Remap { x0, y0, x1, y1, table } => {
                    for y in (*y0).max(0)..(*y1).min(s.h) {
                        for x in (*x0).max(0)..(*x1).min(s.w) {
                            let k = (y * s.w + x) as usize;
                            s.px[k] = table.get(s.px[k] as usize).copied().unwrap_or(s.px[k]);
                        }
                    }
                }
                MenuOp::Text { font, x, y, colour, shadow, clip, text } => self.text(&mut s, *font, *x, *y, *colour, *shadow, *clip, text),
                MenuOp::Tile { path, tile, x, y, hflip, vflip } => {
                    let Some(img) = self.image(packs, path) else { continue };
                    let per_row = (img.w / 8).max(1) as i32;
                    let (tx, ty) = ((*tile as i32 % per_row) * 8, (*tile as i32 / per_row) * 8);
                    for yy in 0..8 {
                        for xx in 0..8 {
                            let (sx, sy) = (tx + if *hflip { 7 - xx } else { xx }, ty + if *vflip { 7 - yy } else { yy });
                            if sx >= img.w as i32 || sy >= img.h as i32 {
                                continue;
                            }
                            let c = img.idx[(sy * img.w as i32 + sx) as usize];
                            if c != 0 {
                                s.set(x + xx, y + yy, c);
                            }
                        }
                    }
                }
                MenuOp::TextOutline { font, x, y, colour, outline, clip, text } => self.text_outline(&mut s, *font, *x, *y, *colour, *outline, *clip, text),
                MenuOp::AffineObjs { pieces, pa } => {
                    let tex = std::mem::replace(&mut s.px, vec![0; (s.w * s.h).max(0) as usize]);
                    for p in pieces.iter().rev() {
                        let [sx0, sy0, w, h, dx0, dy0] = *p;
                        for y in 0..h {
                            for x in 0..w {
                                let tx = ((pa * (x - w / 2)) >> 8) + w / 2;
                                let ty = ((pa * (y - h / 2)) >> 8) + h / 2;
                                if tx < 0 || ty < 0 || tx >= w || ty >= h {
                                    continue;
                                }
                                let (ax, ay) = (sx0 + tx, sy0 + ty);
                                if ax < 0 || ay < 0 || ax >= s.w || ay >= s.h {
                                    continue;
                                }
                                let c = tex[(ay * s.w + ax) as usize];
                                if c != 0 {
                                    s.set(dx0 + x, dy0 + y, c);
                                }
                            }
                        }
                    }
                }
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

    /// The countdown / stopwatch display (object 0x0802FCA0, draw 0x0802FD16) for `t` frames, as 64x16 ARGB (0 =
    /// transparent): the value in hundredths `(t * 53) >> 5` as M:SS:CC (9:59:99 from 36,000 frames on), each
    /// character in the thick timer font, outlined (colour 5, outline 1: `0x0803F70C`), right-aligned at the
    /// columns 12 / 27 / 37 / 52 / 62 by the dialogue font's advance (`0x0804F9A4`), the colons at 17 / 42 less theirs;
    /// a leading 0 minute and its colon are left out
    pub fn timer(&mut self, packs: &Packs, t: i32) -> Vec<u32> {
        let ch: [u16; 5] = if t >= 36_000 {
            [b'9' as u16, b'5' as u16, b'9' as u16, b'9' as u16, b'9' as u16]
        } else {
            let v = ((t.max(0) as u32 * 53) >> 5) as u32;
            let (m, r) = (v / 6000, v % 6000);
            [b'0' as u16 + m as u16, b'0' as u16 + (r / 1000 % 10) as u16, b'0' as u16 + (r / 100 % 10) as u16, b'0' as u16 + (r / 10 % 10) as u16, b'0' as u16 + (r % 10) as u16]
        };
        let adv = |c: u16| -> i32 {
            self.fonts.get(font_slot(MenuFont::Dialog)).and_then(|f| f.as_ref()).and_then(|f| f.glyphs.get(c as usize).cloned().flatten()).map(|g| g.0 as i32 + 1).unwrap_or(1)
        };
        let colon = b':' as u16;
        let mut put: Vec<(i32, u16)> = vec![];
        if ch[0] != b'0' as u16 {
            put.push((12 - adv(ch[0]), ch[0]));
            put.push((17 - adv(colon), colon));
        }
        put.push((27 - adv(ch[1]), ch[1]));
        put.push((37 - adv(ch[2]), ch[2]));
        put.push((42 - adv(colon), colon));
        put.push((52 - adv(ch[3]), ch[3]));
        put.push((62 - adv(ch[4]), ch[4]));
        let ops: Vec<MenuOp> = put.into_iter().map(|(x, c)| MenuOp::TextOutline { font: MenuFont::Timer, x, y: 12, colour: 5, outline: 1, clip: [0, 0, 64, 16], text: vec![c] }).collect();
        let s = self.surface(packs, &wf_view::MenuLayer { w: 64, h: 16, x: 0, y: 0, priority: 1, ops });
        s.px.iter().map(|&c| if c == 0 { 0 } else { 0xFF00_0000 | (self.pal.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) }).collect()
    }

    /// A script's title text (`0x0802BECC`): (width, 32-px-high ARGB) - w = (the title font's width + 33) rounded
    /// down to 32, the text centred at (w / 2, 22), colour 2 outlined with 1 (`0x0803F70C`), like the minigames'
    /// label (`0x08030860`) one font and 16 px taller
    pub fn title(&mut self, packs: &Packs, text: &[u16]) -> (i32, Vec<u32>) {
        let adv = |c: u16| -> i32 {
            self.fonts.get(font_slot(MenuFont::Title)).and_then(|f| f.as_ref()).and_then(|f| f.glyphs.get(c as usize).cloned().flatten()).map(|g| g.0 as i32 + 1).unwrap_or(0)
        };
        let mut m = 0;
        let mut k = 0;
        while k < text.len() {
            if text[k] == 8 {
                k += 2;
                continue;
            }
            m += adv(text[k]);
            k += 1;
        }
        let w = ((m + 0x21) >> 5) << 5;
        let x = w / 2 - ((m + 1) >> 1);
        let ops = vec![MenuOp::TextOutline { font: MenuFont::Title, x, y: 22, colour: 2, outline: 1, clip: [0, 0, w, 32], text: text.to_vec() }];
        let s = self.surface(packs, &wf_view::MenuLayer { w: w as u16, h: 32, x: 0, y: 0, priority: 0, ops });
        // (the top byte: the OBJ palette entry, for the palette fade)
        (w, s.px.iter().map(|&c| if c == 0 { 0 } else { (c as u32) << 24 | (self.pal.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) }).collect())
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
        // overlay screens (mini-games): u16::MAX = nothing drawn, the field shows through (alpha 0 in the output)
        let mut idx: Vec<u16> = vec![if mv.overlay { u16::MAX } else { mv.backdrop }; 240 * 160];
        let order: Vec<usize> = (0..mv.layers.len()).collect();
        let surfaces: Vec<Surface> = mv.layers.iter().map(|l| self.surface(packs, l)).collect();
        // which layer is on top / under it per pixel (blending): 254 = backdrop, 255 = OBJ
        let mut top_l: Vec<u8> = vec![254; 240 * 160];
        let mut under: Vec<(u16, u8)> = vec![(mv.backdrop, 254); 240 * 160];
        // the PPU's order: priority 3 down to 0; per priority the BGs (view order: later on top), then the OBJs of
        // that priority (last OAM entry first so index 0 ends on top)
        for p in (0..=3u8).rev() {
            for &li in &order {
                let l = &mv.layers[li];
                if l.priority.min(3) != p {
                    continue;
                }
                let s = &surfaces[li];
                for y in 0..160 {
                    for x in 0..240 {
                        let (sx, sy) = (x - l.x, y - l.y);
                        if sx < 0 || sy < 0 || sx >= s.w || sy >= s.h {
                            continue;
                        }
                        let c = s.px[(sy * s.w + sx) as usize];
                        if c != 0 {
                            let k = (y * 240 + x) as usize;
                            under[k] = (idx[k], top_l[k]);
                            idx[k] = c as u16;
                            top_l[k] = li.min(253) as u8;
                        }
                    }
                }
            }
            for o in mv.objs.iter().rev() {
                if o.priority.min(3) != p {
                    continue;
                }
                if !o.sheet.is_empty() {
                    // a sprite-sheet frame: RGBA pixels back to OBJ palette indices (the in-game OBJ palette the sheets
                    // were exported with; the UI palette as fallback), the object's remap, shown through the screen's
                    // palette (both halves = the UI palette in these screens)
                    let Some(sh) = self.sheet(packs, &o.sheet) else { continue };
                    let Some(f) = sh.def.frames.get(o.frame as usize) else { continue };
                    let img = &sh.image;
                    for j in 0..f.h as i32 {
                        let py = o.y - f.anchor.y + j;
                        if !(0..160).contains(&py) {
                            continue;
                        }
                        for i in 0..f.w as i32 {
                            let px = if o.hflip { o.x + f.anchor.x - 1 - i } else { o.x - f.anchor.x + i };
                            if !(0..240).contains(&px) {
                                continue;
                            }
                            let c = img.px[((f.y as i32 + j) as u32 * img.w + f.x + i as u32) as usize];
                            if c >> 24 == 0 {
                                continue;
                            }
                            let mut ci = self.obj_index(packs, c & 0xFF_FFFF) as usize;
                            if let Some(&r) = o.remap.get(ci) {
                                ci = r as usize;
                            }
                            let k = (py * 240 + px) as usize;
                            under[k] = (idx[k], top_l[k]);
                            idx[k] = 256 + ci as u16;
                            top_l[k] = 255;
                        }
                    }
                    continue;
                }
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
                        let k = (py * 240 + px) as usize;
                        under[k] = (idx[k], top_l[k]);
                        idx[k] = c as u16 + o.pal;
                        top_l[k] = 255;
                    }
                }
            }
        }
        let fb = fade_black.min(16) as u32;
        let bit = |l: u8| -> u32 {
            match l {
                255 => 1 << 31,
                254 => 1 << 30,
                n => 1 << n,
            }
        };
        idx.iter()
            .enumerate()
            .map(|(k, &i)| {
                if i == u16::MAX {
                    return 0;
                }
                let mut c = pal.get(i as usize).copied().unwrap_or(0);
                if let Some(b) = mv.blend {
                    let (ui, ul) = under[k];
                    if b.first & bit(top_l[k]) != 0 && b.second & bit(ul) != 0 && top_l[k] != 254 {
                        let d = if ui == u16::MAX { 0 } else { pal.get(ui as usize).copied().unwrap_or(0) };
                        let mut o = 0;
                        for sh in [16, 8, 0] {
                            let v = ((((c >> sh) & 0xFF) * b.eva as u32 + ((d >> sh) & 0xFF) * b.evb as u32) >> 4).min(255);
                            o |= v << sh;
                        }
                        c = o;
                    }
                }
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
