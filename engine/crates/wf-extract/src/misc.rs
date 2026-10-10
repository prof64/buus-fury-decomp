//! Portraits, supply / equipment icons and skill icons.
//!
//! * Portraits: table `0x086F395C`, 155 × webfoot-LZ 64×64 linear 8bpp (OBJ palette, index 0 transparent) →
//!   `portraits/<id>.png`.
//! * Supplies `0x083BD2F8` (83 × 52 B, +4 u16 name id), equipment `0x086F82EC` (162 × 44 B, +0 u16 name id):
//!   the first webfoot-LZ blob pointer in the record is a 16×16 linear 8bpp icon → `icons/{supply,equipment}/NNN.png`.
//! * Skills `0x0871DAF8` + 0x18·id (ids 1..23): +4 / +8 raw 8bpp 4×2-tile OBJ icons (normal / alternate) →
//!   `icons/skills/skill_NN.png`, `skill_NN_alt.png`.

use crate::annotations::Names;
use crate::sheet::slugify;
use crate::sprites::OBJ_PALETTE;
use serde_json::{json, Value};
use wf_rom::content::Content;
use wf_rom::gfx::{obj_image, palette, Image, Rgb};
use wf_rom::text::ustr_raw;
use wf_rom::{hx, Rom};

pub const PORTRAITS: u32 = 0x086F_395C;
pub const N_PORTRAITS: u32 = 155;
pub const SUPPLIES: (u32, u32, u32, u32) = (0x083B_D2F8, 83, 52, 4);
pub const EQUIPMENT: (u32, u32, u32, u32) = (0x086F_82EC, 162, 44, 0);
pub const SKILLS: u32 = 0x0871_DAF8;
pub const N_SKILLS: u32 = 23;

/// Linear (row-major) 8bpp bitmap; index 0 transparent unless `opaque`.
pub fn linear_image(d: &[u8], w: u32, h: u32, pal: &[Rgb], opaque: bool) -> Image {
    let mut im = Image::new(w, h);
    for (i, &c) in d.iter().take((w * h) as usize).enumerate() {
        if c != 0 || opaque {
            let p = pal[c as usize];
            im.px[i] = [p[0], p[1], p[2], 255];
        }
    }
    im
}

pub fn portraits(rom: &Rom, out: &mut Content, names: &Names) {
    let pal = palette(rom, OBJ_PALETTE, 256);
    let mut index = Vec::new();
    for i in 0..N_PORTRAITS {
        let p = rom.u32(PORTRAITS + 4 * i);
        let d = match rom.lz(p) {
            Ok(d) => d,
            Err(e) => {
                out.warn(format!("portrait {i}: {e}"));
                continue;
            }
        };
        // an 8bpp OBJ: index 0 is transparent (the dialogue box shows the screen through it, VERIFIED emulator)
        let im = linear_image(&d, 64, 64, &pal, false);
        let (name, conf) = names.portrait(i as usize);
        let id = match &name {
            Some(n) => format!("portrait_{i:03}_{}", slugify(n)),
            None => format!("portrait_{i:03}"),
        };
        out.put(format!("portraits/{id}.png"), im.to_png());
        index.push(json!({"id": id, "portrait_index": i, "name": name.unwrap_or(format!("Portrait {i}")),
                          "name_confidence": conf, "png": format!("{id}.png"),
                          "rom": {"table": hx(PORTRAITS + 4 * i), "gfx": hx(p)},
                          // CPU cycles the game spends decompressing it (the stream follows an 8-byte header; deccost.rs)
                          "decode_cycles": crate::deccost::decode_cycles(rom.data(), (p & 0x01FF_FFFF) as usize + 8)}));
    }
    out.put_json("portraits/index.json", &Value::Array(index));
}

fn valid_blob(rom: &Rom, p: u32) -> bool {
    rom.isptr(p) && p & 3 == 0 && matches!(rom.u32(p), 1 | 2) && matches!(rom.u32(p + 4), 128 | 256 | 512 | 1024)
}

pub fn item_icons(rom: &Rom, out: &mut Content) {
    let pal = palette(rom, OBJ_PALETTE, 256);
    let mut index = Vec::new();
    for (kind, (tbl, n, stride, name_off)) in [("supply", SUPPLIES), ("equipment", EQUIPMENT)] {
        for i in 0..n {
            let r = tbl + stride * i;
            let name = ustr_raw(rom, rom.u32(wf_rom::text::STRTAB + 4 * rom.u16(r + name_off) as u32), 81);
            let mut icon = None;
            for off in (0..stride).step_by(4) {
                let p = rom.u32(r + off);
                if !valid_blob(rom, p) {
                    continue;
                }
                let Ok(d) = rom.lz(p) else { continue };
                let side = match d.len() {
                    256 => 16,
                    1024 => 32,
                    _ => continue,
                };
                icon = Some((linear_image(&d, side, side, &pal, false), p));
                break;
            }
            let Some((im, p)) = icon else { continue };
            let path = format!("icons/{kind}/{i:03}.png");
            out.put(&path, im.to_png());
            index.push(json!({"kind": kind, "index": i, "name": name, "slug": format!("{kind}_{i:03}_{}", slugify(&name)),
                              "png": path, "rom": {"record": hx(r), "gfx": hx(p)}}));
        }
    }
    // skill icons
    for id in 1..=N_SKILLS {
        let r = SKILLS + 0x18 * id;
        for (k, suffix) in [(4u32, ""), (8, "_alt")] {
            let g = rom.u32(r + k);
            // indexed with the whole OBJ palette in palette-RAM order (index = OBJ colour, 0 transparent): the
            // frontend's palette fades need the colour index, not just the RGB (several OBJ entries share an RGB)
            let data = rom.bytes(g, 0x200);
            let mut idx = vec![0u8; 32 * 16];
            for t in 0..8usize {
                let ti = wf_rom::gfx::tile_indices(data, t, 8, 0);
                let (ox, oy) = ((t % 4) * 8, (t / 4) * 8);
                for i in 0..64 {
                    idx[(oy + i / 8) * 32 + ox + i % 8] = ti[i];
                }
            }
            let path = format!("icons/skills/skill_{id:02}{suffix}.png");
            out.put(&path, wf_rom::gfx::encode_png_indexed(32, 16, &idx, &pal, true));
            index.push(json!({"kind": "skill", "index": id, "variant": if suffix.is_empty() {"normal"} else {"alt"},
                              "png": path, "rom": {"record": hx(r), "gfx": hx(g)}}));
        }
    }
    out.put_json("icons/index.json", &Value::Array(index));
}
