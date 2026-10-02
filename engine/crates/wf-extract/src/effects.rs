//! Code-driven effect / projectile frames (`tools/extract/wf_effects.py`, `misc_export.effects`), ground
//! shadows and the halo (`misc_export.buu_overlays`) → `effects/<id>/{sheet.png, effect.json}`.

use crate::sheet::Builder;
use crate::sprites::{compose, Obj, OBJ_PALETTE};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use wf_rom::content::Content;
use wf_rom::gfx::{obj_image, obj_size, palette, Image, Rgb};
use wf_rom::{hx, Rom};

pub const SHADOWS: u32 = 0x086B_759C;
pub const HALO: u32 = 0x0807_25F0;

struct Rec {
    addr: u32,
    ox: i32,
    oy: i32,
    obj: Obj,
}

/// Scan the ROM for runs of 12-byte raw-OBJ records (same heuristics as `wf_effects.scan`).
fn scan(rom: &Rom, known: &BTreeSet<u32>) -> Vec<Vec<Rec>> {
    let d = rom.data();
    let nwords = d.len() / 4;
    let mut runs: Vec<Vec<Rec>> = Vec::new();
    for i in 2..nwords {
        let gp = rom.u32(4 * i as u32);
        if !rom.isptr(gp) {
            continue;
        }
        let o = ((i - 2) * 4) as u32;
        let a0 = rom.u16(o + 4);
        let a1 = rom.u16(o + 6);
        let Some((w, h)) = obj_size(a0 >> 14, a1 >> 14) else { continue };
        if (a0 & 0x1F00 & !0x2000) != 0 || (a1 & 0x0FFF) != 0 {
            continue;
        }
        if (rom.u8(o + 2) as u32, rom.u8(o + 3) as u32) != (w, h) || known.contains(&(0x0800_0000 + o)) {
            continue;
        }
        let t = rom.u32(gp);
        let n = rom.u32(gp + 4);
        if (t == 1 || t == 2) && (n == w * h || n == w * h / 2) {
            continue;
        }
        if gp & 3 != 0 || o < 0x10_0000 || gp < 0x0810_0000 {
            continue;
        }
        let (ox, oy) = (rom.s8(o) as i32, rom.s8(o + 1) as i32);
        if ox.abs() > w as i32 + 8 || oy.abs() > h as i32 + 24 {
            continue;
        }
        let rec = Rec {
            addr: 0x0800_0000 + o,
            ox,
            oy,
            obj: Obj {
                dx: -ox,
                dy: -oy,
                attr0: a0,
                attr1: a1,
                gfx: gp,
                w,
                h,
                bpp: if a0 & 0x2000 != 0 { 8 } else { 4 },
                hflip: a1 & 0x1000 != 0,
                vflip: a1 & 0x2000 != 0,
            },
        };
        match runs.last_mut() {
            Some(r) if rec.addr - r.last().unwrap().addr == 12 => r.push(rec),
            _ => runs.push(vec![rec]),
        }
    }
    runs
}

/// colorsys.rgb_to_hls
fn rgb_to_hls(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let sumc = maxc + minc;
    let rangec = maxc - minc;
    let l = sumc / 2.0;
    if minc == maxc {
        return (0.0, l, 0.0);
    }
    let s = if l <= 0.5 { rangec / sumc } else { rangec / (2.0 - maxc - minc) };
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), l, s)
}

