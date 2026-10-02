//! BG layer decoding and rendering to palette indices (index 0 = transparent / backdrop).

use crate::rom_maps::{Header, Resource, RES_TILEMAP};
use crate::Ctx;

/// A decoded layer: `w x h` text-mode screen entries (tile 0..9, hflip 10, vflip 11, palbank 12..15).
pub struct Layer {
    pub w: u32,
    pub h: u32,
    pub entries: Vec<u16>,
}

/// Chunks are 32x32 entries (0x800-byte webfoot-LZ blobs), laid out row-major `cols x rows`.
pub fn decode_layer(ctx: &mut Ctx, r: &Resource) -> Layer {
    let (cols, rows) = (r.cols as u32, r.rows as u32);
    let (w, h) = (cols * 32, rows * 32);
    let mut entries = vec![0u16; (w * h) as usize];
    for cy in 0..rows {
        for cx in 0..cols {
            let p = r.chunks[(cy * cols + cx) as usize];
            if !ctx.rom.isptr(p) {
                continue;
            }
            let d = ctx.blob(p);
            for ty in 0..32u32 {
                for tx in 0..32u32 {
                    let o = ((ty * 32 + tx) * 2) as usize;
                    if let Some(b) = d.get(o..o + 2) {
                        entries[((cy * 32 + ty) * w + cx * 32 + tx) as usize] = u16::from_le_bytes([b[0], b[1]]);
                    }
                }
            }
        }
    }
    Layer { w, h, entries }
}

/// Render an 8bpp layer with the VRAM tiles; tiles past the loaded data are blank.
pub fn render_layer(l: &Layer, tiles: &[u8]) -> Vec<u8> {
    let (pw, ph) = (l.w * 8, l.h * 8);
    let mut out = vec![0u8; (pw * ph) as usize];
    for ty in 0..l.h {
        for tx in 0..l.w {
            let se = l.entries[(ty * l.w + tx) as usize];
            let t = (se & 0x3FF) as usize * 64;
            let Some(td) = tiles.get(t..t + 64) else { continue };
            let (hf, vf) = (se & 0x400 != 0, se & 0x800 != 0);
            for y in 0..8u32 {
                let sy = if vf { 7 - y } else { y };
                let row = ((ty * 8 + y) * pw + tx * 8) as usize;
                for x in 0..8u32 {
                    let sx = if hf { 7 - x } else { x };
                    out[row + x as usize] = td[(sy * 8 + sx) as usize];
                }
            }
        }
    }
    out
}

/// Layer position in world px at camera (0, 0): the game scrolls a layer to `(camera * rate + offset) >> 10`
/// (0x08007E84), so layer pixel `p` is drawn at world `p - (offset >> 10)` when the camera is at the origin.
pub fn layer_origin(r: &Resource) -> (i32, i32) {
    (-(r.offset.0 >> 10), -(r.offset.1 >> 10))
}

/// Composite of the tilemap layers over the backdrop (palette index 0), world-aligned, `w x h` px:
/// GBA order (larger priority value further back; equal priority: larger BG index further back).
/// Wrapping auto-scroll layers (RES_WRAP) are not part of the composite.
pub fn composite(layers: &[(usize, Resource, Layer, Vec<u8>)], h: &Header, w: u32, hgt: u32) -> Vec<u8> {
    composite_rect(layers, h, 0, 0, w, hgt)
}

/// [`composite`] over the world rectangle (x0, y0) .. (x0 + w, y0 + hgt).
pub fn composite_rect(layers: &[(usize, Resource, Layer, Vec<u8>)], h: &Header, x0: i32, y0: i32, w: u32, hgt: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * hgt) as usize];
    let mut order: Vec<&(usize, Resource, Layer, Vec<u8>)> = layers.iter().filter(|l| l.1.kind == RES_TILEMAP).collect();
    order.sort_by_key(|l| (std::cmp::Reverse(h.priority[l.0]), std::cmp::Reverse(l.0)));
    for (_, r, l, idx) in order {
        let (ox, oy) = layer_origin(r);
        let (pw, ph) = (l.w as i32 * 8, l.h as i32 * 8);
        for y in 0..ph {
            let wy = y + oy - y0;
            if wy < 0 || wy >= hgt as i32 {
                continue;
            }
            for x in 0..pw {
                let wx = x + ox - x0;
                if wx < 0 || wx >= w as i32 {
                    continue;
                }
                let c = idx[(y * pw + x) as usize];
                if c != 0 {
                    out[(wy as u32 * w + wx as u32) as usize] = c;
                }
            }
        }
    }
    out
}
