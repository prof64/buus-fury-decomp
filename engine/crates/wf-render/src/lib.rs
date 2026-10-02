//! Software compositor with GBA semantics: turns a [`RenderView`] + content into an ARGB framebuffer of any size.
//!
//! Why software: the game is 2D tiles + OBJs at integer positions; composing them on the CPU the way the GBA PPU
//! does (BG priorities, OBJ-over-BG ties, top-two-layer alpha blending with BLDCNT/EVA/EVB, tile animations,
//! per-layer scroll rates) is cheap (~1-3 ms for a 640x360 zoomed-out view) and is pixel-exact by construction —
//! identical on every platform, testable with golden hashes. The host only uploads the framebuffer and scales
//! it by an integer factor (canvas `drawImage` with smoothing off on the web, a nearest-neighbour blit natively).
//!
//! Composition order (GBA): for priority 3 down to 0, the BGs with that priority (BG3 first, BG0 last), then the
//! OBJs with that priority (last OAM entry first). The two top-most opaque pixels are kept per screen pixel for
//! blending.

pub mod font;

use std::collections::HashMap;
use std::sync::Arc;

use wf_content::map::{LayerKind, Map};
use wf_content::sprite::Sheet;
use wf_content::Packs;
use wf_view::{FrameSel, RenderView, SpriteEffect, GBA_H, GBA_W};

/// How the window maps to world pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// The original 240x160 screen, scaled by the largest integer that fits (or `scale` when > 0).
    Gba { scale: u32 },
    /// PC "zoomed out": the view is the window size / an integer zoom (0 = auto: the smallest zoom >= 2 whose
    /// view the map covers with <= 8 % black margin, max 6), centred on the game's camera.
    Pc { zoom: u32 },
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Pc { zoom: 0 }
    }
}

/// View size in world pixels and the integer scale to present it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub view_w: u32,
    pub view_h: u32,
    pub scale: u32,
}

pub fn auto_zoom(win_w: u32, win_h: u32, world: Option<(i32, i32)>) -> u32 {
    let mut z = 2;
    if let Some((ww, wh)) = world.filter(|(w, h)| *w > 0 && *h > 0) {
        let black = |z: u32| {
            let vw = (win_w / z).max(1) as f64;
            let vh = (win_h / z).max(1) as f64;
            1.0 - (ww as f64 / vw).min(1.0) * (wh as f64 / vh).min(1.0)
        };
        while z < 6 && black(z) > 0.08 {
            z += 1;
        }
    }
    z
}

pub fn fit(mode: ViewMode, win_w: u32, win_h: u32, world: Option<(i32, i32)>) -> Fit {
    let (win_w, win_h) = (win_w.max(1), win_h.max(1));
    let max_z = (win_w / GBA_W as u32).min(win_h / GBA_H as u32).max(1);
    match mode {
        ViewMode::Gba { scale } => {
            Fit { view_w: GBA_W as u32, view_h: GBA_H as u32, scale: if scale > 0 { scale } else { max_z } }
        }
        ViewMode::Pc { zoom } => {
            let z = if zoom > 0 { zoom } else { auto_zoom(win_w, win_h, world) }.clamp(1, max_z);
            Fit { view_w: (win_w / z).max(1), view_h: (win_h / z).max(1), scale: z }
        }
    }
}

const L_OBJ: u8 = 4;
const L_BD: u8 = 5;

