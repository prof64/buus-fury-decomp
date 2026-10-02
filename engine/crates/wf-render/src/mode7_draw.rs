//! World-map drawing (see `mode7.rs` for the camera math): BG2 ground, BG1 sky, and the OBJs (mini-map marker,
//! label, flyer + halo + shadow, billboards) built like the game's draw routines.

use std::sync::Arc;

use wf_content::map::Map;
use wf_content::mode7::Mode7Data;
use wf_content::sprite::{Sheet, SpriteDef};
use wf_content::Packs;
use wf_view::{FrameSel, RenderView};

use crate::mode7::{billboard, div, flyer_from_camera, Camera3d, HORIZON_LINE};
use crate::Renderer;
use wf_view::GBA_H;

/// One OBJ of the world-map frame (GBA screen coordinates of the 240x160 picture).
#[derive(Clone, Debug)]
pub struct M7Obj {
    pub prio: u8,
    /// attr x (9 bit) / y (8 bit), wrapped like the hardware
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub double: bool,
    /// pa, pb, pc, pd (8.8); None = not affine
    pub affine: Option<[i32; 4]>,
    /// w x h ARGB, alpha 0 = transparent
    pub tex: Vec<u32>,
}

/// What a world-map frame needs, prepared once per render.
pub struct M7Frame {
    pub data: Arc<Mode7Data>,
    pub map: Arc<Map>,
    pub bg_cam: Camera3d,
    pub cam: Camera3d,
    pub hofs: i32,
    /// OAM order (index 0 on top)
    pub objs: Vec<M7Obj>,
}

const OBJ_SIZES: [[(i32, i32); 4]; 3] =
    [[(8, 8), (16, 16), (32, 32), (64, 64)], [(16, 8), (32, 8), (32, 16), (64, 32)], [(8, 16), (8, 32), (16, 32), (32, 64)]];

pub fn obj_size(attr0: u32, attr1: u32) -> (i32, i32) {
    let shape = ((attr0 >> 14) & 3).min(2) as usize;
    OBJ_SIZES[shape][((attr1 >> 14) & 3) as usize]
}

/// OBJ texture (w x h) from a sheet frame: `origin` = where the frame's record origin (its anchor point) sits
/// inside the OBJ; flipped pieces are un-flipped (affine OBJs ignore the flip bits); `step_flip` = the step shows
/// the mirrored image of the frame.
#[allow(clippy::too_many_arguments)]
pub fn tex_from_frame(sheet: &Sheet, frame: usize, step_flip: bool, w: i32, h: i32, origin: (i32, i32), obj_hflip: bool, obj_vflip: bool) -> Vec<u32> {
    let mut t = vec![0u32; (w * h) as usize];
    let Some(f) = sheet.def.frames.get(frame) else { return t };
    let img = &sheet.image;
    for j in 0..h {
        for i in 0..w {
            let pi = if obj_hflip { w - 1 - i } else { i };
            let pj = if obj_vflip { h - 1 - j } else { j };
            let rx = pi - origin.0;
            let ry = pj - origin.1;
            let u = if step_flip { f.anchor.x - 1 - rx } else { rx + f.anchor.x };
            let v = ry + f.anchor.y;
            if u < 0 || v < 0 || u >= f.w as i32 || v >= f.h as i32 {
                continue;
            }
            let c = img.px[((f.y as i32 + v) as u32 * img.w + (f.x as i32 + u) as u32) as usize];
            if c >> 24 != 0 {
                t[(j * w + i) as usize] = c | 0xFF00_0000;
            }
        }
    }
    t
}

