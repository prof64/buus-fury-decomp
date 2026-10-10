//! GBA graphics primitives: BGR555 colours, 8bpp/4bpp tile decoding, OBJ shapes, an RGBA image and PNG I/O.

use crate::Rom;

pub type Rgb = [u8; 3];
pub type Rgba = [u8; 4];

/// BGR555 → RGB888 the way the Python tools expand it (`c << 3 | c >> 2`).
#[inline]
pub fn bgr555(c: u16) -> Rgb {
    let r = ((c & 31) << 3) as u8;
    let g = (((c >> 5) & 31) << 3) as u8;
    let b = (((c >> 10) & 31) << 3) as u8;
    [r | r >> 5, g | g >> 5, b | b >> 5]
}

/// `n` BGR555 colours at `a`.
pub fn palette(rom: &Rom, a: u32, n: usize) -> Vec<Rgb> {
    (0..n).map(|i| bgr555(rom.u16(a + 2 * i as u32))).collect()
}

/// OBJ (shape, size) → (width, height) in pixels.
pub fn obj_size(shape: u16, size: u16) -> Option<(u32, u32)> {
    Some(match (shape, size) {
        (0, 0) => (8, 8),
        (0, 1) => (16, 16),
        (0, 2) => (32, 32),
        (0, 3) => (64, 64),
        (1, 0) => (16, 8),
        (1, 1) => (32, 8),
        (1, 2) => (32, 16),
        (1, 3) => (64, 32),
        (2, 0) => (8, 16),
        (2, 1) => (8, 32),
        (2, 2) => (16, 32),
        (2, 3) => (32, 64),
        _ => return None,
    })
}

/// A plain RGBA8 image (row-major).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<Rgba>,
}

impl Image {
    pub fn new(w: u32, h: u32) -> Image {
        Image { w, h, px: vec![[0, 0, 0, 0]; (w * h) as usize] }
    }
    pub fn filled(w: u32, h: u32, c: Rgba) -> Image {
        Image { w, h, px: vec![c; (w * h) as usize] }
    }
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Rgba {
        self.px[(y * self.w + x) as usize]
    }
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, c: Rgba) {
        self.px[(y * self.w + x) as usize] = c;
    }
    pub fn flip_h(&self) -> Image {
        let mut o = Image::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                o.set(self.w - 1 - x, y, self.get(x, y));
            }
        }
        o
    }
    pub fn flip_v(&self) -> Image {
        let mut o = Image::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                o.set(x, self.h - 1 - y, self.get(x, y));
            }
        }
        o
    }
    /// Bounding box of non-transparent pixels: (x0, y0, x1, y1) exclusive, like PIL `getbbox`.
    pub fn bbox(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.get(x, y)[3] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 == 0 {
            None
        } else {
            Some((x0, y0, x1, y1))
        }
    }
    pub fn crop(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> Image {
        let mut o = Image::new(x1 - x0, y1 - y0);
        for y in y0..y1 {
            for x in x0..x1 {
                if x < self.w && y < self.h {
                    o.set(x - x0, y - y0, self.get(x, y));
                }
            }
        }
        o
    }
    /// Source-over composite of `src` at (dx, dy) (`PIL.Image.alpha_composite` for 0/255 alpha, which is
    /// all the game produces; partial alpha is blended).
    pub fn draw(&mut self, src: &Image, dx: i32, dy: i32) {
        for y in 0..src.h as i32 {
            let ty = dy + y;
            if ty < 0 || ty >= self.h as i32 {
                continue;
            }
            for x in 0..src.w as i32 {
                let tx = dx + x;
                if tx < 0 || tx >= self.w as i32 {
                    continue;
                }
                let s = src.get(x as u32, y as u32);
                if s[3] == 255 {
                    self.set(tx as u32, ty as u32, s);
                } else if s[3] != 0 {
                    let d = self.get(tx as u32, ty as u32);
                    let sa = s[3] as u32;
                    let da = d[3] as u32 * (255 - sa) / 255;
                    let oa = sa + da;
                    let mix = |sc: u8, dc: u8| ((sc as u32 * sa + dc as u32 * da) / oa.max(1)) as u8;
                    self.set(tx as u32, ty as u32, [mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), oa as u8]);
                }
            }
        }
    }
    /// Plain copy (PIL `paste`): overwrites including transparent pixels.
    pub fn paste(&mut self, src: &Image, dx: u32, dy: u32) {
        for y in 0..src.h {
            for x in 0..src.w {
                if dx + x < self.w && dy + y < self.h {
                    self.set(dx + x, dy + y, src.get(x, y));
                }
            }
        }
    }
    pub fn to_png(&self) -> Vec<u8> {
        encode_png_rgba(self.w, self.h, &self.px)
    }
    /// RGB PNG (alpha dropped), like PIL `.convert('RGB')`.
    pub fn to_png_rgb(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity(self.px.len() * 3);
        for p in &self.px {
            raw.extend_from_slice(&p[..3]);
        }
        encode_png(self.w, self.h, png::ColorType::Rgb, &raw)
    }
}