pub struct Renderer {
    sheets: HashMap<String, Option<Arc<Sheet>>>,
    maps: HashMap<String, Option<Arc<Map>>>,
    top: Vec<u32>,
    top_l: Vec<u8>,
    top_b: Vec<u16>,
    bot: Vec<u32>,
    bot_l: Vec<u8>,
    remap: Vec<u32>,
    /// Output, `0xAARRGGBB`, `w * h`.
    pub fb: Vec<u32>,
    pub w: usize,
    pub h: usize,
    /// Content problems (missing sheet / map), each reported once.
    pub errors: Vec<String>,
    pub draw_hud: bool,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn c5(c: u32, sh: u32) -> u32 {
    (c >> sh) & 0xFF
}

/// GBA alpha blend on 5-bit channels, output expanded like the PNGs (`c5 << 3 | c5 >> 2`).
#[inline]
fn blend(a: u32, b: u32, eva: u32, evb: u32) -> u32 {
    let mut out = 0xFF00_0000;
    for sh in [16, 8, 0] {
        let x = c5(a, sh) >> 3;
        let y = c5(b, sh) >> 3;
        let v = ((x * eva + y * evb) >> 4).min(31);
        out |= ((v << 3) | (v >> 2)) << sh;
    }
    out
}

#[inline]
fn darken(a: u32, evy: u32) -> u32 {
    let mut out = 0xFF00_0000;
    for sh in [16, 8, 0] {
        let x = c5(a, sh) >> 3;
        let v = x - ((x * evy) >> 4);
        out |= ((v << 3) | (v >> 2)) << sh;
    }
    out
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            sheets: HashMap::new(),
            maps: HashMap::new(),
            top: Vec::new(),
            top_l: Vec::new(),
            top_b: Vec::new(),
            bot: Vec::new(),
            bot_l: Vec::new(),
            remap: Vec::new(),
            fb: Vec::new(),
            w: 0,
            h: 0,
            errors: Vec::new(),
            draw_hud: true,
        }
    }

    /// Drop cached assets (content / mods changed).
    pub fn clear_cache(&mut self) {
        self.sheets.clear();
        self.maps.clear();
    }

    pub fn sheet(&mut self, packs: &Packs, key: &str) -> Option<Arc<Sheet>> {
        if let Some(s) = self.sheets.get(key) {
            return s.clone();
        }
        let s = match Sheet::load(packs, key) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                self.errors.push(format!("sheet {key}: {e}"));
                None
            }
        };
        self.sheets.insert(key.to_string(), s.clone());
        s
    }

    pub fn map(&mut self, packs: &Packs, id: &str) -> Option<Arc<Map>> {
        if let Some(m) = self.maps.get(id) {
            return m.clone();
        }
        let m = match Map::load(packs, id) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.errors.push(format!("map {id}: {e}"));
                None
            }
        };
        self.maps.insert(id.to_string(), m.clone());
        m
    }

    fn resize(&mut self, w: usize, h: usize) {
        let n = w * h;
        self.w = w;
        self.h = h;
        for v in [&mut self.top, &mut self.bot, &mut self.fb] {
            v.resize(n, 0);
        }
        self.top_l.resize(n, 0);
        self.bot_l.resize(n, 0);
        self.top_b.resize(n, 0);
    }

    /// World rectangle shown for local player `player` in a `w` x `h` view: centred on its 240x160 camera.
    pub fn view_origin(view: &RenderView, player: usize, w: usize, h: usize) -> (i32, i32, i32, i32) {
        let cam = view.cameras.get(player).or(view.cameras.first()).copied().unwrap_or_default();
        let vx = cam.x + GBA_W / 2 - w as i32 / 2;
        let vy = cam.y + GBA_H / 2 - h as i32 / 2;
        (vx, vy, cam.x, cam.y)
    }

    /// Compose the view for local player `player` into `self.fb` (`w` x `h` world pixels).
    pub fn render(&mut self, packs: &Packs, view: &RenderView, player: usize, w: usize, h: usize) -> &[u32] {
        self.resize(w, h);
        let (vx, vy, cx, cy) = Self::view_origin(view, player, w, h);
        let map = view.map.as_ref().and_then(|m| self.map(packs, &m.id).map(|g| (g, m.anim_frame, m.shake)));
        let backdrop = map.as_ref().map(|(m, _, _)| m.backdrop).unwrap_or(0xFF00_0000);
        self.top.fill(backdrop);
        self.top_l.fill(L_BD);
        self.top_b.fill(0);
        self.bot.fill(backdrop);
        self.bot_l.fill(L_BD);

        // sprites resolved once: (priority, oam index, sheet, frame index, flip)
        let mut objs: Vec<(u8, usize, Arc<Sheet>, usize, bool)> = Vec::with_capacity(view.sprites.len());
        for (i, s) in view.sprites.iter().enumerate() {
            let Some(sheet) = self.sheet(packs, &s.sheet) else { continue };
            let (fi, flip) = match &s.frame {
                FrameSel::Frame { index, flip } => (*index as usize, *flip),
                FrameSel::Anim { anim, dir, t, looping } => match sheet.def.anim(anim).and_then(|a| a.step_at(*dir, *t, *looping)) {
                    Some((_, st)) => (st.frame as usize, st.flip),
                    None => continue,
                },
            };
            objs.push((s.priority.min(3), i, sheet, fi, flip ^ s.mirror));
        }

        for prio in (0..4u8).rev() {
            if let Some((m, af, shake)) = &map {
                // BGs of this priority, higher BG index further back
                let mut bgs: Vec<usize> = (0..m.layers.len()).filter(|&i| m.layers[i].priority == prio).collect();
                bgs.sort_by(|&a, &b| m.layers[b].bg.cmp(&m.layers[a].bg));
                for li in bgs {
                    self.draw_layer(m, li, *af, cx + shake.0, cy + shake.1, vx, vy);
                }
            }
            // OBJs of this priority, last OAM entry first
            for (p, i, sheet, fi, flip) in objs.iter().rev() {
                if *p == prio {
                    let s = &view.sprites[*i];
                    self.draw_obj(sheet, *fi, *flip, s.x - vx, s.y - vy, s.effect);
                }
            }
        }

        // blending + output
        let (bld, eva, evb) = map.as_ref().map(|(m, _, _)| (m.bldcnt as u32, m.eva as u32, m.evb as u32)).unwrap_or((0, 16, 0));
        let mode = (bld >> 6) & 3;
        let fade = view.fade_black.min(16) as u32;
        for i in 0..w * h {
            let tl = self.top_l[i];
            let mut c = self.top[i];
            let bl = self.bot_l[i];
            let second = bld >> 8 & (1 << bl) != 0;
            if self.top_b[i] != 0 {
                let b = self.top_b[i];
                if b == 0xFFFF {
                    if second {
                        c = blend(c, self.bot[i], eva.min(16), evb.min(16));
                    }
                } else {
                    c = blend(c, self.bot[i], (b & 0xFF) as u32, (b >> 8) as u32);
                }
            } else if mode == 1 && (bld & (1 << tl)) != 0 && second {
                c = blend(c, self.bot[i], eva.min(16), evb.min(16));
            }
            if fade != 0 {
                c = darken(c, fade);
            }
            self.fb[i] = c;
        }

        if self.draw_hud {
            self.draw_hud(view, player);
        }
        &self.fb
    }

    #[inline]
    fn put(&mut self, i: usize, c: u32, layer: u8, b: u16) {
        self.bot[i] = self.top[i];
        self.bot_l[i] = self.top_l[i];
        self.top[i] = c;
        self.top_l[i] = layer;
        self.top_b[i] = b;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_layer(&mut self, m: &Map, li: usize, anim_frame: u32, cx: i32, cy: i32, vx: i32, vy: i32) {
        let l = &m.layers[li];
        let ts = &m.tileset;
        // tile animation remap for this frame
        if self.remap.len() != ts.count as usize {
            self.remap = (0..ts.count).collect();
        }
        for (i, r) in self.remap.iter_mut().enumerate() {
            *r = i as u32;
        }
        for a in &ts.anims {
            let k = a.frame_at(anim_frame);
            for (i, &src) in a.src[k].iter().enumerate() {
                if let Some(r) = self.remap.get_mut((a.dest + i as u32) as usize) {
                    *r = src;
                }
            }
        }
        let (sx, sy) = l.scroll(cx, cy, anim_frame);
        // screen pixel (px, py) of this view is camera-relative (vx - cx + px); layer pixel = that + scroll
        let ox = vx - cx + sx;
        let oy = vy - cy + sy;
        let (lw, lh) = ((l.w_tiles * 8) as i32, (l.h_tiles * 8) as i32);
        let wrap = l.kind == LayerKind::Wrap;
        let (w, h) = (self.w, self.h);
        let layer = l.bg;
        for py in 0..h {
            let mut ly = oy + py as i32;
            if wrap {
                ly = ly.rem_euclid(256);
            } else if ly < 0 || ly >= lh {
                continue;
            }
            let ty = (ly >> 3) as u32;
            let row_in = (ly & 7) as usize;
            let mut px = 0usize;
            while px < w {
                let mut lx = ox + px as i32;
                if wrap {
                    lx = lx.rem_euclid(256);
                } else if lx < 0 || lx >= lw {
                    // skip to the layer edge
                    if lx < 0 {
                        px += (-lx) as usize;
                    } else {
                        break;
                    }
                    continue;
                }
                let tx = (lx >> 3) as u32;
                let col = (lx & 7) as usize;
                let run = (8 - col).min(w - px);
                if let Some((t, fh, fv)) = l.tile(tx, ty) {
                    let t = self.remap.get(t as usize).copied().unwrap_or(t);
                    if t < ts.count {
                        let r = if fv { 7 - row_in } else { row_in };
                        let base = t as usize * 64 + r * 8;
                        let row = &ts.tiles[base..base + 8];
                        let o = py * w + px;
                        for k in 0..run {
                            let c = col + k;
                            let ix = row[if fh { 7 - c } else { c }];
                            if ix != 0 {
                                let colr = ts.palette.get(ix as usize).copied().unwrap_or(0) | 0xFF00_0000;
                                self.put(o + k, colr, layer, 0);
                            }
                        }
                    }
                }
                px += run;
            }
        }
    }

    fn draw_obj(&mut self, sheet: &Sheet, fi: usize, flip: bool, x: i32, y: i32, eff: SpriteEffect) {
        let Some(f) = sheet.def.frames.get(fi) else { return };
        let img = &sheet.image;
        let b: u16 = match eff {
            SpriteEffect::SemiTransparent => 0xFFFF,
            SpriteEffect::Blend { eva, evb } => (eva.min(16) as u16) | (evb.min(16) as u16) << 8,
            _ => 0,
        };
        let white = eff == SpriteEffect::White;
        let (w, h) = (self.w as i32, self.h as i32);
        for j in 0..f.h as i32 {
            let sy = y - f.anchor.y + j;
            if sy < 0 || sy >= h {
                continue;
            }
            let src_row = ((f.y as i32 + j) as u32 * img.w) as usize;
            for i in 0..f.w as i32 {
                let sx = if flip { x + f.anchor.x - 1 - i } else { x - f.anchor.x + i };
                if sx < 0 || sx >= w {
                    continue;
                }
                let c = img.px[src_row + (f.x as i32 + i) as usize];
                if c >> 24 == 0 {
                    continue;
                }
                let c = if white { 0xFFFF_FFFF } else { c | 0xFF00_0000 };
                self.put(sy as usize * self.w + sx as usize, c, L_OBJ, b);
            }
        }
    }

    /// Placeholder HUD (the game's HUD graphics are not extracted yet): name, level, HP / EP bars, top-left.
    fn draw_hud(&mut self, view: &RenderView, player: usize) {
        let Some(hud) = view.hud.get(player) else { return };
        let (w, h) = (self.w, self.h);
        let fb = &mut self.fb;
        let label = format!("{} LV{}", hud.name, hud.level);
        font::draw_text(fb, w, h, 4, 4, &label, 0xFFFF_FFFF);
        let bar = |fb: &mut [u32], y: i32, v: i32, max: i32, col: u32| {
            let bw = 60;
            font::fill_rect(fb, w, h, 4, y, bw + 2, 5, 0xFF10_1010);
            let fill = if max > 0 { (v.clamp(0, max) as i64 * bw as i64 / max as i64) as i32 } else { 0 };
            font::fill_rect(fb, w, h, 5, y + 1, fill, 3, col);
        };
        bar(fb, 14, hud.hp, hud.hp_max, 0xFF30_D040);
        bar(fb, 21, hud.ep, hud.ep_max, 0xFF40_90F0);
    }
}

/// FNV-1a over the framebuffer (golden-image tests, cross-platform determinism checks).
pub fn hash_fb(fb: &[u32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &p in fb {
        for b in p.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}
