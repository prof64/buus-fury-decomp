//! The game's proportional 1bpp fonts (renderer `0x0803F444`, string drawer `0x0803DADA`).
//!
//! Font record (12 bytes): `+0 u16* T`, `+4 u32 wide_w`, `+8 u16* wide_data`.
//! * code unit `c < 0xFF`: `g = T[c]`; `g == 0` → nothing drawn. Glyph = `T + 2g`: `u16 width`, then `width` u16
//!   **columns**, bit `k` of a column = pixel at row `k` (rows 0..15). Advance = width + 1.
//! * `c >= 0xFF`: fixed-width glyphs (`wide_w` columns at `wide_data + (c − 0x100)·wide_w·2`, valid for
//!   `c < 0x1D9D`); every font in this ROM has `wide_data = 0`, i.e. none (the measure code still advances
//!   `wide_w + 1`).
//! * The drawer puts a glyph's row 0 at `pen_y − 12`, i.e. `pen_y` is 12 px below the glyph cell top.
//! * Every set pixel is written with the surface's current colour index (surface byte +3), set by the control
//!   pair `0x0008, n`.

use crate::surface::Surface;
use wf_rom::Rom;

/// The fonts found in the ROM (all font records referenced by code / rodata).
pub const FONTS: &[(&str, u32, &str)] = &[
    ("small", 0x0806_E050, "small capitals (HUD / labels, the 'Select / Cancel' style captions)"),
    ("dialog", 0x0806_EE6C, "dialogue boxes (also referenced by the dialogue module globals 0x0807172C)"),
    ("digits", 0x0806_F150, "digits 0-9 only (counters)"),
    ("menu", 0x0807_012C, "menus (item / equipment / status lines, title screen menu)"),
    ("bold", 0x082B_9BA0, "bold, menu page titles ('Status', 'Inventory', …)"),
    ("timer", 0x0806_F4F4, "the countdown / stopwatch display's thick digits (0x0802FCA0, drawn outlined)"),
];

/// Glyph-cell top relative to the pen y passed to the drawer.
pub const ASCENT: i32 = 12;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub code: u16,
    pub width: u32,
    /// Column bitmaps (bit k = row k).
    pub cols: Vec<u16>,
    pub rom: u32,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub id: &'static str,
    pub rom: u32,
    pub table: u32,
    pub wide_w: u32,
    pub wide_data: u32,
    /// Indexed by code unit 0..255 (None = no glyph).
    pub glyphs: Vec<Option<Glyph>>,
}

impl Font {
    pub fn load(rom: &Rom, id: &'static str, addr: u32) -> Font {
        let table = rom.u32(addr);
        let wide_w = rom.u32(addr + 4);
        let wide_data = rom.u32(addr + 8);
        let mut glyphs = Vec::with_capacity(256);
        for c in 0..256u32 {
            let g = if c < 0xFF { rom.u16(table + 2 * c) as u32 } else { 0 };
            if g == 0 {
                glyphs.push(None);
                continue;
            }
            let a = table + 2 * g;
            let w = rom.u16(a) as u32;
            let cols = (0..w).map(|x| rom.u16(a + 2 + 2 * x)).collect();
            glyphs.push(Some(Glyph { code: c as u16, width: w, cols, rom: a }));
        }
        Font { id, rom: addr, table, wide_w, wide_data, glyphs }
    }

    pub fn all(rom: &Rom) -> Vec<Font> {
        FONTS.iter().map(|&(id, a, _)| Font::load(rom, id, a)).collect()
    }

    /// `T[T[c]]` — the width the measure routines (`0x0803FFE8` / `0x0804001A`) read; for a code unit without
    /// glyph this is `T[0]` (0 in every font).
    fn raw_width(&self, c: u16) -> u32 {
        if c >= 0xFF {
            return self.wide_w;
        }
        match &self.glyphs[c as usize] {
            Some(g) => g.width,
            None => 0,
        }
    }

    /// Advance of one code unit (width + 1), as the measure / layout code computes it.
    pub fn advance(&self, c: u16) -> u32 {
        self.raw_width(c) + 1
    }

    /// Width of a run of code units (`0x0804001A`): Σ (width + 1). Control codes are not skipped there; the
    /// line fitter `0x0803DE6A` skips the `0x0008, n` pair itself, as [`Font::measure_text`] does.
    pub fn measure_units(&self, s: &[u16]) -> u32 {
        s.iter().map(|&c| self.advance(c)).sum()
    }

    /// Width of displayable text: like the line fitter, `0x0008, n` colour pairs take no space.
    pub fn measure_text(&self, s: &[u16]) -> u32 {
        let mut w = 0;
        let mut i = 0;
        while i < s.len() {
            if s[i] == 8 {
                i += 2;
                continue;
            }
            w += self.advance(s[i]);
            i += 1;
        }
        w
    }

