
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
