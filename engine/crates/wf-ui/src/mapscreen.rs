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

// ---------------------------------------------------------------------------------------------------------------
// Area map pages (the map screen's first page, setup `0x0802574C`): table `0x0874CA24` = u32 count + pointer to
// record pointers. Record: +0 tiles picture record (8bpp BG tiles at char 0), +4 script (unused: 0 everywhere),
// +8 n room-marker entries, +0xC entries (24 bytes: px rect x0 y0 x1 y1, +0x10 OBJ attr0, +0x14 u16 tile offset,
// +0x16 u8 shown), +0x10 OBJ tiles picture record, +0x14 n rooms, +0x18 room pointers. Room: +0 u8 world, +1 u8
// area, +2 u16 visited bit (story flag 0x1C0 + bit), +4 rect1 (tiles x0 y0 x1 y1, s32), +0x14 rect2, +0x24 map1
// (w1 * h1 u16 BG entries), then map2 (w2 * h2). Player marker tiles `0x0874CF2C` (4 frames 8x8 8bpp), scroll
// arrows `0x0874CB24` (16x8, 4 frames) / `0x0874CD24` (8x16), arrow OAM table `0x0874CB04` (u32 attr0 | attr1 << 16,
// frames pointer) x 4.

pub const AREA_MAPS: u32 = 0x0874_CA24;
pub const MARKER_TILES: u32 = 0x0874_CF2C;
pub const ARROWS: u32 = 0x0874_CB04;

fn tiles_png(data: &[u8], pal: &[wf_rom::gfx::Rgb]) -> (u32, Vec<u8>) {
    let n = (data.len() / 64) as u32;
    let tw = 16u32;
    let th = n.div_ceil(tw).max(1);
    (n, Surface::from_tiles(data, tw, th).png(pal, true))
}

/// `ui/mapscreen/area_maps.json` + `area_NN.png` (tiles, 16 per row), `area_NN_obj.png`, `marker.png` (4 frames
/// side by side), `arrow_h.png` (16x8 x 4), `arrow_v.png` (8x16 x 4)
pub fn area_maps(rom: &Rom, out: &mut wf_rom::content::Content, pal: &[wf_rom::gfx::Rgb]) {
    let n = rom.u32(AREA_MAPS);
    let arr = rom.u32(AREA_MAPS + 4);
    let mut recs = Vec::new();
    for i in 0..n {
        let r = rom.u32(arr + 4 * i);
        let tiles = rom.lz(rom.u32(r)).unwrap_or_default();
        let (nt, png) = tiles_png(&tiles, pal);
        out.put(format!("ui/mapscreen/area_{i:02}.png"), png);
        let obj = rom.u32(r + 0x10);
        let objs = if obj != 0 {
            let d = rom.lz(obj).unwrap_or_default();
            let (_, png) = tiles_png(&d, pal);
            out.put(format!("ui/mapscreen/area_{i:02}_obj.png"), png);
            Value::String(format!("mapscreen/area_{i:02}_obj.png"))
        } else {
            Value::Null
        };
        let ne = rom.u32(r + 8);
        let e = rom.u32(r + 0xC);
        let entries: Vec<Value> = (0..ne)
            .map(|k| {
                let a = e + 24 * k;
                json!({"rect": (0..4).map(|j| rom.s32(a + 4 * j)).collect::<Vec<_>>(), "attr0": rom.u16(a + 0x10),
                       "tile": rom.u16(a + 0x14), "shown": rom.u8(a + 0x16)})
            })
            .collect();
        let nr = rom.u32(r + 0x14);
        let rooms: Vec<Value> = (0..nr)
            .map(|k| {
                let m = rom.u32(r + 0x18 + 4 * k);
                let r1: Vec<i32> = (0..4).map(|j| rom.s32(m + 4 + 4 * j)).collect();
                let r2: Vec<i32> = (0..4).map(|j| rom.s32(m + 0x14 + 4 * j)).collect();
                let n1 = ((r1[2] - r1[0]).max(0) * (r1[3] - r1[1]).max(0)) as u32;
                let n2 = ((r2[2] - r2[0]).max(0) * (r2[3] - r2[1]).max(0)) as u32;
                let map1: Vec<u16> = (0..n1).map(|j| rom.u16(m + 0x24 + 2 * j)).collect();
                let map2: Vec<u16> = (0..n2).map(|j| rom.u16(m + 0x24 + 2 * n1 + 2 * j)).collect();
                json!({"world": rom.u8(m), "area": rom.u8(m + 1), "bit": rom.u16(m + 2), "rect1": r1, "rect2": r2, "map1": map1, "map2": map2})
            })
            .collect();
        recs.push(json!({"index": i, "rom": hx(r), "tiles": format!("mapscreen/area_{i:02}.png"), "tile_count": nt, "objs": objs,
                         "entries": entries, "rooms": rooms}));
    }
    // player marker: 4 frames of one 8bpp tile
    let mk = rom.bytes(MARKER_TILES, 4 * 64).to_vec();
    out.put("ui/mapscreen/marker.png", Surface::from_tiles(&mk, 4, 1).png(pal, true));
    // arrows: 1D OBJ tiles, 2 tiles per frame (16x8: left, right; 8x16: top, bottom)
    let mut arrows = Vec::new();
    for (k, name, horiz) in [(0u32, "arrow_h", true), (2, "arrow_v", false)] {
        let base = rom.u32(ARROWS + 8 * k + 4);
        let d = rom.bytes(base, 4 * 128).to_vec();
        let (w, h) = if horiz { (64, 8) } else { (32, 16) };
        let mut s = Surface::new(w, h);
        for f in 0..4u32 {
            for t in 0..2u32 {
                for i in 0..64u32 {
                    let c = d[(f * 128 + t * 64 + i) as usize];
                    let (x, y) = if horiz { (f * 16 + t * 8 + (i & 7), i >> 3) } else { (f * 8 + (i & 7), t * 8 + (i >> 3)) };
                    s.set(x as i32, y as i32, c);
                }
            }
        }
        out.put(format!("ui/mapscreen/{name}.png"), s.png(pal, true));
    }
    for k in 0..4u32 {
        let a = rom.u32(ARROWS + 8 * k);
        arrows.push(json!({"attr0": a & 0xFFFF, "attr1": a >> 16, "png": if k < 2 { "mapscreen/arrow_h.png" } else { "mapscreen/arrow_v.png" }}));
    }
    out.put_json(
        "ui/mapscreen/area_maps.json",
        &json!({"table": hx(AREA_MAPS), "code": {"setup": "0x0802574C", "scroll": "0x08025D48", "bg": "0x08025B34", "step": "0x08024C54"},
                "marker": {"png": "mapscreen/marker.png", "rom": hx(MARKER_TILES), "frames": 4},
                "arrows": arrows, "records": recs}),
    );
}

