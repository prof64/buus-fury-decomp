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

// ---------------------------------------------------------------------------------------------------------------
// The map screen's Scouter page (`0x080261D4`) and its info page (`0x08025E50`): `ui/menu/scouter.json` + pictures.

/// the scouter database: 232 entries of 16 bytes {u16 name string, u16 description string, u32 sprite def id,
/// s32 portrait (dialog portrait arg: >= 8 = portraits index + 8, 0 = none), s32 stats (>= 0: enemy type table
/// `0x08711274` index; -2 - c: story character c; -1: unknown)}
pub const SCOUT_DB: u32 = 0x0874_D380;
pub const SCOUT_DB_N: u32 = 0xE8;
/// the enemy type table (28 bytes; +4 u32 HP, +0xD / +0xE / +0xF STR / POW / END)
pub const ENEMY_TYPES: u32 = 0x0871_1274;
/// the selection's colour remaps (8 pointers, 4 distinct 256-byte tables; `0x08027484`)
pub const SCOUT_REMAPS: u32 = 0x0874_D360;
/// cursor picture record (3 frames 32x32 8bpp; `0x08026F94`)
pub const SCOUT_CURSOR: u32 = 0x0874_D044;
/// info page background (240x160 8bpp picture record) and the scroll arrows' tile frames (2 x 64 bytes each, up
/// at +0x14, down at +0x94 of `0x0874D248`; OAM words at +4 / +8)
pub const SCOUT_INFO_BG: u32 = 0x0873_2384;
pub const SCOUT_ARROWS: u32 = 0x0874_D248;
/// the info sprite's direction cycle (4 bytes, `0x08027336`)
pub const SCOUT_DIRS: u32 = 0x0874_D35C;

pub fn scouter(rom: &Rom, out: &mut Content, pal: &[Rgb]) {
    let mut db = Vec::new();
    for i in 0..SCOUT_DB_N {
        let a = SCOUT_DB + 16 * i;
        let stats = rom.u32(a + 12) as i32;
        let mut e = serde_json::json!({"name": rom.u16(a), "desc": rom.u16(a + 2), "def": rom.u32(a + 4), "portrait": rom.u32(a + 8) as i32, "stats": stats});
        if stats >= 0 {
            let t = ENEMY_TYPES + 28 * stats as u32;
            e["enemy"] = serde_json::json!({"hp": rom.u32(t + 4), "str": rom.u8(t + 0xD), "pow": rom.u8(t + 0xE), "end": rom.u8(t + 0xF)});
        }
        db.push(e);
    }
    let remaps: Vec<Vec<u8>> = (0..8u32).map(|k| (0..256u32).map(|i| rom.u8(rom.u32(SCOUT_REMAPS + 4 * k) + i)).collect()).collect();
    let cur = rom.lz(SCOUT_CURSOR).unwrap_or_default();
    let mut s = Surface::new(96, 32);
    for f in 0..3usize {
        for i in 0..1024usize {
            // 8bpp OBJ tiles (1D mapping: 4 x 4 tiles of 64 bytes)
            let (t, p) = (i / 64, i % 64);
            if let Some(&c) = cur.get(f * 1024 + i) {
                s.set((f * 32 + (t % 4) * 8 + p % 8) as i32, ((t / 4) * 8 + p / 8) as i32, c);
            }
        }
    }
    out.put("ui/menu/scouter_cursor.png", s.png(pal, true));
    let mut bg = rom.lz(SCOUT_INFO_BG).unwrap_or_default();
    bg.resize(240 * 160, 0);
    out.put("ui/menu/scouter_info.png", Surface::from_vec(240, 160, bg).png(pal, false));
    let mut ar = Surface::new(32, 8);
    for (k, off) in [0x14u32, 0x94].iter().enumerate() {
        for f in 0..2u32 {
            for i in 0..64u32 {
                ar.set(((k as u32 * 2 + f) * 8 + (i & 7)) as i32, (i >> 3) as i32, rom.u8(SCOUT_ARROWS + off + f * 64 + i));
            }
        }
    }
    out.put("ui/menu/scouter_arrows.png", ar.png(pal, true));
    let words: Vec<u32> = (0..2u32).map(|k| rom.u32(SCOUT_ARROWS + 4 + 4 * k)).collect();
    let dirs: Vec<u8> = (0..4u32).map(|k| rom.u8(SCOUT_DIRS + k)).collect();
    out.put_json(
        "ui/menu/scouter.json",
        &serde_json::json!({"db": db, "remaps": remaps, "cursor": "menu/scouter_cursor.png", "info_bg": "menu/scouter_info.png",
                            "arrows": {"png": "menu/scouter_arrows.png", "oam": words, "note": "frames: up 0 1, down 2 3 (8x8)"},
                            "info_dirs": dirs,
                            "rom": {"db": format!("0x{SCOUT_DB:08X}"), "enemy_types": format!("0x{ENEMY_TYPES:08X}"), "remaps": format!("0x{SCOUT_REMAPS:08X}"),
                                    "cursor": format!("0x{SCOUT_CURSOR:08X}"), "info_bg": format!("0x{SCOUT_INFO_BG:08X}"), "arrows": format!("0x{SCOUT_ARROWS:08X}")}}),
    );
}
