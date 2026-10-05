//! The GBA work RAM of a real-game state (IWRAM `0x03000000` 32 KiB, EWRAM `0x02000000` 256 KiB) and the
//! addresses the importer reads (re/buu/notes.md, data/field/gamestate_layout.json, tools/field/common_field.py).

use serde_json::Value;

/// IWRAM / EWRAM bases and sizes
pub const IWRAM: u32 = 0x0300_0000;
pub const IWRAM_SIZE: usize = 0x8000;
pub const EWRAM: u32 = 0x0200_0000;
pub const EWRAM_SIZE: usize = 0x4_0000;

/// GameData (0x8A8 bytes: +0 current character, +1 game bits, +2 supplies u8[83], +0x55 story flags, +0xE4 equipment
/// count + u32[250], +0x4D0 play-time base, +0x4D4 countdown, +0x4D8 stopwatch, +0x4DC Holy Water timer + 4 bonuses,
/// +0x4F0 timed special, +0x4F4 zenie, +0x4F8 the 9 character records)
pub const GD: u32 = 0x0300_1068;
/// character record i = REC0 + 0x68 i
pub const REC0: u32 = 0x0300_1560;
/// script words L0..L3 / globals G0..G3
pub const WORDS_L: u32 = 0x0300_1020;
pub const WORDS_G: u32 = 0x0300_1058;
/// loot position (x, y) and velocity (vx, vy 24.8)
pub const LOOT: u32 = 0x0300_1010;
/// world state: +0 world, +1 area, +2 variant, +4 area record, +8 u16 last banner name id, +0x10 load counter
pub const WS: u32 = 0x0300_197C;
/// the player object pointer
pub const PLAYER: u32 = 0x0300_1C34;
/// object world: +4 count + object pointers (updated last to first)
pub const OBJW: u32 = 0x0300_1C4C;
/// trigger list: +0 count + pointers, +0xC4 check counter, +0xD0 enabled
pub const TRIGS: u32 = 0x0300_2A64;
/// camera object (22.10 x, y)
pub const CAM: u32 = 0x0300_19A4;
/// RNG state s0, s1
pub const RNG: u32 = 0x0300_4A5C;
/// logic frame counter
pub const FC: u32 = 0x0300_0F94;
/// the current main-loop mode object (vtable at +0; fades: +4 brightness, +8 step)
pub const MODE: u32 = 0x0300_0F90;
/// input manager: +2 held (logic frames only), +4 pressed, +6 released, +8 press ring keys u16[64], +0x88 ring
/// frame counters
pub const INPUT: u32 = 0x0300_3300;
/// `get_variant_byte`
pub const VARIANT_BYTE: u32 = 0x0300_2BB0;
/// run_event nesting (scene depth)
pub const DEPTH: u32 = 0x0300_2A5C;
/// sound driver (song player 0: +5 == 1 playing, +0xC song header)
pub const SONG_PLAYER: u32 = 0x0300_38A4;
pub const SONG_TABLE: u32 = 0x083B_B78C;
/// the idle state's frame counter (1024 -> bored animation)
pub const IDLE_COUNT: u32 = 0x0300_2B48;

/// main-loop mode vtables
pub const MODE_NORMAL: u32 = 0x0804_94AC;
pub const MODE_FADE_OUT: u32 = 0x0804_BB74;
pub const MODE_FADE_IN: u32 = 0x0804_B57C;
/// the modal scene (an event's scene object runs as the main mode; SCRIPT.md mode classes)
pub const MODE_SCENE: u32 = 0x0804_9D18;

/// object vtables (tools/field/common_field.py VT)
pub const VT_PLAYER: u32 = 0x0804_C9F0;
pub const VT_ENEMY: u32 = 0x0804_C484;
pub const VT_NPC: u32 = 0x0804_C92C;
pub const VT_NPC_TALK: u32 = 0x0804_CEF4;
pub const VT_BREAKABLE: u32 = 0x0804_CE00;
pub const VT_HUD: u32 = 0x0804_9FEC;
pub const VT_BANNER: u32 = 0x0804_B528;
/// the player's autowalk state object vtable (state at player +0x14)
pub const ST_AUTOWALK: u32 = 0x0804_E00C;

/// A copy of the GBA's work RAM.
#[derive(Clone)]
pub struct GbaRam {
    pub iwram: Vec<u8>,
    pub ewram: Vec<u8>,
    /// the screen is black at the cut (the palette / blend IO is not work RAM: the recording's display probe; a
    /// script's fade_out before its warp leaves it so - in-event imports)
    pub screen_black: Option<bool>,
    /// the cut stands inside a long game iteration (the recording's first row still has the cut's logic count): the
    /// iteration's remaining work - its camera update - is not in RAM yet (in-event imports)
    pub mid_iteration: bool,
    /// the recording's actor rows at the cut (object pointer, sprite id, raw animation, cel, tick): sprite ids and
    /// animation indices need the ROM's sprite-def tables, which a work-RAM snapshot does not have (mid-area imports)
    pub actor_rows: Vec<(u32, i32, i32, i32, i32)>,
}

impl GbaRam {
    pub fn new(iwram: Vec<u8>, ewram: Vec<u8>) -> GbaRam {
        let mut r = GbaRam { iwram, ewram, screen_black: None, mid_iteration: false, actor_rows: vec![] };
        r.iwram.resize(IWRAM_SIZE, 0);
        r.ewram.resize(EWRAM_SIZE, 0);
        r
    }