/// Rough dominant colour word used to label uncurated effects (`misc_export._colour_word`).
fn colour_word(frames: &[Image]) -> &'static str {
    let (mut t, mut n) = ([0u64; 3], 0u64);
    for im in frames {
        for p in &im.px {
            if p[3] != 0 {
                t[0] += p[0] as u64;
                t[1] += p[1] as u64;
                t[2] += p[2] as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        return "empty";
    }
    let f = |v: u64| v as f64 / n as f64 / 255.0;
    let (h, l, s) = rgb_to_hls(f(t[0]), f(t[1]), f(t[2]));
    if s < 0.18 {
        return if l > 0.75 {
            "white"
        } else if l > 0.35 {
            "grey"
        } else {
            "dark"
        };
    }
    let hue = h * 360.0;
    for (lim, w) in [
        (15.0, "red"),
        (45.0, "orange"),
        (70.0, "yellow"),
        (160.0, "green"),
        (200.0, "cyan"),
        (255.0, "blue"),
        (290.0, "purple"),
        (335.0, "pink"),
        (361.0, "red"),
    ] {
        if hue < lim {
            return w;
        }
    }
    "mixed"
}

/// (image, anchor x, anchor y, frame rom address, step `obj` = record geometry [ox, oy, w, h, hflip, vflip])
pub type FxFrame = (Image, i32, i32, Option<u32>, Option<Value>);

/// One-animation entity (`misc_export.single_entity`).
#[allow(clippy::too_many_arguments)]
pub fn single_entity(
    out: &mut Content,
    dir: &str,
    file: &str,
    id: &str,
    name: &str,
    category: &str,
    conf: &str,
    frames: &[FxFrame],
    duration: i64,
    anim_name: &str,
    extra: Value,
) -> Value {
    let mut b = Builder::new();
    let mut seq = Vec::new();
    for (im, ox, oy, rom, obj) in frames {
        let (fi, flip) = b.add(im, *ox, *oy, *rom);
        let mut st = json!({"frame": fi, "duration_frames": duration, "flip": flip, "rom": rom.map(hx)});
        if let Some(o) = obj {
            st["obj"] = o.clone();
        }
        seq.push(st);
    }
    let (sheet, pos) = b.pack();
    out.put(format!("{dir}/sheet.png"), sheet.to_png());
    let mut e = json!({
        "id": id, "name": name, "category": category, "name_confidence": conf,
        "sheet": "sheet.png", "frames": b.frame_json(&pos),
        "animations": {anim_name: {"raw_index": 0, "loop": true, "timing_source": "default",
                                   "directions": {"any": seq}}},
    });
    if let (Some(m), Some(x)) = (e.as_object_mut(), extra.as_object()) {
        for (k, v) in x {
            m.insert(k.clone(), v.clone());
        }
    }
    out.put_json(format!("{dir}/{file}"), &e);
    e
}

pub fn extract(rom: &Rom, out: &mut Content, known: &BTreeSet<u32>) {
    let pal: Vec<Rgb> = palette(rom, OBJ_PALETTE, 256);
    let fx_names: serde_json::Map<String, Value> =
        serde_json::from_str(include_str!("../../../annotations/effect_names.json")).expect("effect_names.json");
    let mut index = Vec::new();
    for run in scan(rom, known) {
        let key = format!("{:08X}", run[0].addr);
        let frames: Vec<FxFrame> = run
            .iter()
            .map(|r| {
                let n = (r.obj.w * r.obj.h * r.obj.bpp / 8) as usize;
                let im = obj_image(rom.bytes(r.obj.gfx, n), r.obj.w, r.obj.h, r.obj.bpp, &pal, None, 0);
                let (im, ox, oy) = compose(std::slice::from_ref(&r.obj), &[im]);
                (im, ox, oy, Some(r.addr), Some(json!([r.ox, r.oy, r.obj.w, r.obj.h, r.obj.hflip, r.obj.vflip])))
            })
            .collect();
        let imgs: Vec<Image> = frames.iter().map(|f| f.0.clone()).collect();
        let info = fx_names.get(&key).cloned().unwrap_or(Value::Null);
        let name = info["name"].as_str().map(String::from).unwrap_or_else(|| {
            format!("Effect {key} ({}, {}x{}, {} frames)", colour_word(&imgs), run[0].obj.w, run[0].obj.h, run.len())
        });
        let notes = info["notes"].as_str().unwrap_or(
            "frames in ROM order; grouping into per-direction animations is done by game code, durations are a default",
        );
        let id = format!("fx_{}", key.to_lowercase());
        single_entity(
            out,
            &format!("effects/{id}"),
            "effect.json",
            &id,
            &name,
            info["category"].as_str().unwrap_or("effect"),
            info["confidence"].as_str().unwrap_or("low"),
            &frames,
            4,
            "default",
            json!({"rom": {"frame_records": hx(run[0].addr), "count": run.len(), "palette": hx(OBJ_PALETTE),
                           "gfx_format": "raw (uncompressed) 8bpp tiles, shared OBJ palette"},
                   "notes": notes}),
        );
        index.push(json!({"id": id, "name": name, "kind": "fx"}));
    }
    // ground shadows (table 0x086B759C, looked up by the sprite header shadow bytes [2], [3])
    for i in 0..4u32 {
        let a = SHADOWS + 16 * i;
        let (w, h) = (rom.u8(a), rom.u8(a + 1));
        let g = rom.u32(a + 4);
        let (a0, a1) = (rom.u16(a + 12), rom.u16(a + 14));
        let Some((ow, oh)) = obj_size(a0 >> 14, a1 >> 14) else { continue };
        let bpp = if a0 & 0x2000 != 0 { 8 } else { 4 };
        let im = obj_image(rom.bytes(g, (ow * oh * bpp / 8) as usize), ow, oh, bpp, &pal, None, 0);
        let id = format!("shadow_{w}x{h}");
        let name = format!("Ground shadow {w}x{h}");
        single_entity(
            out,
            &format!("effects/{id}"),
            "effect.json",
            &id,
            &name,
            "effect",
            "high",
            &[(im, 0, 0, Some(g), None)],
            4,
            "default",
            json!({"shadow_size": [w, h], "rom": {"table": hx(a), "gfx": hx(g), "obj": format!("{ow}x{oh} {bpp}bpp")},
                   "notes": "drawn at the entity position (anchor), behind the body, for sprites whose `shadow` is this id"}),
        );
        index.push(json!({"id": id, "name": name, "kind": "shadow"}));
    }
    let im = obj_image(rom.bytes(HALO, 128), 16, 8, 8, &pal, None, 0);
    single_entity(
        out,
        "effects/halo",
        "effect.json",
        "halo",
        "Halo",
        "effect",
        "high",
        &[(im, 0, 0, Some(HALO), None)],
        4,
        "default",
        json!({"rom": {"gfx": hx(HALO), "obj": "16x8 8bpp"},
               "notes": "drawn at entity position + the step `attach` offset of sprites with an attachment"}),
    );
    index.push(json!({"id": "halo", "name": "Halo", "kind": "attachment"}));
    crate::fx_extra::extract(rom, out, &mut index);
    out.put_json("effects/index.json", &Value::Array(index));
}