/// sprite header bytes 8..13: [box w, box h, shadow w, shadow h, shadow dx (s8), shadow dy (u8)]
fn header_bytes(def: &SpriteDef) -> [i32; 6] {
    let h = def.header.as_ref();
    let sh: Vec<i32> = h.and_then(|h| h["shadow"].as_array()).map(|a| a.iter().map(|v| v.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
    let w12 = h.and_then(|h| h["word12"].as_i64()).unwrap_or(0) as u32;
    [
        sh.first().copied().unwrap_or(16),
        sh.get(1).copied().unwrap_or(8),
        sh.get(2).copied().unwrap_or(0),
        sh.get(3).copied().unwrap_or(0),
        (w12 & 0xFF) as u8 as i8 as i32,
        ((w12 >> 8) & 0xFF) as i32,
    ]
}

fn indexed_argb(img: &wf_content::image::Indexed, idx: &[u8]) -> Vec<u32> {
    idx.iter().map(|&c| if c == 0 { 0 } else { img.palette.get(c as usize).copied().unwrap_or(0) | 0xFF00_0000 }).collect()
}

impl Renderer {
    fn mode7_data(&mut self, packs: &Packs, id: &str) -> Option<Arc<Mode7Data>> {
        if let Some(d) = self.mode7.get(id) {
            return d.clone();
        }
        let d = match Mode7Data::load(packs, id) {
            Ok(d) => Some(Arc::new(d)),
            Err(e) => {
                self.errors.push(format!("mode7 {id}: {e}"));
                None
            }
        };
        self.mode7.insert(id.to_string(), d.clone());
        d
    }

    /// The world-map OBJs the renderer would place for a view, in OAM order (tools / tests).
    pub fn mode7_objs(&mut self, packs: &Packs, view: &RenderView) -> Vec<M7Obj> {
        self.mode7_prepare(packs, view).map(|f| f.objs).unwrap_or_default()
    }

    /// Prepare the world-map frame (None on 2D maps).
    pub(crate) fn mode7_prepare(&mut self, packs: &Packs, view: &RenderView) -> Option<M7Frame> {
        let mv = view.map.as_ref()?;
        let v = mv.mode7.as_ref()?;
        let data = self.mode7_data(packs, &mv.id)?;
        let map = self.map(packs, &mv.id)?;
        let wm = view.world_map.as_ref();
        let cam = Camera3d::new(&data, v);
        let bg_cam = wm.and_then(|w| w.bg_camera.as_ref()).map(|b| Camera3d::new(&data, b)).unwrap_or_else(|| cam.clone());
        let hofs = bg_cam.heading.wrapping_shl(3) >> 10;
        let mut objs: Vec<M7Obj> = Vec::new();
        let mut matrices = 0;
        let (fx, fy, fz) = match wm.and_then(|w| w.flyer.as_ref()) {
            Some(f) => (f.x, f.y, f.z),
            None => flyer_from_camera(&data, v),
        };
        // mini-map marker (script object created first: matrix 0, priority 0)
        if let Some(mk) = &data.marker {
            let b = wm.and_then(|w| w.minimap_box).unwrap_or(data.marker_box);
            let (ww, wh) = (map.world_w.max(1), map.world_h.max(1));
            let sx = div((b[2] - b[0]) << 22, ww);
            let sy = div((b[3] - b[1]) << 22, wh);
            let x = ((fx >> 8).wrapping_mul(sx) >> 22) + b[0] - 8;
            let y = ((fy >> 8).wrapping_mul(sy) >> 22) + b[1] - 8;
            let r = cam.rot;
            objs.push(M7Obj { prio: 0, x: x & 511, y: y & 255, w: 8, h: 8, double: true, affine: Some([r[0] >> 2, r[2] >> 2, r[1] >> 2, r[3] >> 2]), tex: indexed_argb(mk, &mk.idx) });
            matrices += 1;
        }
        // bottom label (message trigger 0x17): the 240x16 surface at y 136, priority 1
        if let (Some(id), Some(pic)) = (v.label, &data.label) {
            if self.strings.is_none() {
                self.strings = Some(packs.json::<Vec<String>>("text/strings.json").unwrap_or_default());
            }
            let text = self.strings.as_ref().and_then(|s| s.get(id as usize)).cloned();
            if let Some(text) = text {
                let mut idx = pic.idx.clone();
                if let Ok(font) = crate::ui::Font::load(packs, &data.label_font) {
                    let u = text_units(&text);
                    // the centring callback measures every code unit (0x0804001A: colour pairs included)
                    let wdt: i32 = u.iter().map(|&c| font.advance(c) as i32).sum();
                    let mut col = 2u8;
                    let x0 = 120 - (wdt + 1) / 2;
                    // drop shadow in colour 1 at (+1, +1) (colour pairs ignored), then the text
                    let mut plain = Vec::new();
                    let mut k = 0;
                    while k < u.len() {
                        if u[k] == 8 {
                            k += 2;
                            continue;
                        }
                        plain.push(u[k]);
                        k += 1;
                    }
                    let mut shadow = 1u8;
                    font.draw_line(&mut idx, 240, 16, &plain, x0 + 1, 12, &mut shadow);
                    font.draw_line(&mut idx, 240, 16, &u, x0, 11, &mut col);
                }
                objs.push(M7Obj { prio: 1, x: 0, y: 136, w: 240, h: 16, double: false, affine: None, tex: indexed_argb(pic, &idx) });
            }
        }
        // 3D objects (sort key -depth: nearest first)
        let mut world: Vec<(i32, Vec<M7Obj>)> = Vec::new();
        let player = wm.and_then(|w| w.flyer.as_ref()).filter(|f| !f.sheet.is_empty()).map(|f| (f.sheet.clone(), f.frame as usize, f.flip));
        let player = match player {
            Some(p) => Some(p),
            None => {
                let mut found = None;
                for s in &view.sprites {
                    if !s.sheet.starts_with("sprites/") {
                        continue;
                    }
                    let Some(sh) = self.sheet(packs, &s.sheet) else { continue };
                    if sh.def.category.as_deref() != Some("player") {
                        continue;
                    }
                    found = match &s.frame {
                        FrameSel::Frame { index, flip } => Some((s.sheet.clone(), *index as usize, *flip)),
                        FrameSel::Anim { anim, dir, t, looping } => {
                            sh.def.anim(anim).and_then(|a| a.step_at(*dir, *t, *looping)).map(|(_, st)| (s.sheet.clone(), st.frame as usize, st.flip))
                        }
                    };
                    break;
                }
                found
            }
        };
        if let Some((key, frame, flip)) = player {
            if let Some(sheet) = self.sheet(packs, &key) {
                if let Some(g) = self.flyer_group(packs, &cam, &sheet, frame, flip, (fx, fy, fz)) {
                    world.push(g);
                }
            }
        }
        if let Some(props) = self.sheet(packs, &data.props_sheet) {
            for p in &data.props {
                let i = p.index as u16;
                let vis = match wm {
                    Some(w) if w.props_hidden.contains(&i) => false,
                    Some(w) if w.props_shown.contains(&i) => true,
                    _ => p.visible,
                };
                if !vis {
                    continue;
                }
                let Some((x, y, s, depth)) = billboard(&cam, p) else { continue };
                let (a0, a1) = (p.attr & 0xFFFF, p.attr >> 16);
                let (w, h) = obj_size(a0, a1);
                let tex = tex_from_frame(&props, p.index, false, w, h, (0, 0), a1 & 0x1000 != 0, a1 & 0x2000 != 0);
                world.push((depth, vec![M7Obj { prio: 3, x: x & 511, y: y & 255, w, h, double: true, affine: Some([s, 0, 0, s]), tex }]));
            }
        }
        world.sort_by_key(|(d, _)| *d);
        for (_, group) in world {
            if matrices >= 16 {
                break;
            }
            matrices += 1;
            objs.extend(group);
        }
        Some(M7Frame { data, map, bg_cam, cam, hofs, objs })
    }

    /// The flyer, its halo and shadow (`0x08010098`; draw `0x0800EF94`, affine path): (depth, OBJs in OAM order).
    fn flyer_group(&mut self, packs: &Packs, cam: &Camera3d, sheet: &Arc<Sheet>, frame: usize, flip: bool, pos: (i32, i32, i32)) -> Option<(i32, Vec<M7Obj>)> {
        let (x, y, z) = pos;
        let (sx, sy, depth) = cam.project([x.wrapping_shl(2), y.wrapping_shl(2), z])?;
        if !(-64..=0x130).contains(&sx) || !(-64..=0xE0).contains(&sy) {
            return None;
        }
        let def = &sheet.def;
        let hb = header_bytes(def);
        let ox = sx - hb[0] / 2;
        let oy = sy - hb[1] / 2;
        let s = (depth.wrapping_mul(0x222) >> 17).max(0x80);
        let step = def.animations.values().flat_map(|a| a.directions.values().flatten()).find(|st| st.frame as usize == frame && st.flip == flip).cloned();
        // 0x0800F012: a frame whose first piece is h-flipped mirrors the whole group through PA = -scale (affine
        // OBJs ignore the flip bits; halo, pieces and shadow share the matrix)
        let mirror = step.as_ref().and_then(|st| st.objs.first()).is_some_and(|o| o[5] & 0x1000 != 0);
        let m = Some([if mirror { -s } else { s }, 0, 0, s]);
        let mut out = Vec::new();
        if let Some(st) = &step {
            // halo attachment: 16x8 8bpp affine double, before the body
            if let (Some([ax, ay]), Some(_)) = (st.attach, &def.attachment) {
                if let Some(halo) = self.sheet(packs, "effects/halo") {
                    let hx = div(ax * 192, s) + ox - 8;
                    let hy = div(ay * 192, s) + oy - 12;
                    let tex = tex_from_frame(&halo, 0, false, 16, 8, (0, 0), false, false);
                    out.push(M7Obj { prio: 3, x: hx & 511, y: hy & 255, w: 16, h: 8, double: true, affine: m, tex });
                }
            }
            for o in &st.objs {
                let [dx, dy, bw, bh, a0, a1] = *o;
                let (w, h) = obj_size(a0 as u32, a1 as u32);
                let px = ox + dx - (bw >> 1);
                let py = oy + dy - (bh >> 1);
                let tex = tex_from_frame(sheet, frame, flip, w, h, (-dx, -dy), a1 & 0x1000 != 0, a1 & 0x2000 != 0);
                out.push(M7Obj { prio: 3, x: px & 511, y: py & 255, w, h, double: true, affine: m, tex });
            }
        }
        // ground shadow: the ground point's projection; 16x8, affine, not double
        if let Some(sh) = &def.shadow {
            if let Some((gx, gy, _)) = cam.project([x.wrapping_shl(2), y.wrapping_shl(2), 0]) {
                if (-32..=0x110).contains(&gx) && (-32..=0xC0).contains(&gy) {
                    if let Some(shs) = self.sheet(packs, &format!("effects/{sh}")) {
                        let tex = tex_from_frame(&shs, 0, false, 16, 8, (0, 0), false, false);
                        let x0 = ox + hb[4];
                        let y0 = oy + hb[5] + (gy - sy);
                        out.push(M7Obj { prio: 3, x: x0 & 511, y: y0 & 255, w: 16, h: 8, double: false, affine: m, tex });
                    }
                }
            }
        }
        Some((depth, out))
    }

    /// BG2 ground and BG1 sky of priority `prio` (BG2 first: the higher BG index is further back).
    pub(crate) fn draw_mode7_bgs(&mut self, f: &M7Frame, prio: u8, ox: i32, oy: i32) {
        let d = f.data.clone();
        let map = f.map.clone();
        let ts = &map.tileset;
        let (w, h) = (self.w, self.h);
        if d.ground_priority == prio {
            let gw = d.ground_w as i32 * 8;
            let gh = d.ground_h as i32 * 8;
            let layer = d.ground_bg;
            for py in 0..h {
                let line = py as i32 - oy;
                // zoomed-out views: nothing above the GBA picture (only sky); below it the scanline angles continue
                if line < 0 {
                    continue;
                }
                let (pa, pc, x0, y0) = if line >= HORIZON_LINE as i32 {
                    f.bg_cam.scanline(&d, line as usize)
                } else {
                    // above the table: the last values written (all -1), PB = PD = -1 per line
                    (-1, -1, -1 - line.max(0), -1 - line.max(0))
                };
                for px in 0..w {
                    let sx = px as i32 - ox;
                    let lx = x0.wrapping_add(sx.wrapping_mul(pa)) >> 8;
                    let ly = y0.wrapping_add(sx.wrapping_mul(pc)) >> 8;
                    // every on-screen sample shows the world tile at its true position (the ring streams what the
                    // frustum needs); outside the world the map wraps
                    let wx = lx.rem_euclid(gw.max(1));
                    let wy = ly.rem_euclid(gh.max(1));
                    let t = d.ground[((wy >> 3) * (gw >> 3) + (wx >> 3)) as usize] as usize;
                    let ix = ts.tiles.get(t * 64 + ((wy & 7) * 8 + (wx & 7)) as usize).copied().unwrap_or(0);
                    if ix != 0 {
                        let c = ts.palette.get(ix as usize).copied().unwrap_or(0) | 0xFF00_0000;
                        self.put(py * w + px, c, layer, 0);
                    }
                }
            }
        }
        if d.pano_priority == prio {
            let hofs = f.hofs;
            let k = (hofs >> 3) & 0x1E;
            let r2 = ((hofs >> 8) & 1) as usize;
            let layer = d.pano_bg;
            for py in 0..h {
                let line = py as i32 - oy;
                // zoomed-out views: the sky map's top row continues above the picture; below it nothing
                if line >= GBA_H {
                    continue;
                }
                let my = line.max(0) & 255;
                let row = (my >> 3) as usize;
                for px in 0..w {
                    let mx = (px as i32 - ox + hofs) & 255;
                    let col = (mx >> 3) as usize;
                    let ch = if col as i32 >= k { r2 } else { r2 ^ 1 };
                    let e = d.pano[ch][row * 32 + col];
                    let t = (e & 0x3FF) as usize;
                    let tx = if e & 0x400 != 0 { 7 - (mx & 7) } else { mx & 7 };
                    let ty = if e & 0x800 != 0 { 7 - (my & 7) } else { my & 7 };
                    let ix = ts.tiles.get(t * 64 + (ty * 8 + tx) as usize).copied().unwrap_or(0);
                    if ix != 0 {
                        let c = ts.palette.get(ix as usize).copied().unwrap_or(0) | 0xFF00_0000;
                        self.put(py * w + px, c, layer, 0);
                    }
                }
            }
        }
        // zoomed-out views: BG0's horizon haze (a fixed 240 px picture that also holds the mini-map) continues
        // sideways with its left column, drawn here (same priority, BG0 is the front BG) outside its own columns
        if let Some(l0) = map.layers.iter().find(|l| l.bg == 0 && l.priority == prio && l.rate_x == 0) {
            let lw = (l0.w_tiles * 8) as i32;
            for py in 0..h {
                let ly = py as i32 - oy;
                if !(0..GBA_H).contains(&ly) {
                    continue;
                }
                let Some((t, fh, fv)) = l0.tile(0, (ly >> 3) as u32) else { continue };
                let r = if fv { 7 - (ly & 7) } else { ly & 7 };
                let cx = if fh { 7 } else { 0 };
                let ix = ts.tiles.get(t as usize * 64 + (r * 8 + cx) as usize).copied().unwrap_or(0);
                if ix == 0 {
                    continue;
                }
                let c = ts.palette.get(ix as usize).copied().unwrap_or(0) | 0xFF00_0000;
                for px in 0..w {
                    let sx = px as i32 - ox;
                    if sx < 0 || sx >= lw {
                        self.put(py * w + px, c, 0, 0);
                    }
                }
            }
        }
    }

    /// The world-map OBJs of priority `prio`, last OAM entry first.
    pub(crate) fn draw_mode7_objs(&mut self, f: &M7Frame, prio: u8, ox: i32, oy: i32) {
        for o in f.objs.iter().rev() {
            if o.prio != prio {
                continue;
            }
            let (bw, bh) = if o.double { (2 * o.w, 2 * o.h) } else { (o.w, o.h) };
            let x = if o.x >= 240 { o.x - 512 } else { o.x };
            let y = if o.y >= 160 { o.y - 256 } else { o.y };
            // the bottom label stays at the bottom edge of zoomed-out views
            let oy = if o.affine.is_none() && o.w == 240 { self.h as i32 - GBA_H } else { oy };
            for j in 0..bh {
                let syy = y + j + oy;
                if syy < 0 || syy >= self.h as i32 {
                    continue;
                }
                for i in 0..bw {
                    let sxx = x + i + ox;
                    if sxx < 0 || sxx >= self.w as i32 {
                        continue;
                    }
                    let (tx, ty) = match o.affine {
                        Some([pa, pb, pc, pd]) => {
                            let lx = i - bw / 2;
                            let ly = j - bh / 2;
                            (((pa * lx + pb * ly) >> 8) + o.w / 2, ((pc * lx + pd * ly) >> 8) + o.h / 2)
                        }
                        None => (i, j),
                    };
                    if tx < 0 || ty < 0 || tx >= o.w || ty >= o.h {
                        continue;
                    }
                    let c = o.tex[(ty * o.w + tx) as usize];
                    if c >> 24 != 0 {
                        self.put(syy as usize * self.w + sxx as usize, c, crate::L_OBJ, 0);
                    }
                }
            }
        }
    }
}

/// String-table text to code units: `{XXXX}` escapes are raw code units (colour pairs `0x0008, n`, ...).
pub fn text_units(s: &str) -> Vec<u16> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if b[i] == b'{' && i + 5 < b.len() && b[i + 5] == b'}' {
            if let Ok(v) = u16::from_str_radix(&s[i + 1..i + 5], 16) {
                out.push(v);
                i += 6;
                continue;
            }
        }
        let c = s[i..].chars().next().unwrap();
        out.extend(crate::ui::units(&c.to_string()));
        i += c.len_utf8();
    }
    out
}