    /// `{"iwram": base64(zlib(bytes)), "ewram": ...}` (tools/segments/segment.py)
    pub fn from_json(v: &Value) -> Result<GbaRam, String> {
        let get = |k: &str| -> Result<Vec<u8>, String> {
            let s = v.get(k).and_then(|x| x.as_str()).ok_or(format!("ram.{k} missing"))?;
            let z = b64(s)?;
            miniz_oxide::inflate::decompress_to_vec_zlib(&z).map_err(|e| format!("ram.{k}: inflate {e:?}"))
        };
        Ok(GbaRam::new(get("iwram")?, get("ewram")?))
    }

    fn slot(&self, a: u32) -> Option<&[u8]> {
        if (IWRAM..IWRAM + IWRAM_SIZE as u32).contains(&a) {
            Some(&self.iwram[(a - IWRAM) as usize..])
        } else if (EWRAM..EWRAM + EWRAM_SIZE as u32).contains(&a) {
            Some(&self.ewram[(a - EWRAM) as usize..])
        } else {
            None
        }
    }
    /// whether `a` is a work-RAM address (object pointers)
    pub fn valid(&self, a: u32) -> bool {
        self.slot(a).is_some()
    }
    /// n bytes at `a` (zeros outside work RAM)
    pub fn read(&self, a: u32, n: usize) -> Vec<u8> {
        match self.slot(a) {
            Some(s) => {
                let mut v = s[..n.min(s.len())].to_vec();
                v.resize(n, 0);
                v
            }
            None => vec![0; n],
        }
    }
    pub fn r8(&self, a: u32) -> u8 {
        self.slot(a).map(|s| s[0]).unwrap_or(0)
    }
    pub fn s8(&self, a: u32) -> i32 {
        self.r8(a) as i8 as i32
    }
    pub fn r16(&self, a: u32) -> u16 {
        let b = self.read(a, 2);
        u16::from_le_bytes([b[0], b[1]])
    }
    pub fn s16(&self, a: u32) -> i32 {
        self.r16(a) as i16 as i32
    }
    pub fn r32(&self, a: u32) -> u32 {
        let b = self.read(a, 4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    pub fn s32(&self, a: u32) -> i32 {
        self.r32(a) as i32
    }

    /// every EWRAM word equal to `vt` (heap objects outside the world list: a script command's object)
    pub fn ewram_with(&self, vt: u32) -> Vec<u32> {
        (0..self.ewram.len() / 4).map(|k| 0x0200_0000 + 4 * k as u32).filter(|&a| self.r32(a) == vt).collect()
    }

    /// A pause page's cut: the field torn down while the page shows. The world save `0x08011DB4` copies the object
    /// world (0xE00 bytes at [`OBJW`]) into a heap block (pointer at `OBJW + 0xBF8`) and clears the live lists; the
    /// objects themselves stay in the heap, and `0x08011E0E` copies the block back on the close. This RAM with the
    /// block restored and the leader pointer set (the close's message 0xB does that): None when the world is live
    pub fn with_saved_world(&self) -> Option<GbaRam> {
        let saved = self.r32(OBJW + 0xBF8);
        if self.r32(OBJW + 4) != 0 || !(EWRAM..EWRAM + EWRAM_SIZE as u32).contains(&saved) || self.r32(PLAYER) != 0 {
            return None;
        }
        let block = self.read(saved, 0xE00);
        let mut r = self.clone();
        let o = (OBJW - IWRAM) as usize;
        r.iwram[o..o + 0xE00].copy_from_slice(&block);
        let p = r.objects().into_iter().find(|&o| r.r32(o) == VT_PLAYER)?;
        let po = (PLAYER - IWRAM) as usize;
        r.iwram[po..po + 4].copy_from_slice(&p.to_le_bytes());
        Some(r)
    }

    /// the object world's pointers in list order (the game updates them last to first)
    pub fn objects(&self) -> Vec<u32> {
        let n = (self.r32(OBJW + 4) as usize).min(256);
        (0..n).map(|k| self.r32(OBJW + 8 + 4 * k as u32)).filter(|&o| self.valid(o)).collect()
    }
    /// the objects a purge suspended (`0x0800F516`: out of the world list until a text box closes, `0x08011E94` puts
    /// them back last first): world +0x40C the count, +0x410.. the objects, in suspension order
    pub fn suspended_objects(&self) -> Vec<u32> {
        let n = (self.r32(OBJW + 0x40C) as usize).min(64);
        (0..n).map(|k| self.r32(OBJW + 0x410 + 4 * k as u32)).filter(|&o| self.valid(o)).collect()
    }
    /// the player object (0 = none)
    pub fn player(&self) -> u32 {
        let p = self.r32(PLAYER);
        if self.valid(p) {
            p
        } else {
            0
        }
    }
    pub fn rng(&self) -> [u32; 2] {
        [self.r32(RNG), self.r32(RNG + 4)]
    }
    pub fn lf(&self) -> u32 {
        self.r32(FC)
    }
    pub fn mode_vt(&self) -> u32 {
        let m = self.r32(MODE);
        if self.valid(m) {
            self.r32(m)
        } else {
            0
        }
    }
}

pub(crate) fn b64(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c).ok_or_else(|| format!("bad base64 byte {c}"))?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::b64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::b64("aGVsbG8hIQ==").unwrap(), b"hello!!");
    }
}
