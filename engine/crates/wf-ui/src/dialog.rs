//! Dialogue boxes (module `0x0800AD00..0x0800B300`, globals `0x08071700`).
//!
//! * Box styles: 2 × 20-byte records at `0x08071730`: `u32 max_lines, u32 wrap_width, u32 x, u32 y, ptr image`;
//!   image = webfoot-LZ 160×64 8bpp (OBJ palette = UI palette). The box surface (160×64, `dialog obj + 0x80`)
//!   starts as a copy of the image; text is drawn with the dialogue font (`0x0806EE6C`) in colour 2, lines 14 px
//!   apart (`0x0800AF9C`; the surface byte +1 = 0x0F at `0x0800B1F0` is not the line step).
//! * Layout (`0x0800AEF6..0x0800B020`): the page's lines are found by the line fitter `0x0803DE6A` (wrap at
//!   `wrap_width`); centred mode (the default for story text): each line is centred, `x = style.x +
//!   (wrap_width − line_width) / 2`, and the block is centred vertically,
//!   `y = style.y + 7 · (max_lines − lines)`, then `y += 14` per line (pen y; glyph tops at `y − 12`).
//! * The surface is shown as three 8bpp OBJs: 64×64 + 64×64 + 32×64 at x 40 / 104 / 168 (tiles from
//!   `dialog obj + 0x78..`), box y from the text's position code (`!` top / `@` middle / `#` bottom).

use crate::font::{wrap, Font};
use crate::surface::Surface;
use serde_json::{json, Value};
use wf_rom::Rom;

pub const STYLES: u32 = 0x0807_1730;
/// pen y advance per line: +0xE (`0x0800AF9C`; VERIFIED on 2..4-line intro / talk pages, `wf-app` test
/// `dialog_portrait_parity`). The surface's font height byte (+1 = 0x0F, `0x0800B1F0`) is not the line step.
pub const LINE_HEIGHT: i32 = 14;
pub const TEXT_COLOR: u8 = 2;

#[derive(Clone, Debug)]
pub struct Style {
    pub index: u32,
    pub max_lines: u32,
    pub wrap_width: u32,
    pub x: i32,
    pub y: i32,
    pub image: u32,
}

pub fn styles(rom: &Rom) -> Vec<Style> {
    (0..2)
        .map(|i| {
            let a = STYLES + 20 * i;
            Style {
                index: i,
                max_lines: rom.u32(a),
                wrap_width: rom.u32(a + 4),
                x: rom.u32(a + 8) as i32,
                y: rom.u32(a + 12) as i32,
                image: rom.u32(a + 16),
            }
        })
        .collect()
}

pub fn box_image(rom: &Rom, st: &Style) -> Surface {
    let d = rom.lz(st.image).unwrap_or_else(|_| vec![0; 160 * 64]);
    Surface::from_vec(160, 64, d)
}

/// Split text into pages of at most `max_lines` lines (0x000B also ends a page).
pub fn pages(font: &Font, st: &Style, text: &[u16]) -> Vec<Vec<(Vec<u16>, u32)>> {
    let mut out = Vec::new();
    for chunk in text.split(|&c| c == 0x0B) {
        let lines = wrap(font, chunk, st.wrap_width);
        for p in lines.chunks(st.max_lines.max(1) as usize) {
            out.push(p.to_vec());
        }
    }
    out
}

/// Render one page into a fresh box surface, centred like the game's story text.
pub fn render_page(rom: &Rom, font: &Font, st: &Style, page: &[(Vec<u16>, u32)]) -> Surface {
    let mut s = box_image(rom, st);
    let n = page.len() as i32;
    let mut y = st.y + 7 * (st.max_lines as i32 - n);
    let mut color = TEXT_COLOR;
    for (line, w) in page {
        let x = st.x + (st.wrap_width as i32 - *w as i32) / 2;
        font.draw_line(&mut s, line, x, y, &mut color);
        y += LINE_HEIGHT;
    }
    s
}

pub fn json(styles: &[Style]) -> Value {
    json!({
        "font": "fonts/dialog.json",
        "text_color": TEXT_COLOR,
        "line_height_px": LINE_HEIGHT,
        "styles": styles.iter().map(|s| json!({
            "index": s.index, "max_lines": s.max_lines, "wrap_width_px": s.wrap_width, "text_x": s.x,
            "text_y": s.y, "image": format!("images/dialog_box_{}.png", s.index), "size": [160, 64],
            "rom": wf_rom::hx(STYLES + 20 * s.index), "image_rom": wf_rom::hx(s.image),
        })).collect::<Vec<_>>(),
        "layout": "lines from the word wrapper (break at the last space that fits wrap_width); each line centred: x = text_x + (wrap_width - line_width) / 2; block centred vertically: first pen y = text_y + 7 * (max_lines - lines), then +14 per line; glyph tops at pen y - 12. With a portrait (node arg 1..6 / >= 8): not centred, lines at (text_x, text_y), +14 per line; a '- ' option line at x + 20, wrap - 20; a leading '^' keeps the centred layout",
        "screen": {"objs": [[40, 64], [104, 64], [168, 32]], "obj_height": 64, "x": 40,
                    "y_by_position_code": {"!": "top", "@": "middle", "#": "bottom (default)"},
                    "note": "the 160x64 surface is uploaded to OBJ tiles each frame (0x0800ADA0) and shown as 64x64 + 64x64 + 32x64 8bpp OBJs, alpha-blended"},
        "code": {"module_globals": "0x08071700", "layout": "0x0800AEF6", "line_fitter": "0x0803DE6A", "upload": "0x0800ADA0"},
    })
}
