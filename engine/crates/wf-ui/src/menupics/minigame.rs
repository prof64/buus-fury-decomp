//! Mini-games, credits, map screen.
//!
//! The gauge of the power struggle (`0x0803018C`) and of the punch game (`0x080309F4`), both drawn by
//! `0x08030274`: a 96x8 bar (3 OBJs 32x8, 8bpp, template `0x08757BE8`, 0x300 bytes in tile order) whose steps
//! k >= value get colour 1 at the 4 bytes of table `0x08757EE8` (8 bytes per step, 85 steps); the peak marker is an
//! 8x16 OBJ (`0x08757B68`, 0x80 bytes). Exported as `ui/menu/mg_bar.png` (frame v = value v, 0..=85, stacked
//! vertically) and `ui/menu/mg_marker.png`.

use super::MenuPic;
use crate::surface::Surface;
use wf_rom::content::Content;
use wf_rom::gfx::Rgb;
use wf_rom::Rom;

pub const PICS: &[MenuPic] = &[];

pub const BAR_TEMPLATE: u32 = 0x0875_7BE8;
pub const BAR_STEPS: u32 = 0x0875_7EE8;
pub const BAR_MARKER: u32 = 0x0875_7B68;
pub const BAR_MAX: usize = 85;

/// byte offset in the 3 x (4 tiles of 64 bytes) buffer -> (x, y) in the 96x8 bar
fn tile_px(off: usize) -> (usize, usize) {
    let o = off / 0x100;
    let t = (off % 0x100) / 64;
    (o * 32 + t * 8 + off % 8, (off % 64) / 8)
}

pub fn extract(rom: &Rom, out: &mut Content, pal: &[Rgb]) {
    let tpl: Vec<u8> = (0..0x300u32).map(|i| rom.u8(BAR_TEMPLATE + i)).collect();
    let mut base = vec![0u8; 96 * 8];
    for (off, &v) in tpl.iter().enumerate() {
        let (x, y) = tile_px(off);
        base[y * 96 + x] = v;
    }
    let mut all = Vec::with_capacity(96 * 8 * (BAR_MAX + 1));
    for value in 0..=BAR_MAX {
        let mut f = base.clone();
        for k in value..BAR_MAX {
            for j in 0..4u32 {
                let off = rom.u16(BAR_STEPS + 8 * k as u32 + 2 * j) as usize;
                if off < 0x300 {
                    let (x, y) = tile_px(off);
                    f[y * 96 + x] = 1;
                }
            }
        }
        all.extend_from_slice(&f);
    }
    out.put("ui/menu/mg_bar.png", Surface::from_vec(96, (8 * (BAR_MAX + 1)) as u32, all).png(pal, true));
    let mk: Vec<u8> = (0..0x80u32).map(|i| rom.u8(BAR_MARKER + i)).collect();
    out.put("ui/menu/mg_marker.png", Surface::from_vec(8, 16, mk).png(pal, true));
}
