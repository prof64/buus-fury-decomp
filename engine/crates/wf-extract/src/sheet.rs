//! Frame collection, de-duplication (incl. mirrored duplicates), sheet packing and the per-entity JSON —
//! the Rust twin of `tools/extract/export.py` + `render.py` (same packing, so sheets are pixel-identical).

use std::collections::HashMap;
use wf_rom::gfx::Image;

/// Trim to the opaque bounding box; the origin moves with it (`render.trim`).
pub fn trim(im: &Image, ox: i32, oy: i32) -> (Image, i32, i32) {
    match im.bbox() {
        None => (Image::new(1, 1), ox, oy),
        Some((x0, y0, x1, y1)) => (im.crop(x0, y0, x1, y1), ox - x0 as i32, oy - y0 as i32),
    }
}

pub struct Frame {
    pub img: Image,
    pub ox: i32,
    pub oy: i32,
    pub rom: Option<u32>,
}

#[derive(Default)]
pub struct Builder {
    pub frames: Vec<Frame>,
    index: HashMap<(Image, i32, i32), usize>,
    mirror: HashMap<(Image, i32, i32), usize>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Add a composed frame whose entity origin is at (ox, oy). Returns (frame index, flip).
    pub fn add(&mut self, im: &Image, ox: i32, oy: i32, rom: Option<u32>) -> (usize, bool) {
        let (im, ox, oy) = trim(im, ox, oy);
        let k = (im, ox, oy);
        if let Some(&i) = self.index.get(&k) {
            return (i, false);
        }
        if let Some(&i) = self.mirror.get(&k) {
            return (i, true);
        }
        let i = self.frames.len();
        let m = k.0.flip_h();
        let mw = m.w as i32;
        self.mirror.entry((m, mw - k.1, k.2)).or_insert(i);
        self.frames.push(Frame { img: k.0.clone(), ox: k.1, oy: k.2, rom });
        self.index.insert(k, i);
        (i, false)
    }

    /// Pack into a sheet like `export.EntityBuilder.write`. Returns (sheet, positions).
    pub fn pack(&self) -> (Image, Vec<(u32, u32)>) {
        let imgs: Vec<&Image> = self.frames.iter().map(|f| &f.img).collect();
        let area: u64 = imgs.iter().map(|i| (i.w * i.h) as u64).sum();
        let max_w = 256i64.max(1024i64.min(((area as f64).sqrt() * 1.3) as i64 + 1)) as u32;
        pack(&imgs, max_w, 1)
    }

    /// Frame records for the entity JSON: sheet rect + anchor (+ ROM provenance).
    pub fn frame_json(&self, pos: &[(u32, u32)]) -> Vec<serde_json::Value> {
        self.frames
            .iter()
            .zip(pos)
            .map(|(f, p)| {
                serde_json::json!({
                    "x": p.0, "y": p.1, "w": f.img.w, "h": f.img.h,
                    "anchor": {"x": f.ox, "y": f.oy},
                    "rom": f.rom.map(wf_rom::hx),
                })
            })
            .collect()
    }
}

/// Shelf packer (`render.pack`): tallest first (stable), rows of at most `max_w` px, 1 px padding.
pub fn pack(images: &[&Image], max_w: u32, pad: u32) -> (Image, Vec<(u32, u32)>) {
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(images[i].h));
    let mut pos = vec![(0u32, 0u32); images.len()];
    let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
    let width = images.iter().map(|i| i.w + pad).chain(std::iter::once(max_w)).max().unwrap();
    for &i in &order {
        let im = images[i];
        if x + im.w > width {
            x = 0;
            y += shelf + pad;
            shelf = 0;
        }
        pos[i] = (x, y);
        x += im.w + pad;
        shelf = shelf.max(im.h);
    }
    let h = y + shelf;
    let w = pos.iter().zip(images).map(|(p, i)| p.0 + i.w).max().unwrap_or(1);
    let mut sheet = Image::new(w.max(1), h.max(1));
    for (i, im) in images.iter().enumerate() {
        sheet.paste(im, pos[i].0, pos[i].1);
    }
    (sheet, pos)
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut pend = false;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pend && !out.is_empty() {
                out.push('_');
            }
            pend = false;
            out.push(c);
        } else {
            pend = true;
        }
    }
    out
}
