//! Research: decode webfoot-LZ blobs as 8bpp linear images.
//! lzimg <rom> <palram.bin (1024 bytes) | -> <half: bg|obj> <out dir> <addr:width[:headerless_size]>...
use wf_rom::gfx::{bgr555, encode_png_indexed};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rom = std::fs::read(&a[1]).unwrap();
    let pal_raw = if a[2] == "-" { vec![0u8; 1024] } else { std::fs::read(&a[2]).unwrap() };
    let base = if a[3] == "obj" { 512 } else { 0 };
    let pal: Vec<_> = (0..256)
        .map(|i| bgr555(u16::from_le_bytes([pal_raw[base + 2 * i], pal_raw[base + 2 * i + 1]])))
        .collect();
    for spec in &a[5..] {
        let p: Vec<&str> = spec.split(':').collect();
        let addr = u32::from_str_radix(p[0].trim_start_matches("0x"), 16).unwrap();
        let w: u32 = p[1].parse().unwrap();
        let off = (addr & 0x1FF_FFFF) as usize;
        let data = if p.len() > 2 {
            // headerless stream: fabricate a header
            let n: u32 = p[2].parse().unwrap();
            let mut tmp = Vec::new();
            tmp.extend_from_slice(&1u32.to_le_bytes());
            tmp.extend_from_slice(&n.to_le_bytes());
            tmp.extend_from_slice(&rom[off..(off + 0x20000).min(rom.len())]);
            wf_rom::lz::decompress(&tmp, 0).map(|x| x.0)
        } else {
            wf_rom::lz::decompress(&rom, off).map(|x| x.0)
        };
        match data {
            Ok(d) => {
                let h = d.len() as u32 / w;
                let png = encode_png_indexed(w, h, &d[..(w * h) as usize], &pal, true);
                std::fs::write(format!("{}/{:08x}.png", a[4], addr), png).unwrap();
                println!("{addr:08x}: {} bytes -> {w}x{h}", d.len());
            }
            Err(e) => println!("{addr:08x}: {e}"),
        }
    }
}
