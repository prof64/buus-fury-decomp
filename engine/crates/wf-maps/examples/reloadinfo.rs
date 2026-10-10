//! `reloadinfo <header addr hex>`: the compressed streams the map loader decompresses for a map header, with
//! the decompressor's cycles (wf_rom::deccost), to compare with an emulator profile of an area reload
//! (work/earth/timing/leaveprof.py).
use wf_maps::rom_maps;
use wf_rom::deccost::decode_cycles_to;

fn main() {
    let a = u32::from_str_radix(std::env::args().nth(1).expect("header").trim_start_matches("0x"), 16).unwrap();
    let rom = wf_rom::testutil::rom().expect("rom");
    let h = rom_maps::header(&rom, a);
    let cyc = |p: u32, ew: bool| decode_cycles_to(rom.data(), (p & 0x01FF_FFFF) as usize, ew);
    for (k, &r) in h.res.iter().enumerate() {
        let Some(res) = rom_maps::resource(&rom, r) else { continue };
        println!("layer {k} kind {} rate {:?} offset {:?} {}x{}", res.kind, res.rate, res.offset, res.cols, res.rows);
        for (i, &c) in res.chunks.iter().enumerate() {
            if rom.isptr(c) {
                println!("   chunk {i} stream {:08x} cycles {:?}", c + 8, cyc(c + 8, true));
            } else {
                println!("   chunk {i} none");
            }
        }
    }
    println!("tile list stream {:08x} cycles {:?}", h.tile_list + 8, cyc(h.tile_list + 8, false));
    let list = rom.lz(h.tile_list).unwrap_or_default();
    let n = (list.len() / 2).min(1024);
    let mut e: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([list[2 * i], list[2 * i + 1]])).collect();
    for i in 1..n {
        e[i] = e[i].wrapping_add(e[i - 1]).wrapping_add(1);
    }
    let mut last = u32::MAX;
    let mut total = 0;
    for &x in &e {
        let c = x as u32 >> 8;
        if c != last {
            last = c;
            let p = rom.u32(h.tile_chunks + 4 * c);
            let cy = cyc(p + 8, true);
            total += cy.unwrap_or(0);
            println!("tile chunk {c} stream {:08x} cycles {:?}", p + 8, cy);
        }
    }
    println!("tile chunks total {total}");
    for an in &h.anims {
        if let Some(&(_, p)) = an.frames.first() {
            println!("anim frame 0 stream {:08x} cycles {:?} ({} tiles)", p, cyc(p, false), an.ntiles);
        }
    }
}
