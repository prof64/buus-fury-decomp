//! The game's text layer as menus use it: font metrics (`ui/fonts/*.json`), the string table (`text/strings.json`,
//! `0x0806BCE8`), the printf (`0x0803DB7C`), the measure routines (`0x0803FFE8` / `0x0804001A`) and the string
//! drawer (`0x0803DADA`) with its alignment modes. Drawing produces [`wf_view::MenuOp`]s; the renderer rasterises
//! the glyphs (`0x0803F444` / `0x0803F5A4`).
//!
//! Quirks kept: measuring counts every code unit without a glyph (colour codes `0x0008 n` included) as 1 px, while
//! drawing skips them, so right / centre aligned strings with colour codes sit 2 px per colour change further left.

use crate::content::ContentSource;
use serde_json::Value;
use wf_view::{MenuFont, MenuOp};

/// glyph widths of one font (units < 0xFF; 0 = no glyph) and the wide width (units >= 0xFF)
#[derive(Clone, Debug, Default)]
pub struct FontMetrics {
    pub w: Vec<u8>,
    pub has: Vec<bool>,
    pub wide_w: u32,
    /// the glyphs' column bitmaps (`cols` of the font file: one u16 per pixel column, bit 0 = top row); empty when
    /// the content lacks them (only the redraw timing model reads them, `sndsim.rs`)
    pub cols: Vec<Vec<u16>>,
}

impl FontMetrics {
    /// `0x0804001A(font, s, n)`: width of the first n units, every unit `width + 1` (no glyph: table entry 0 = 0)
    pub fn measure_n(&self, s: &[u16], n: usize) -> i32 {
        s.iter().take(n).map(|&c| self.unit(c)).sum()
    }
    /// `0x0803FFE8(font, s)`: width up to the terminator
    pub fn measure(&self, s: &[u16]) -> i32 {
        s.iter().take_while(|&&c| c != 0).map(|&c| self.unit(c)).sum()
    }
    fn unit(&self, c: u16) -> i32 {
        if c >= 0xFF {
            self.wide_w as i32 + 1
        } else {
            self.w.get(c as usize).copied().unwrap_or(0) as i32 + 1
        }
    }
    /// the glyph routine's advance (no glyph: nothing drawn, no advance)
    pub fn advance(&self, c: u16) -> i32 {
        if c < 0xFF && self.has.get(c as usize).copied().unwrap_or(false) {
            self.w[c as usize] as i32 + 1
        } else {
            0
        }
    }
}

/// menu text content: the five fonts' metrics and the string table
#[derive(Clone, Debug, Default)]
pub struct TextData {
    pub fonts: [FontMetrics; 5],
    pub strings: Vec<Vec<u16>>,
}

fn font_index(f: MenuFont) -> usize {
    match f {
        MenuFont::Small => 0,
        MenuFont::Dialog => 1,
        MenuFont::Digits => 2,
        MenuFont::Menu => 3,
        MenuFont::Bold => 4,
        // (the timer display's digits are drawn by the renderer only: no menu measures them)
        MenuFont::Timer => 2,
        // (the title text too)
        MenuFont::Title => 1,
    }
}

pub fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// a `text/strings.json` entry: UTF-16 with control units written as `{XXXX}` (hex)
pub fn decode_escaped(s: &str) -> Vec<u16> {
    let u: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::with_capacity(u.len());
    let mut i = 0;
    while i < u.len() {
        if u[i] == b'{' as u16 && i + 5 < u.len() && u[i + 5] == b'}' as u16 {
            let hex: String = String::from_utf16_lossy(&u[i + 1..i + 5]);
            if let Ok(v) = u16::from_str_radix(&hex, 16) {
                out.push(v);
                i += 6;
                continue;
            }
        }
        out.push(u[i]);
        i += 1;
    }
    out
}