/// BG0 of the map screen alone (index 0 transparent) with the L / R prompt states: 0 = no previous / next page
/// (the frame's own corner tiles), 1 = the prompt (first 4 tiles of the blob), 2 = pressed (tiles 4..7, loaded by
/// the step `0x08024C54` when L / R starts a page change)
pub fn frame_bg0(rom: &Rom, l: u8, r: u8) -> Surface {
    let tiles = rom.lz(FRAME_TILES).unwrap_or_default();
    let map = rom.lz(FRAME_MAP).unwrap_or_default();
    let lb = rom.lz(BTN_L).unwrap_or_default();
    let rb = rom.lz(BTN_R).unwrap_or_default();
    let half = |b: &[u8], st: u8, t: usize| -> Option<[u8; 64]> {
        let off = if st == 2 { 0x100 } else { 0 } + t * 64;
        let mut o = [0u8; 64];
        o.copy_from_slice(b.get(off..off + 64)?);
        Some(o)
    };
    let vram = |t: u16| -> Option<[u8; 64]> {
        match t {
            0x36F..=0x372 if r != 0 => half(&rb, r, t as usize - 0x36F),
            0x373..=0x376 if l != 0 => half(&lb, l, t as usize - 0x373),
            0x378.. => {
                let mut o = [0u8; 64];
                o.copy_from_slice(tiles.get((t as usize - 0x378) * 64..(t as usize - 0x378) * 64 + 64)?);
                Some(o)
            }
            _ => Some([0; 64]),
        }
    };
    let mut ents: Vec<u16> = map.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]).wrapping_add(BASE)).collect();
    ents.resize(1024.max(ents.len()), 0);
    if l != 0 {
        ents[0] = 0x373;
        ents[1] = 0x374;
        ents[32] = 0x375;
        ents[33] = 0x376;
    }
    if r != 0 {
        ents[28] = 0x36F;
        ents[29] = 0x370;
        ents[60] = 0x371;
        ents[61] = 0x372;
    }
    let mut s = Surface::new(240, 160);
    for ty in 0..20 {
        for tx in 0..30 {
            put_tile(&mut s, &vram, ents[(ty * 32 + tx) as usize], tx, ty, true);
        }
    }
    s
}

/// `ui/mapscreen/bg0_<l><r>.png` (the 9 prompt states of [`frame_bg0`]) and `grid.png` (BG3's tile 0x377)
pub fn frame_variants(rom: &Rom, out: &mut wf_rom::content::Content, pal: &[wf_rom::gfx::Rgb]) {
    for l in 0..3u8 {
        for r in 0..3u8 {
            out.put(format!("ui/mapscreen/bg0_{l}{r}.png"), frame_bg0(rom, l, r).png(pal, true));
        }
    }
    let fill: Vec<u8> = rom.bytes(FILL_TILE, 64).to_vec();
    out.put("ui/mapscreen/grid.png", Surface::from_vec(8, 8, fill).png(pal, true));
}
