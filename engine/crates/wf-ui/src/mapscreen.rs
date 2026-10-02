//! The map screen (SELECT): frame, L / R page buttons, world map (setup `0x08024A9C`, page switch `0x080264B4`).
//!
//! Frame (BG0, 8bpp, screenblock 29, priority 0): tiles = webfoot-LZ `0x087315E8` (40 tiles) loaded at VRAM tile
//! 0x378; tilemap = webfoot-LZ `0x087317C8` (32×32 entries) with 0x378 added to every entry. The L / R buttons
//! (shown when there is a previous / next page) are 2×2-tile blocks: L = first 4 tiles of `0x087318B4` at tile
//! 0x373, placed at map (0, 0); R = first 4 tiles of `0x08731914` at tile 0x36F, placed at (28, 0). BG3
//! (screenblock 31, priority 3) is filled with tile 0x377 = the raw 8×8 tile at `0x0873198C`. Palette `0x0805652C`
//! in both halves. The page content is BG2: the area map, or the world map (webfoot-LZ `0x08732604`: 600 8bpp
//! tiles = a 240×160 picture in tile order, loaded at tile 1).

use crate::surface::Surface;
use serde_json::{json, Value};
use wf_rom::{hx, Rom};

pub const FRAME_TILES: u32 = 0x0873_15E8;
pub const FRAME_MAP: u32 = 0x0873_17C8;
pub const BTN_L: u32 = 0x0873_18B4;
pub const BTN_R: u32 = 0x0873_1914;
pub const FILL_TILE: u32 = 0x0873_198C;
pub const WORLD_MAP: u32 = 0x0873_2604;
const BASE: u16 = 0x378;

fn put_tile(s: &mut Surface, tiles: &dyn Fn(u16) -> Option<[u8; 64]>, ent: u16, tx: i32, ty: i32, key0: bool) {
    let Some(t) = tiles(ent & 0x3FF) else { return };
    let (hf, vf) = (ent & 0x400 != 0, ent & 0x800 != 0);
    for y in 0..8 {
        for x in 0..8 {
            let (sx, sy) = (if hf { 7 - x } else { x }, if vf { 7 - y } else { y });
            let c = t[sy * 8 + sx];
            if !key0 || c != 0 {
                s.set(tx * 8 + x as i32, ty * 8 + y as i32, c);
            }
        }
    }
}

/// Frame + buttons composited over the BG3 fill, as palette indices (240×160, camera at 0). `grid`: BG3 =
/// tile 0x377 (setup argument != 0, world-map page) instead of tile 0.
pub fn render_frame(rom: &Rom, show_l: bool, show_r: bool, grid: bool) -> Surface {
    let tiles = rom.lz(FRAME_TILES).unwrap_or_default();
    let map = rom.lz(FRAME_MAP).unwrap_or_default();
    let l = rom.lz(BTN_L).unwrap_or_default();
    let r = rom.lz(BTN_R).unwrap_or_default();
    let fill: Vec<u8> = rom.bytes(FILL_TILE, 64).to_vec();
    let vram = |t: u16| -> Option<[u8; 64]> {
        let src: &[u8] = match t {
            0x36F..=0x372 if show_r => &r[(t as usize - 0x36F) * 64..],
            0x373..=0x376 if show_l => &l[(t as usize - 0x373) * 64..],
            0x377 => &fill[..],
            0x378.. => tiles.get((t as usize - 0x378) * 64..)?,
            _ => return Some([0; 64]),
        };
        let mut o = [0u8; 64];
        o.copy_from_slice(src.get(..64)?);
        Some(o)
    };
    let mut s = Surface::new(240, 160);
    // BG3: tile 0x377 everywhere (the grid; world-map page) or tile 0 (empty; area-map page)
    if grid {
        for ty in 0..20 {
            for tx in 0..30 {
                put_tile(&mut s, &vram, 0x377, tx, ty, false);
            }
        }
    }
    // BG0 on top (index 0 transparent)
    let mut ents: Vec<u16> = map.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]).wrapping_add(BASE)).collect();
    if ents.len() < 1024 {
        ents.resize(1024, 0);
    }
    if show_l {
        ents[0] = 0x373;
        ents[1] = 0x374;
        ents[32] = 0x375;
        ents[33] = 0x376;
    }
    if show_r {
        ents[28] = 0x36F;
        ents[29] = 0x370;
        ents[60] = 0x371;
        ents[61] = 0x372;
    }
    for ty in 0..20 {
        for tx in 0..30 {
            put_tile(&mut s, &vram, ents[(ty * 32 + tx) as usize], tx, ty, true);
        }
    }
    s
}

pub fn world_map(rom: &Rom) -> (Vec<u8>, Surface) {
    let d = rom.lz(WORLD_MAP).unwrap_or_default();
    let s = Surface::from_tiles(&d, 30, 20);
    (d, s)
}

pub fn json() -> Value {
    json!({
        "frame": {"image": "mapscreen/frame.png", "tiles": "mapscreen/frame_tiles.png", "tilemap": "mapscreen/frame_map.json",
                  "bg": 0, "priority": 0, "tile_base": 0x378, "rom": {"tiles": hx(FRAME_TILES), "tilemap": hx(FRAME_MAP)}},
        "fill": {"bg": 3, "priority": 3, "tile": 0x377, "rom": hx(FILL_TILE)},
        "buttons": {
            "L": {"image": "mapscreen/button_l.png", "tile_block": [0, 0, 2, 2], "tile": 0x373, "rom": hx(BTN_L), "shown": "previous page exists (vfunc +0x2C)"},
            "R": {"image": "mapscreen/button_r.png", "tile_block": [28, 0, 2, 2], "tile": 0x36F, "rom": hx(BTN_R), "shown": "next page exists (vfunc +0x30)"},
            "note": "each blob holds 8 tiles; the code copies the first 4 (0x100 bytes)",
        },
        "world_map": {"image": "mapscreen/world_map.png", "bg": 2, "tile_base": 1, "rom": hx(WORLD_MAP),
                      "note": "600 tiles in row-major order = the 240x160 picture; the outline of visited areas is drawn over it by script (0x0877D034)"},
        "area_maps": "not decoded: per-area map pages (tile + map blob pairs 0x08733040..0x0874F058 with records like 0x08733AE4) - see docs/ui.md gaps",
        "palette": "ui/palette.json (0x0805652C, both halves)",
        "blend": {"BLDCNT": "0x3E5E / 0x3F5F (fade between pages, 0x08024BE6)"},
        "code": {"setup": "0x08024A9C", "world_page": "0x080264B4"},
    })
}