/// RGBA PNG. Images with at most 256 distinct colours (everything the GBA produces) are stored as an
/// indexed PNG with a tRNS chunk — pixel-for-pixel the same RGBA after decoding, several times smaller.
/// Fully transparent pixels are normalised to (0,0,0,0).
pub fn encode_png_rgba(w: u32, h: u32, px: &[Rgba]) -> Vec<u8> {
    let mut map: std::collections::HashMap<Rgba, u8> = std::collections::HashMap::new();
    let mut pal: Vec<Rgba> = Vec::new();
    let mut idx = Vec::with_capacity(px.len());
    for p in px {
        let p = if p[3] == 0 { [0, 0, 0, 0] } else { *p };
        let i = match map.get(&p) {
            Some(&i) => i,
            None => {
                if pal.len() == 256 {
                    return encode_png(w, h, png::ColorType::Rgba, px.as_flattened());
                }
                // keep transparent entries first so tRNS stays short
                pal.push(p);
                let i = (pal.len() - 1) as u8;
                map.insert(p, i);
                i
            }
        };
        idx.push(i);
    }
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w.max(1), h.max(1));
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(pal.iter().flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
        let last_alpha = pal.iter().rposition(|c| c[3] != 255);
        if let Some(n) = last_alpha {
            e.set_trns(pal[..=n].iter().map(|c| c[3]).collect::<Vec<u8>>());
        }
        e.set_compression(png::Compression::Default);
        let mut wr = e.write_header().expect("png header");
        wr.write_image_data(&idx).expect("png data");
    }
    out
}

fn encode_png(w: u32, h: u32, ct: png::ColorType, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w.max(1), h.max(1));
        e.set_color(ct);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::Default);
        let mut wr = e.write_header().expect("png header");
        wr.write_image_data(raw).expect("png data");
    }
    out
}

/// Indexed (palette) PNG, used for tilesets so modders can edit them with palette-aware tools.
/// `trns`: index 0 transparent when true.
pub fn encode_png_indexed(w: u32, h: u32, idx: &[u8], pal: &[Rgb], index0_transparent: bool) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w.max(1), h.max(1));
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        let mut plte = Vec::with_capacity(768);
        for i in 0..256 {
            plte.extend_from_slice(pal.get(i).unwrap_or(&[0, 0, 0]));
        }
        e.set_palette(plte);
        if index0_transparent {
            e.set_trns(vec![0u8]);
        }
        e.set_compression(png::Compression::Default);
        let mut wr = e.write_header().expect("png header");
        wr.write_image_data(idx).expect("png data");
    }
    out
}

/// Decode any 8-bit PNG (RGB, RGBA, grey, indexed) to RGBA. Used by parity tests.
pub fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let mut px = Vec::with_capacity((w * h) as usize);
    let n = info.color_type.samples();
    for i in 0..(w * h) as usize {
        let s = &buf[i * n..i * n + n];
        px.push(match n {
            1 => [s[0], s[0], s[0], 255],
            2 => [s[0], s[0], s[0], s[1]],
            3 => [s[0], s[1], s[2], 255],
            _ => [s[0], s[1], s[2], s[3]],
        });
    }
    Ok(Image { w, h, px })
}

/// Decode 1D-mapped OBJ tile data into a W×H image (`tools/extract/render.py:obj_image`).
/// 8bpp: pixel index (after the optional remap) 0 is transparent. 4bpp: colour 0 of the bank is transparent.
pub fn obj_image(data: &[u8], w: u32, h: u32, bpp: u32, pal: &[Rgb], remap: Option<&[u8]>, palbank: u32) -> Image {
    let mut im = Image::new(w, h);
    let tw = w / 8;
    let tsz = if bpp == 8 { 64 } else { 32 };
    let ntiles = ((w / 8) * (h / 8)).min((data.len() / tsz) as u32);
    for t in 0..ntiles {
        let (ox, oy) = ((t % tw) * 8, (t / tw) * 8);
        let base = (t as usize) * tsz;
        for i in 0..64u32 {
            let c: usize = if bpp == 8 {
                let mut c = data[base + i as usize] as usize;
                if let Some(r) = remap {
                    c = r[c] as usize;
                }
                c
            } else {
                let b = data[base + (i >> 1) as usize];
                let c = if i & 1 == 1 { b >> 4 } else { b & 15 } as usize;
                if c != 0 {
                    c + palbank as usize * 16
                } else {
                    0
                }
            };
            let vis = if bpp == 4 { c & 15 != 0 } else { c != 0 };
            if vis {
                let p = pal[c];
                im.set(ox + (i & 7), oy + (i >> 3), [p[0], p[1], p[2], 255]);
            }
        }
    }
    im
}

/// One 8×8 8bpp tile → 64 palette indices; 4bpp → indices + bank*16 (0 stays 0).
pub fn tile_indices(data: &[u8], tid: usize, bpp: u32, bank: u8) -> [u8; 64] {
    let mut o = [0u8; 64];
    if bpp == 8 {
        if let Some(s) = data.get(tid * 64..tid * 64 + 64) {
            o.copy_from_slice(s);
        }
    } else if let Some(s) = data.get(tid * 32..tid * 32 + 32) {
        for i in 0..64 {
            let b = s[i >> 1];
            let c = if i & 1 == 1 { b >> 4 } else { b & 15 };
            o[i] = if c != 0 { c + bank * 16 } else { 0 };
        }
    }
    o
}
