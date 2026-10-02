//! Dialogue text.
//!
//! * **Text banks**: `u32[7]` at `0x087B5B64` → webfoot-LZ containers of UTF-16LE text (≈ 64 KB each), all lines
//!   concatenated without terminators. The dialogue box loader (`0x08012230` / `0x080122B4`) inflates the bank
//!   into a 64 KB heap buffer and copies one line out.
//! * **Text reference** (`u32`, event node kinds 5 and 6): `bank = v & 0x3F`, `offset = (v >> 6) & 0x7FFF`
//!   (UTF-16 code units), `length = v >> 21` (code units).
//! * **Inline text** (node kinds 1 and 3): NUL-terminated UTF-16 right in the node; kind 2: pointer to one.
//!
//! Control codes (box renderer `0x08012012` and the font code): `0x0A` new line, `0x0B` new box (page),
//! `0x08 n` colour change (n = 7 highlight, 2 normal), `0xA0` non-breaking space, `%s` / `%d` printf slots
//! (kinds 2 and 6). A line starting with `!` / `@` / `#` is drawn in a box at the top / middle / bottom
//! (y = 40 / 80 / 120; default bottom); the character itself is not shown.

use wf_rom::Rom;

pub const TEXT_BANKS: u32 = 0x087B_5B64;

pub struct Banks {
    /// (ROM address of the LZ container, UTF-16 code units)
    pub banks: Vec<(u32, Vec<u16>)>,
}

impl Banks {
    pub fn load(rom: &Rom) -> Banks {
        let mut banks = Vec::new();
        for i in 0..64u32 {
            let p = rom.u32(TEXT_BANKS + 4 * i);
            if !rom.isptr(p) {
                break;
            }
            let d = rom.lz(p).unwrap_or_default();
            let u: Vec<u16> = d.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            banks.push((p, u));
        }
        Banks { banks }
    }
    /// The code units of a packed text reference (empty when out of range).
    pub fn get(&self, v: u32) -> &[u16] {
        let (b, off, n) = unpack_ref(v);
        match self.banks.get(b as usize) {
            Some((_, u)) if off as usize + n as usize <= u.len() => &u[off as usize..off as usize + n as usize],
            _ => &[],
        }
    }
}

/// (bank, offset, length) of a packed text reference; offset and length in UTF-16 code units.
pub fn unpack_ref(v: u32) -> (u32, u32, u32) {
    (v & 0x3F, (v >> 6) & 0x7FFF, v >> 21)
}

/// UTF-16 at `a` up to the NUL (at most `max` units).
pub fn inline_units(rom: &Rom, a: u32, max: usize) -> Vec<u16> {
    let mut v = Vec::new();
    for i in 0..max as u32 {
        let c = rom.u16(a + 2 * i);
        if c == 0 {
            break;
        }
        v.push(c);
    }
    v
}

/// Readable text: printable characters as-is (incl. Latin-1), `\n` kept, `0x0B` → `{page}`, `0x08 n` → `{color n}`,
/// `0xA0` → `{nbsp}`, anything else → `{xxxx}`. [`encode`] inverts it exactly.
pub fn render(u: &[u16]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < u.len() {
        let c = u[i];
        match c {
            0x0A => s.push('\n'),
            0x0B => s.push_str("{page}"),
            0xA0 => s.push_str("{nbsp}"),
            0x08 if i + 1 < u.len() => {
                s.push_str(&format!("{{color {}}}", u[i + 1]));
                i += 1;
            }
            0x7B => s.push_str("{{"),
            0x20..=0x7E | 0xA1..=0xFF => s.push(char::from_u32(c as u32).unwrap()),
            _ => s.push_str(&format!("{{{c:04x}}}")),
        }
        i += 1;
    }
    s
}

/// Plain text for reading: control codes dropped, page breaks as blank lines, the box-position prefix removed.
pub fn plain(u: &[u16]) -> String {
    let mut s = String::new();
    let mut i = if matches!(u.first(), Some(0x21 | 0x40 | 0x23)) { 1 } else { 0 };
    while i < u.len() {
        match u[i] {
            0x0A => s.push('\n'),
            0x0B => s.push_str("\n\n"),
            0xA0 => s.push(' '),
            0x08 => i += 1,
            c => s.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}')),
        }
        i += 1;
    }
    s
}

/// Inverse of [`render`].
pub fn encode(s: &str) -> Vec<u16> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '{' {
            if it.peek() == Some(&'{') {
                it.next();
                out.push(0x7B);
                continue;
            }
            let mut tag = String::new();
            for c in it.by_ref() {
                if c == '}' {
                    break;
                }
                tag.push(c);
            }
            match tag.as_str() {
                "page" => out.push(0x0B),
                "nbsp" => out.push(0xA0),
                t if t.starts_with("color ") => {
                    out.push(0x08);
                    out.push(t[6..].parse().unwrap_or(2));
                }
                t => out.push(u16::from_str_radix(t, 16).unwrap_or(0xFFFD)),
            }
        } else if ch == '\n' {
            out.push(0x0A);
        } else {
            out.push(ch as u32 as u16);
        }
    }
    out
}

/// Box position chosen by the first character (`0x08012012`).
pub fn box_position(u: &[u16]) -> &'static str {
    match u.first() {
        Some(0x21) => "top",
        Some(0x40) => "middle",
        Some(0x23) => "bottom",
        _ => "default",
    }
}