impl TextData {
    pub fn load(src: &dyn ContentSource) -> Option<TextData> {
        let mut t = TextData::default();
        for f in [MenuFont::Small, MenuFont::Dialog, MenuFont::Digits, MenuFont::Menu, MenuFont::Bold] {
            let b = src.read(&format!("ui/fonts/{}.json", f.id()))?;
            let v: Value = serde_json::from_slice(&b).ok()?;
            let mut m = FontMetrics { w: vec![0; 256], has: vec![false; 256], wide_w: v["wide_glyphs"]["width"].as_u64().unwrap_or(0) as u32, cols: vec![Vec::new(); 256] };
            if let Some(g) = v["glyphs"].as_object() {
                for (k, gv) in g {
                    if let Ok(c) = k.parse::<usize>() {
                        if c < 256 {
                            m.w[c] = gv["w"].as_u64().unwrap_or(0) as u8;
                            m.has[c] = true;
                            m.cols[c] = gv["cols"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u16).collect()).unwrap_or_default();
                        }
                    }
                }
            }
            t.fonts[font_index(f)] = m;
        }
        if let Some(b) = src.read("text/strings.json") {
            if let Ok(Value::Array(a)) = serde_json::from_slice::<Value>(&b) {
                t.strings = a.iter().map(|s| decode_escaped(s.as_str().unwrap_or(""))).collect();
            }
        }
        Some(t)
    }
    pub fn font(&self, f: MenuFont) -> &FontMetrics {
        &self.fonts[font_index(f)]
    }
    /// string table entry (`0x0806BCE8[i]`), empty when missing
    pub fn s(&self, i: usize) -> &[u16] {
        self.strings.get(i).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// printf argument
#[derive(Clone, Debug)]
pub enum Arg<'a> {
    I(i32),
    S(&'a [u16]),
}

/// `0x0803DB7C` (vsprintf): `%[0][width](d|s|S)`; `%d` signed decimal padded to the width with spaces or zeros
/// (the sign counts toward the width and comes after the padding), `%s` a UTF-16 string, `%S` a byte string.
pub fn sprintf(fmt: &[u16], args: &[Arg]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut ai = 0;
    let mut next = || {
        let a = args.get(ai).cloned();
        ai += 1;
        a
    };
    while i < fmt.len() && fmt[i] != 0 {
        let c = fmt[i];
        if c != b'%' as u16 {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let mut pad = b' ' as u16;
        let mut width = 0i32;
        if fmt.get(i) == Some(&(b'0' as u16)) {
            pad = b'0' as u16;
            i += 1;
        }
        while let Some(&d) = fmt.get(i) {
            if (b'0' as u16..=b'9' as u16).contains(&d) {
                width = width * 10 + (d - b'0' as u16) as i32;
                i += 1;
            } else {
                break;
            }
        }
        let Some(&k) = fmt.get(i) else { break };
        match k {
            0 => break,
            0x64 => {
                let v = match next() {
                    Some(Arg::I(v)) => v,
                    _ => 0,
                };
                let neg = v < 0;
                let mut u = v.unsigned_abs();
                let mut digits = Vec::new();
                loop {
                    digits.push(b'0' as u16 + (u % 10) as u16);
                    u /= 10;
                    if u == 0 {
                        break;
                    }
                }
                if neg {
                    digits.push(b'-' as u16);
                }
                for _ in 0..(width - digits.len() as i32).max(0) {
                    out.push(pad);
                }
                out.extend(digits.iter().rev());
            }
            0x73 | 0x53 => {
                if let Some(Arg::S(s)) = next() {
                    out.extend(s.iter().take_while(|&&c| c != 0));
                }
            }
            // any other conversion character is dropped
            _ => {}
        }
        i += 1;
    }
    out
}

/// The draw context the game passes around (stack struct of 0x34 bytes): +0 flags (bits 0-1 align: 0 left,
/// 1 centre, 2 right; bit 2 single line), +1 line height, +2 shadow colour, +3 colour, +4 x, +8 y, +0xC font,
/// +0x20 glyph routine (`0x0803F5A4` = with shadow).
#[derive(Clone, Debug)]
pub struct Pen {
    pub align: u8,
    pub single: bool,
    pub line_h: i32,
    pub shadow: Option<u8>,
    pub colour: u8,
    pub x: i32,
    pub y: i32,
    pub font: MenuFont,
    pub clip: [i32; 4],
}

impl Default for Pen {
    fn default() -> Pen {
        Pen { align: 0, single: false, line_h: 0, shadow: None, colour: 0, x: 0, y: 0, font: MenuFont::Menu, clip: [0, 0, 240, 160] }
    }
}

impl Pen {
    pub fn new(font: MenuFont, x: i32, y: i32, align: u8, colour: u8, shadow: Option<u8>) -> Pen {
        Pen { align, colour, x, y, font, shadow, ..Pen::default() }
    }
    pub fn at(mut self, x: i32, y: i32) -> Pen {
        self.x = x;
        self.y = y;
        self
    }
}

/// a display list being built (one surface)
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    pub ops: Vec<MenuOp>,
}

impl Canvas {
    pub fn new() -> Canvas {
        Canvas::default()
    }
    /// `0x08004F1C`
    pub fn background(&mut self, x: i32, y: i32, w: i32, rows: i32) {
        self.ops.push(MenuOp::Background { x, y, w, rows });
    }
    /// `0x08004F50`
    pub fn panel(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.ops.push(MenuOp::Panel { x0, y0, x1, y1 });
    }
    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, colour: u8) {
        self.ops.push(MenuOp::Fill { x0, y0, x1, y1, colour });
    }
    pub fn pixel(&mut self, x: i32, y: i32, colour: u8) {
        self.ops.push(MenuOp::Pixel { x, y, colour });
    }
    pub fn image(&mut self, path: &str, frame: u16, fw: u16, fh: u16, x: i32, y: i32) {
        self.ops.push(MenuOp::Image { path: path.to_string(), frame, fw, fh, x, y, key0: true });
    }
    /// `0x0803DADA(pen, s)`: line by line (0x000A breaks unless single-line); each line aligned with the measure of
    /// its units (control units 1 px); in left mode the pen x advances by the width when the string ends on its
    /// last line; colour codes change `pen.colour` for good; the pen y moves by the line height per line break.
    pub fn text(&mut self, t: &TextData, pen: &mut Pen, s: &[u16]) {
        let fm = t.font(pen.font);
        let mut start = 0usize;
        loop {
            let mut end = start;
            while end < s.len() && s[end] != 0 && s[end] != 0x0A {
                end += 1;
            }
            let line = &s[start..end];
            let n = line.len();
            if n > 0 {
                let w = fm.measure_n(line, n);
                let at_end = end >= s.len() || s[end] == 0;
                let x = match pen.align & 3 {
                    0 => {
                        let x = pen.x;
                        if at_end {
                            pen.x += w;
                        }
                        x
                    }
                    1 => pen.x - ((w + 1) >> 1),
                    2 => pen.x - w,
                    _ => pen.x,
                };
                // the line as drawn: colour codes are kept for the renderer, the pen's colour follows them
                self.ops.push(MenuOp::Text { font: pen.font, x, y: pen.y, colour: pen.colour, shadow: pen.shadow, clip: pen.clip, text: line.to_vec() });
                let mut k = 0;
                while k < n {
                    if line[k] == 8 && k + 1 < n {
                        pen.colour = line[k + 1] as u8;
                        k += 2;
                    } else {
                        k += 1;
                    }
                }
            }
            if pen.single || end >= s.len() || s[end] == 0 {
                break;
            }
            pen.y += pen.line_h;
            start = end + 1;
        }
    }
    /// printf + draw (`0x0803DC96`)
    pub fn textf(&mut self, t: &TextData, pen: &mut Pen, fmt: &[u16], args: &[Arg]) {
        let s = sprintf(fmt, args);
        self.text(t, pen, &s);
    }
    /// `0x0803DD66(pen, max_w, s)`: word-wrapped text: per line (leading spaces skipped) the units up to the
    /// end, a new line (kept in the line), or the last space before the width passes `max_w` (colour codes take
    /// no width here); lines aligned like `draw_string`, drawn only while the glyph top is inside the clip
    /// (y > clip y0 - 16), the pen y moving by the line height every line.
    pub fn text_wrapped(&mut self, t: &TextData, pen: &mut Pen, max_w: i32, s: &[u16]) {
        let fm = t.font(pen.font);
        let at = |k: usize| s.get(k).copied().unwrap_or(0);
        let mut pos = 0usize;
        let mut y0 = pen.y - 12;
        while y0 < pen.clip[3] {
            while at(pos) == 0x20 {
                pos += 1;
            }
            if at(pos) == 0 {
                break;
            }
            let mut end = pos;
            let mut acc = 0;
            let mut sp: Option<usize> = None;
            loop {
                let c = at(end);
                if c == 0 {
                    break;
                }
                if c == 0x0A {
                    end += 1;
                    break;
                }
                if c == 8 {
                    end += 2;
                    continue;
                }
                if c == 0x20 {
                    sp = Some(end);
                }
                acc += fm.measure_n(&[c], 1);
                if acc > max_w {
                    if let Some(p) = sp {
                        end = p;
                    }
                    break;
                }
                end += 1;
            }
            let end = end.min(s.len());
            let line = &s[pos..end];
            if !line.is_empty() && y0 > pen.clip[1] - 16 {
                let w = fm.measure_n(line, line.len());
                let x = match pen.align & 3 {
                    1 => pen.x - ((w + 1) >> 1),
                    2 => pen.x - w,
                    _ => pen.x,
                };
                self.ops.push(MenuOp::Text { font: pen.font, x, y: pen.y, colour: pen.colour, shadow: pen.shadow, clip: pen.clip, text: line.to_vec() });
                let mut k = 0;
                while k < line.len() {
                    if line[k] == 8 && k + 1 < line.len() {
                        pen.colour = line[k + 1] as u8;
                        k += 2;
                    } else {
                        k += 1;
                    }
                }
            }
            y0 += pen.line_h;
            pen.y += pen.line_h;
            pos = end;
        }
    }
}

