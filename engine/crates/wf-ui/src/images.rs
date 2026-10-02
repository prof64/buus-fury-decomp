//! UI pictures: webfoot-LZ blobs of linear 8bpp pixels (sizes fixed in code), drawn with the UI palette.

use crate::surface::Surface;
use wf_rom::Rom;

pub struct Pic {
    pub id: &'static str,
    pub rom: u32,
    pub w: u32,
    pub h: u32,
    /// Full-screen pictures are opaque (index 0 = palette colour 0); pieces treat index 0 as transparent.
    pub opaque: bool,
    pub frames: u32,
    pub desc: &'static str,
    pub code: &'static str,
}

/// Fixed pictures (container address, size from the code that loads them).
pub const PICS: &[Pic] = &[
    Pic { id: "screen_legal", rom: 0x0805_5620, w: 240, h: 160, opaque: true, frames: 1, desc: "copyright / licence screen (first boot screen)", code: "0x08004EFC" },
    Pic { id: "screen_atari", rom: 0x086F_9EC8, w: 240, h: 160, opaque: true, frames: 1, desc: "Atari logo", code: "0x08017BE4" },
    Pic { id: "screen_funimation", rom: 0x086F_A398, w: 240, h: 160, opaque: true, frames: 1, desc: "FUNimation logo", code: "0x08017BF4" },
    Pic { id: "screen_webfoot", rom: 0x086F_ABF4, w: 240, h: 160, opaque: true, frames: 1, desc: "Webfoot Technologies logo", code: "0x08017C14" },
    Pic { id: "screen_title", rom: 0x083B_E3D4, w: 240, h: 160, opaque: true, frames: 1, desc: "title screen art (the START / OPTIONS menu is drawn over it in the menu font)", code: "0x08005414, 0x0801E744, 0x0801E9E0" },
    Pic { id: "cursor", rom: 0x086B_7E8C, w: 16, h: 16, opaque: false, frames: 1, desc: "menu cursor (fireball arrow; title menu, inventory)", code: "0x080162C4, 0x080179D8" },
    Pic { id: "star_bronze", rom: 0x086B_7ED8, w: 16, h: 16, opaque: false, frames: 1, desc: "journal goal marker", code: "rodata 0x086B8030" },
    Pic { id: "star_silver", rom: 0x086B_7F3C, w: 16, h: 16, opaque: false, frames: 1, desc: "journal goal marker", code: "rodata 0x086B802C" },
    Pic { id: "star_gold", rom: 0x086B_7FAC, w: 16, h: 16, opaque: false, frames: 1, desc: "journal goal marker", code: "rodata 0x086B8028" },
    Pic { id: "gauge_hp_ep", rom: 0x086B_8294, w: 53, h: 9, opaque: false, frames: 1, desc: "HP / EP gauge (inventory, item preview)", code: "0x08016A7D" },
    Pic { id: "emote_sparkle", rom: 0x086B_814C, w: 8, h: 8, opaque: false, frames: 5, desc: "emote animation strip, 8x8 frames stacked vertically (name visual)", code: "rodata 0x086B8128" },
    Pic { id: "emote_bubble", rom: 0x086B_81A0, w: 8, h: 8, opaque: false, frames: 4, desc: "emote animation strip (speech bubble)", code: "rodata 0x086B812C" },
    Pic { id: "emote_marks", rom: 0x086B_8218, w: 8, h: 8, opaque: false, frames: 3, desc: "emote animation strip (! / ? marks)", code: "rodata 0x086B8130" },
    Pic { id: "emote_heart", rom: 0x086B_81E8, w: 8, h: 8, opaque: false, frames: 4, desc: "emote animation strip (heart)", code: "rodata 0x086B8134" },
    Pic { id: "emote_drop", rom: 0x086B_8244, w: 8, h: 8, opaque: false, frames: 6, desc: "emote animation strip (sweat drop / flame)", code: "rodata 0x086B8138" },
];

pub fn decode(rom: &Rom, p: &Pic) -> Surface {
    let n = (p.w * p.h * p.frames) as usize;
    let mut d = rom.lz(p.rom).unwrap_or_default();
    d.resize(n, 0);
    Surface::from_vec(p.w, p.h * p.frames, d)
}

/// Decode a headerless LZ stream (called through `0x08049120` with the literal width in r3; every UI stream
/// starts by setting its literal mode, so the initial width does not matter — checked by the tests).
pub fn stream(rom: &Rom, ptr: u32, n: usize) -> Vec<u8> {
    stream_typ(rom, ptr, n, 1)
}

pub fn stream_typ(rom: &Rom, ptr: u32, n: usize, typ: u32) -> Vec<u8> {
    let start = (ptr & 0x1FF_FFFF) as usize;
    let end = (start + 0x10000).min(rom.len());
    let mut buf = Vec::with_capacity(end - start + 8);
    buf.extend_from_slice(&typ.to_le_bytes());
    buf.extend_from_slice(&(n as u32).to_le_bytes());
    buf.extend_from_slice(&rom.data()[start..end]);
    let mut d = wf_rom::lz::decompress(&buf, 0).map(|x| x.0).unwrap_or_default();
    d.resize(n, 0);
    d
}

/// Button prompt icons: table `0x086B77A4`, 12-byte records `u16 id, u16 key mask, u8 w, u8 h, u16 0,
/// ptr → {u32 frames, ptr stream[frames]}` (animated: 3 frames).
pub const BUTTONS: u32 = 0x086B_77A4;

pub struct Button {
    pub id: u16,
    pub key: u16,
    pub name: &'static str,
    pub w: u32,
    pub h: u32,
    pub frames: Vec<Vec<u8>>,
    pub rom: u32,
}

fn key_name(k: u16) -> &'static str {
    match k {
        0x001 => "a",
        0x002 => "b",
        0x004 => "select",
        0x008 => "start",
        0x010 => "right",
        0x020 => "left",
        0x040 => "up",
        0x080 => "down",
        0x100 => "r",
        0x200 => "l",
        _ => "unknown",
    }
}

pub fn buttons(rom: &Rom) -> Vec<Button> {
    let mut out = Vec::new();
    for i in 0..16u32 {
        let a = BUTTONS + 12 * i;
        let id = rom.u16(a);
        if id as u32 != i {
            break;
        }
        let key = rom.u16(a + 2);
        let (w, h) = (rom.u8(a + 4) as u32, rom.u8(a + 5) as u32);
        let list = rom.u32(a + 8);
        if !rom.isptr(list) {
            break;
        }
        let n = rom.u32(list).min(8);
        let frames = (0..n).map(|k| stream(rom, rom.u32(list + 4 + 4 * k), (w * h) as usize)).collect();
        out.push(Button { id, key, name: key_name(key), w, h, frames, rom: a });
    }
    out
}
