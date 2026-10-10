
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
