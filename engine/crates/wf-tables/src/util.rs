//! Small helpers shared by the table builders.

use serde_json::{Map, Value};
use wf_rom::text::StringTable;
use wf_rom::Rom;

/// Python `hex()` formatting (lowercase, no padding, `-0x..` for negatives) — the research JSON uses it for
/// ROM addresses, so the content files keep the same spelling.
pub fn ph(v: i64) -> String {
    if v < 0 {
        format!("-{:#x}", -v)
    } else {
        format!("{v:#x}")
    }
}

/// Python `round(x, n)` (correctly rounded, ties to even on the exact binary value — same as Rust's
/// fixed-precision formatting).
pub fn pyround(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse().unwrap()
}

/// Python `str(float)` (repr): shortest round-trip digits, `.0` for integral values, exponent form outside
/// [1e-4, 1e16).
pub fn pyfloat(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let a = x.abs();
    if (1e-4..1e16).contains(&a) {
        let s = format!("{x}");
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        let s = format!("{x:e}"); // e.g. 1e-5, 1.5e16
        let (m, e) = s.split_once('e').unwrap();
        let ev: i32 = e.parse().unwrap();
        format!("{m}e{}{:02}", if ev < 0 { '-' } else { '+' }, ev.abs())
    }
}

/// f64 → JSON number (keeps integral floats as floats, like Python).
pub fn f(x: f64) -> Value {
    Value::from(x)
}

/// Shared read-only context: the ROM and its string table.
pub struct Ctx<'a> {
    pub rom: &'a Rom,
    pub strings: StringTable,
}

impl<'a> Ctx<'a> {
    pub fn new(rom: &'a Rom) -> Ctx<'a> {
        Ctx { rom, strings: StringTable::load(rom) }
    }
    /// String id → text, `null` when out of range (Python `S()`).
    pub fn s(&self, id: i64) -> Value {
        if id < 0 {
            return Value::Null;
        }
        match self.strings.get(id as usize) {
            Some(s) => Value::from(s),
            None => Value::Null,
        }
    }
    pub fn s_str(&self, id: i64) -> Option<String> {
        if id < 0 {
            return None;
        }
        self.strings.get(id as usize).map(String::from)
    }
    pub fn u8(&self, a: u32) -> u8 {
        self.rom.u8(a)
    }
    pub fn s8(&self, a: u32) -> i8 {
        self.rom.s8(a)
    }
    pub fn u16(&self, a: u32) -> u16 {
        self.rom.u16(a)
    }
    pub fn s16(&self, a: u32) -> i16 {
        self.rom.s16(a)
    }
    pub fn u32(&self, a: u32) -> u32 {
        self.rom.u32(a)
    }
    pub fn s32(&self, a: u32) -> i32 {
        self.rom.s32(a)
    }
    pub fn bytes(&self, a: u32, n: usize) -> &[u8] {
        self.rom.bytes(a, n)
    }
}

/// Parse an embedded annotation file.
pub fn annot(src: &str) -> Value {
    serde_json::from_str(src).expect("embedded annotation json")
}

pub fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub fn hexbytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `0x08xxxxxx`-range check used by the research scripts.
pub fn in_rom_range(p: u32) -> bool {
    (0x0800_0000..0x0880_0000).contains(&p)
}

/// Lower-case ASCII slug (`re.sub('[^a-z0-9]+', '_', s.lower()).strip('_')`).
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut us = false;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            us = false;
        } else if !us {
            out.push('_');
            us = true;
        }
    }
    out.trim_matches('_').to_string()
}
