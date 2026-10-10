//! The area-name banner (`ui/banner.json`; object ctor `0x0800BF34`, vt `0x0804B528`): a 240x16 8bpp strip
//! (background picture `ui/images/banner.png`) with the area name centred in the `small` font (shadow colour 1 at
//! (+1, +1), then colour 2, per glyph), shown as 8 affine OBJs at y 128, priority 0, for 239 frames; while the
//! object's timer is <= 16 the OBJs' matrix d = 0x1000 / timer collapses the strip onto its centre line.

use wf_content::image::{decode_indexed, Indexed};
use wf_content::Packs;

use crate::ui::{units, Font};

pub const Y: i32 = 128;
pub const W: usize = 240;
pub const H: usize = 16;
pub const PRIORITY: u8 = 0;
/// Visible frames (timer 240, decremented before every draw, removed at 0).
pub const FRAMES: u32 = 239;

pub struct Banner {
    bg: Indexed,
    font: Font,
    cache: Option<(String, Vec<u8>)>,
}

impl Banner {
    pub fn load(packs: &Packs) -> Option<Banner> {
        let bg = decode_indexed(&packs.read("ui/images/banner.png")?).ok()?;
        let font = Font::load(packs, "small").ok()?;
        Some(Banner { bg, font, cache: None })
    }

    /// The strip's palette indices for `text` (`0x0800BE90`: string drawer `0x0803DADA` mode 1 at (120, 11)).
    fn surface(&mut self, text: &str) -> &[u8] {
        if self.cache.as_ref().is_none_or(|(t, _)| t != text) {
            let mut idx = self.bg.idx.clone();
            idx.resize(W * H, 0);
            let u = units(text);
            let w = self.font.measure(&u) as i32;
            let mut pen = 120 - (w + 1) / 2;
            let top = 11 - self.font.ascent;
            for &c in &u {
                if let Some(Some(g)) = self.font.glyphs.get(c as usize) {
                    for (dx, dy, col) in [(1, 1, 1u8), (0, 0, 2u8)] {
                        for &(gx, gy) in &g.px {
                            let (x, y) = (pen + gx as i32 + dx, top + gy as i32 + dy);
                            if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
                                idx[y as usize * W + x as usize] = col;
                            }
                        }
                    }
                    pen += g.w as i32 + 1;
                } else {
                    pen += 1;
                }
            }
            self.cache = Some((text.to_string(), idx));
        }
        &self.cache.as_ref().unwrap().1
    }

    /// The banner as it appears `t` frames after its first draw: ARGB 240x16 (alpha 0 = transparent), or None
    /// once it is gone.
    pub fn frame(&mut self, text: &str, t: u32) -> Option<Vec<u32>> {
        if t >= FRAMES {
            return None;
        }
        let timer = (FRAMES - t) as i32;
        let pd = if timer > 16 { 0x100 } else { 0x1000 / timer };
        let pal = self.bg.palette.clone();
        let src = self.surface(text).to_vec();
        let mut out = vec![0u32; W * H];
        // every OBJ (32x16, the last 16x16) is 16 px high: rows map around its centre line y 8
        for y in 0..H as i32 {
            let ty = ((pd * (y - 8)) >> 8) + 8;
            if !(0..H as i32).contains(&ty) {
                continue;
            }
            for x in 0..W {
                let c = src[ty as usize * W + x];
                if c != 0 {
                    // top byte = OBJ palette index (the strip's colours are OBJ palette entries)
                    out[y as usize * W + x] = (pal.get(c as usize).copied().unwrap_or(0) & 0xFF_FFFF) | (c as u32) << 24;
                }
            }
        }
        Some(out)
    }
}
