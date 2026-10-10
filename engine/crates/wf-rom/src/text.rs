//! The game's string table: `u32[]` pointers at `0x0806BCE8` to NUL-terminated UTF-16LE strings
//! (`re/buu/strtab.py`). Ids are what records store (`u16 name id`).

use crate::Rom;

pub const STRTAB: u32 = 0x0806_BCE8;

/// Raw UTF-16 string at `a` (up to `maxlen` code units), every code unit mapped with `char::from_u32`.
pub fn ustr_raw(rom: &Rom, a: u32, maxlen: usize) -> String {
    let mut s = String::new();
    for i in 0..maxlen {
        let c = rom.u16(a + 2 * i as u32);
        if c == 0 {
            break;
        }
        s.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
    }
    s
}

/// String as `re/buu/strtab.py:ustr` prints it: printable ASCII / Latin-1 and `\n` kept, everything
/// else (game control codes) as `{xxxx}`.
pub fn ustr(rom: &Rom, a: u32, maxlen: usize) -> String {
    let mut s = String::new();
    for i in 0..maxlen {
        let c = rom.u16(a + 2 * i as u32);
        if c == 0 {
            break;
        }
        if (0x20..0x7f).contains(&c) || c == 10 || (0xa0..0x100).contains(&c) {
            s.push(char::from_u32(c as u32).unwrap());
        } else {
            s.push_str(&format!("{{{c:04x}}}"));
        }
    }
    s
}

/// The whole table: pointer list until the first non-ROM pointer.
pub struct StringTable {
    pub ptrs: Vec<u32>,
    pub strings: Vec<String>,
}

impl StringTable {
    pub fn load(rom: &Rom) -> StringTable {
        let mut ptrs = Vec::new();
        loop {
            let p = rom.u32(STRTAB + 4 * ptrs.len() as u32);
            if !(0x0800_0000..0x0880_0000).contains(&p) {
                break;
            }
            ptrs.push(p);
        }
        let strings = ptrs.iter().map(|&p| ustr(rom, p, 2000)).collect();
        StringTable { ptrs, strings }
    }
    pub fn get(&self, i: usize) -> Option<&str> {
        self.strings.get(i).map(|s| s.as_str())
    }
    pub fn len(&self) -> usize {
        self.strings.len()
    }
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}