/// `0x0803DE6A` the line fitter from unit `start`: leading spaces / line breaks / 0x0B skipped, then units up to
/// the end (done), a new line, 0x0B (done), or the last space before the width passes `max_w` (a longer word is
/// cut where it stops fitting). Returns (line start, line end, width, done).
pub fn line_fit(fm: &FontMetrics, s: &[u16], start: usize, max_w: i32) -> (usize, usize, i32, bool) {
    let at = |k: usize| s.get(k).copied().unwrap_or(0);
    let mut p = start;
    while matches!(at(p), 0x20 | 0x0A | 0x0B) {
        p += 1;
    }
    let first = p;
    let mut w = 0;
    let mut space: Option<(usize, i32)> = None;
    loop {
        let c = at(p);
        match c {
            0 | 0x0B => return (first, p, w, true),
            0x0A => return (first, p, w, false),
            8 => {
                p += 2;
                continue;
            }
            0x20 => space = Some((p, w)),
            _ => {}
        }
        let nw = w + fm.measure_n(&[c], 1);
        if nw > max_w {
            return match space {
                Some((sp, sw)) => (first, sp, sw, false),
                None => (first, p, w, false),
            };
        }
        p += 1;
        w = nw;
    }
}

/// the number of `line_fit` calls until it reports the end (how the Journal centres its entries)
pub fn count_lines(fm: &FontMetrics, s: &[u16], max_w: i32) -> i32 {
    let mut n = 0;
    let mut p = 0;
    loop {
        let (_, e, _, done) = line_fit(fm, s, p, max_w);
        n += 1;
        if done || n > 64 {
            return n;
        }
        p = e;
    }
}