    /// Draw one line (no wrapping) with the pen at (x, y) — glyph rows start at `y − ASCENT`. Handles the
    /// colour pair `0x0008, n` like `0x0803DADA`. Returns the pen x after the line.
    pub fn draw_line(&self, s: &mut Surface, text: &[u16], x: i32, y: i32, color: &mut u8) -> i32 {
        let mut px = x;
        let top = y - ASCENT;
        let mut i = 0;
        while i < text.len() {
            let c = text[i];
            if c == 8 {
                if let Some(&n) = text.get(i + 1) {
                    *color = n as u8;
                }
                i += 2;
                continue;
            }
            if c < 0xFF {
                if let Some(g) = &self.glyphs[c as usize] {
                    for (cx, col) in g.cols.iter().enumerate() {
                        for row in 0..16 {
                            if col >> row & 1 == 1 {
                                s.set(px + cx as i32, top + row, *color);
                            }
                        }
                    }
                    px += g.width as i32 + 1;
                }
            } else {
                px += self.wide_w as i32 + 1;
            }
            i += 1;
        }
        px
    }
}

/// Word wrap as the dialogue line fitter `0x0803DE6A` does it: leading spaces / new lines are skipped, a line
/// ends at `\n` (consumed), `0x000B` or end; it breaks at the last space whose prefix fits `max_w` (a word longer
/// than the line is cut where it stops fitting). Returns the lines (without the break characters) and, per line,
/// its measured width.
pub fn wrap(font: &Font, text: &[u16], max_w: u32) -> Vec<(Vec<u16>, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < text.len() && matches!(text[i], 0x20 | 0x0A | 0x0B) {
            i += 1;
        }
        if i >= text.len() {
            break;
        }
        let start = i;
        let mut w = 0u32;
        let mut last_space: Option<(usize, u32)> = None;
        let mut end = text.len();
        let mut next = text.len();
        let mut j = i;
        while j < text.len() {
            let c = text[j];
            if c == 0x0A || c == 0x0B {
                end = j;
                next = j + 1;
                break;
            }
            if c == 8 {
                j += 2;
                continue;
            }
            if c == 0x20 {
                last_space = Some((j, w));
            }
            let nw = w + font.advance(c);
            if nw > max_w {
                match last_space {
                    Some((sp, sw)) => {
                        end = sp;
                        next = sp + 1;
                        w = sw;
                    }
                    None => {
                        end = j;
                        next = j;
                    }
                }
                break;
            }
            w = nw;
            j += 1;
        }
        if j >= text.len() && end == text.len() {
            next = text.len();
        }
        let line: Vec<u16> = text[start..end].to_vec();
        let lw = font.measure_text(&line);
        let _ = w;
        out.push((line, lw));
        i = next;
    }
    out
}

/// Metrics JSON + atlas for one font: 16×16 grid of 16×16 cells (cell = code unit), glyph drawn at the cell's
/// top-left in white on transparent.
pub fn atlas(font: &Font) -> (Surface, serde_json::Value) {
    use serde_json::json;
    let mut s = Surface::new(256, 256);
    let mut glyphs = serde_json::Map::new();
    let mut max_rows = 0;
    for g in font.glyphs.iter().flatten() {
        let (cx, cy) = ((g.code as i32 % 16) * 16, (g.code as i32 / 16) * 16);
        let mut rows = 0;
        for (x, col) in g.cols.iter().enumerate() {
            for r in 0..16 {
                if col >> r & 1 == 1 {
                    s.set(cx + x as i32, cy + r, 1);
                    rows = rows.max(r + 1);
                }
            }
        }
        max_rows = max_rows.max(rows);
        let ch = char::from_u32(g.code as u32).map(|c| c.to_string()).unwrap_or_default();
        glyphs.insert(
            format!("{}", g.code),
            json!({"char": ch, "x": cx, "y": cy, "w": g.width, "advance": g.width + 1, "rows": rows,
                   "rom": wf_rom::hx(g.rom), "cols": g.cols}),
        );
    }
    let desc = FONTS.iter().find(|f| f.0 == font.id).map(|f| f.2).unwrap_or("");
    let meta = json!({
        "id": font.id,
        "usage": desc,
        "atlas": format!("{}.png", font.id),
        "cell": [16, 16],
        "columns": 16,
        "encoding": "UTF-16 code unit = cell index (row = unit >> 4, column = unit & 15) for units < 0xFF; the game's text is plain UTF-16 (ASCII + Latin-1 + Œ/œ at 0x8C/0x9C as in Windows-1252 positions)",
        "glyph_rows": max_rows,
        "ascent_px": ASCENT,
        "advance": "glyph width + 1 px",
        "draw": "pen (x, y): glyph cell top-left = (x, y − 12); pixels take the current colour index",
        "control_codes": {
            "0x0000": "end of string",
            "0x000A": "new line (line advance = the text box's line height, e.g. 15 px in dialogue boxes)",
            "0x000B": "end of box / page break (dialogue paging: the line fitter stops)",
            "0x0008": "followed by one code unit n: set the colour index to n (UI palette 0x0805652C); 2 = normal white, 3/5/6/7/0x1D highlight colours used by menus",
            "0x0020": "space, the only word-wrap break",
            "%s / %d": "printf slots, filled before drawing (menu strings)"
        },
        "wide_glyphs": {"width": font.wide_w, "data": wf_rom::hx(font.wide_data),
                        "note": "code units >= 0xFF; no font in this ROM has wide glyph data"},
        "table": wf_rom::hx(font.table),
        "rom": wf_rom::hx(font.rom),
        "glyphs": glyphs,
    });
    (s, meta)
}
