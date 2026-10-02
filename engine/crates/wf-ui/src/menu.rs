//! Pause-menu pages (Status / Inventory / Equip / Options / Journal) are drawn by code into a 240×160 8bpp
//! surface shown as BG2 (identity tilemap, tiles 1..600, palette `0x0805652C`). The two procedural primitives
//! every page uses are reproduced here from their code:
//!
//! * [`background`] — `0x08004F1C(surface, x, y, w, rows)`: rows alternate colour 0x01 / 0x30 (memset with
//!   0x01010101, XOR 0x31313131 per row), starting with 0x01.
//! * [`panel`] — `0x08004F50(surface, rect)`: the yellow bevelled panel with a 3-px drop shadow (colours 0x1D,
//!   0x6E, 0x3F outer → inner, 0x01 dark, 0x30 inner line), transcribed instruction by instruction.
//!
//! Page layout constants live in each page's draw code (e.g. Status `0x08006718`: background (0, 0, 240, 160),
//! panel (8, 24)–(232, 152), title in the bold font centred at x 120, y 16, colour 5). See `engine/docs/ui.md`.

use crate::surface::Surface;

/// `0x08004F1C`.
pub fn background(s: &mut Surface, x: i32, y: i32, w: i32, rows: i32) {
    let mut c = 0x01u8;
    for r in 0..rows {
        s.hline(x, y + r, w, c);
        c ^= 0x31;
    }
}

/// `0x08004F50`: rect = (x0, y0, x1, y1).
pub fn panel(s: &mut Surface, x0: i32, y0: i32, x1: i32, y1: i32) {
    let w = x1 - x0;
    let h = y1 - y0;
    // side columns, rows 0..2 (left 1D 6E 3F 01 01 30, right at w-5: 30 01 1D 6E 3F)
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
            // drop shadow right of the panel
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

/// The common page skeleton: background over the whole surface and the main panel.
pub fn page_skeleton() -> Surface {
    let mut s = Surface::new(240, 160);
    background(&mut s, 0, 0, 240, 160);
    panel(&mut s, 8, 24, 232, 152);
    s
}
