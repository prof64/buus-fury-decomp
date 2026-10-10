//! An 8bpp software surface — what the game draws every menu, dialogue box and the HUD into before copying it to
//! VRAM (surface object: `+0x18/+0x1C` w/h, `+0x24` pixels, `+0x28` pitch).

use wf_rom::gfx::{encode_png_indexed, Rgb};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Surface {
    pub fn new(w: u32, h: u32) -> Surface {
        Surface { w, h, px: vec![0; (w * h) as usize] }
    }
    pub fn from_vec(w: u32, h: u32, px: Vec<u8>) -> Surface {
        assert_eq!(px.len(), (w * h) as usize);
        Surface { w, h, px }
    }
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = c;
        }
    }
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.px[(y * self.w + x) as usize]
    }
    /// memset of a horizontal run (clipped).
    pub fn hline(&mut self, x: i32, y: i32, n: i32, c: u8) {
        for i in 0..n.max(0) {
            self.set(x + i, y, c);
        }
    }
    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        for y in y0..y1 {
            self.hline(x0, y, x1 - x0, c);
        }
    }
    /// Copy `src` at (dx, dy); index 0 is transparent when `key0`.
    pub fn blit(&mut self, src: &Surface, dx: i32, dy: i32, key0: bool) {
        for y in 0..src.h {
            for x in 0..src.w {
                let c = src.get(x, y);
                if !key0 || c != 0 {
                    self.set(dx + x as i32, dy + y as i32, c);
                }
            }
        }
    }
    /// Indexed PNG with the given palette.
    pub fn png(&self, pal: &[Rgb], index0_transparent: bool) -> Vec<u8> {
        encode_png_indexed(self.w, self.h, &self.px, pal, index0_transparent)
    }
    /// Linear image from GBA 8bpp tiles laid out `tw` tiles per row (1D order).
    pub fn from_tiles(data: &[u8], tw: u32, th: u32) -> Surface {
        let mut s = Surface::new(tw * 8, th * 8);
        for t in 0..(tw * th) {
            let base = (t * 64) as usize;
            if base + 64 > data.len() {
                break;
            }
            for i in 0..64u32 {
                s.set(((t % tw) * 8 + (i & 7)) as i32, ((t / tw) * 8 + (i >> 3)) as i32, data[base + i as usize]);
            }
        }
        s
    }
}
