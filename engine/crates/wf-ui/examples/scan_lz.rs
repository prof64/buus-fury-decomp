//! Research: list every webfoot-LZ container (u32 type 1|2, u32 size, stream) that decodes to exactly `size`.
//! scan_lz <rom>  ->  "addr size end" lines
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rom = std::fs::read(&a[1]).unwrap();
    let mut off = 0usize;
    while off + 8 < rom.len() {
        let typ = u32::from_le_bytes(rom[off..off + 4].try_into().unwrap());
        let size = u32::from_le_bytes(rom[off + 4..off + 8].try_into().unwrap());
        if (typ == 1 || typ == 2) && (16..0x40000).contains(&size) {
            if let Ok((d, end)) = wf_rom::lz::decompress(&rom, off) {
                if d.len() == size as usize && end > off + 8 {
                    println!("{:08x} {:x} {:08x}", 0x0800_0000 + off, size, 0x0800_0000 + end);
                    off = (end + 3) & !3;
                    continue;
                }
            }
        }
        off += 4;
    }
}
