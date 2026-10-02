//! The content files are a faithful, self-sufficient description: map.json gids, tileset PNGs and
//! collision.json decode back to exactly what the renderer used.

use wf_maps::{collision, render, rom_maps, tiled, Ctx};
use wf_rom::{content::Content, gfx, testutil};

#[test]
fn content_roundtrip() {
    let Some(rom) = testutil::rom() else { return };
    let mut c = Content::new();
    wf_maps::extract(&rom, &mut c).unwrap();
    let index = c.json("maps/index.json").unwrap();
    let maps = index["maps"].as_array().unwrap();
    assert_eq!(maps.len(), 427);
    let areas = c.json("areas.json").unwrap();
    assert_eq!(areas["areas"].as_array().unwrap().len(), 452);
    let mut ctx = Ctx::new(&rom);
    for m in maps {
        let id = m["id"].as_str().unwrap();
        let mj = c.json(&format!("maps/{id}/map.json")).unwrap();
        let ts_id = m["tileset"].as_str().unwrap();
        let tsj = c.json(&format!("maps/tilesets/{ts_id}.json")).unwrap();
        let tsimg = gfx::decode_png(c.get(&format!("maps/tilesets/{ts_id}.png")).unwrap()).unwrap();
        assert_eq!(tsj["imagewidth"].as_u64().unwrap() as u32, tsimg.w);
        let hdr = rom_maps::header(&rom, u32::from_str_radix(m["header"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap());
        let mw = mj["width"].as_u64().unwrap() as u32;
        for l in mj["layers"].as_array().unwrap() {
            let bg: usize = l["name"].as_str().unwrap()[2..].parse().unwrap();
            let r = rom_maps::resource(&rom, hdr.res[bg]).unwrap();
            let lay = render::decode_layer(&mut ctx, &r);
            let gids = tiled::decode_gids(l["data"].as_str().unwrap());
            for y in 0..lay.h {
                for x in 0..lay.w {
                    let se = lay.entries[(y * lay.w + x) as usize];
                    let g = gids[(y * mw + x) as usize];
                    assert_eq!(g & 0x3FFF_FFFF, (se & 0x3FF) as u32 + 1, "{id} bg{bg}");
                    assert_eq!(g >> 31 == 1, se & 0x400 != 0);
                    assert_eq!((g >> 30) & 1 == 1, se & 0x800 != 0);
                }
            }
            // the layer PNG equals tiles from the tileset PNG placed by the gids
            let png = gfx::decode_png(c.get(&format!("maps/{id}/bg{bg}.png")).unwrap()).unwrap();
            for (ty, tx) in [(0u32, 0u32), (lay.h / 2, lay.w / 2), (lay.h - 1, lay.w - 1)] {
                let g = gids[(ty * mw + tx) as usize];
                let t = (g & 0x3FFF_FFFF) - 1;
                for i in 0..64u32 {
                    let (x, y) = (i % 8, i / 8);
                    let sx = if g >> 31 == 1 { 7 - x } else { x };
                    let sy = if (g >> 30) & 1 == 1 { 7 - y } else { y };
                    let p = tsimg.get((t % 16) * 8 + sx, (t / 16) * 8 + sy);
                    assert_eq!(p, png.get(tx * 8 + x, ty * 8 + y), "{id} bg{bg} tile {t}");
                }
            }
        }
        let cj = c.json(&format!("maps/{id}/collision.json")).unwrap();
        let g = collision::grids(&mut ctx, &hdr);
        assert_eq!(collision::unrle(cj["walk"].as_str().unwrap(), 65536), g.walk);
        assert_eq!(collision::unrle(cj["shot"].as_str().unwrap(), 65536), g.shot);
    }
}
