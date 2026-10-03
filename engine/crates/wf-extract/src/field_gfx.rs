//! Field object graphics (the overworld's coins, item pickups, map props / breakables, level-sign labels) and the
//! collision boxes the game derives from their pixels. Decoded from the game's draw code (re/buu/notes.md
//! "Field: object graphics"); every sheet keeps its frames untrimmed and in a fixed index order, so a frame index
//! is a stable id (`FrameSel::Frame { index, flip: false }`):
//!
//! * `effects/coin_<size>` (size 0..3): the 4 animation cels of the coin def `[0x086FC250 + 4 size]` (+0x18, raw 8bpp
//!   w×h, OBJ palette); anchor (w/2, h): drawn at (x, y - z) of the coin (`0x08018284`).
//! * `effects/field_items`: frame = supply index (0..82) for supplies, `EQUIPMENT_FRAME0 + index` for equipment; the
//!   16×16 icons (record's LZ blob, as `icons/`), anchor (0, 0) = top-left.
//! * `effects/props_<map id>`: frame k = map header prop k (+0x24 count, +0x28 0x18-byte records; graphics = the
//!   header's +0x2C LZ blob at tile offset record +0x14 low half × 32 bytes, OBJ shape from record +0x10); anchor (0, 0)
//!   = the prop rect's top-left. `maps/<id>/objects.json` props gain `gfx`, `visible`, `priority`, `pixel_bbox` and
//!   `hit_box` (breakables: bbox of the non-zero pixels, top raised to the middle — the game reads VRAM, `0x0800DB64`).
//! * `effects/field_labels`: the level-sign labels of the map sprites (area list +0x20, `0x0800C300`): "%d" of def
//!   +6 in font `0x0806F4F4`, colour def +5, shadow colour 1 at (+1, +1), centred in a 32×16 surface; anchor (16, 8).
//!   objects.json map_sprite objects gain `label`.
//! * `icons/index.json` supply / equipment entries gain `sub_box` (the dropped item's collision box inside the icon:
//!   pixel bbox with the top raised to the middle, `0x0800DB64`).

use crate::misc::{linear_image, EQUIPMENT, SUPPLIES};
use crate::sheet::pack;
use crate::sprites::OBJ_PALETTE;
use serde_json::{json, Value};
use wf_rom::content::Content;
use wf_rom::gfx::{obj_image, obj_size, palette, Image, Rgb};
use wf_rom::{hx, Rom};

pub const COIN_DEFS: u32 = 0x086F_C250;
pub const EQUIPMENT_FRAME0: u32 = 100;
pub const LABEL_FONT: u32 = 0x0806_F4F4;

/// An untrimmed sheet: frames in the given order, anchor per frame.
fn plain_sheet(out: &mut Content, dir: &str, id: &str, name: &str, frames: &[(Image, i32, i32)], extra: Value) {
    let imgs: Vec<&Image> = frames.iter().map(|f| &f.0).collect();
    let (sheet, pos) = pack(&imgs, 512, 1);
    out.put(format!("{dir}/sheet.png"), sheet.to_png());
    let fr: Vec<Value> = frames
        .iter()
        .zip(&pos)
        .map(|((im, ax, ay), p)| json!({"x": p.0, "y": p.1, "w": im.w, "h": im.h, "anchor": {"x": ax, "y": ay}}))
        .collect();
    let steps: Vec<Value> = (0..frames.len()).map(|k| json!({"frame": k, "duration_frames": 8, "flip": false})).collect();
    let mut e = json!({
        "id": id, "name": name, "category": "field", "name_confidence": "high", "sheet": "sheet.png", "frames": fr,
        "animations": {"default": {"raw_index": 0, "loop": true, "timing_source": "code (see notes)", "directions": {"any": steps}}},
    });
    if let (Some(m), Some(x)) = (e.as_object_mut(), extra.as_object()) {
        for (k, v) in x {
            m.insert(k.clone(), v.clone());
        }
    }
    out.put_json(format!("{dir}/effect.json"), &e);
}

/// Bounding box of the opaque pixels (exclusive max) with the top raised to the middle (`0x0800DB64` + caller).
pub fn lower_half_bbox(im: &Image) -> Option<[i32; 4]> {
    let (x0, y0, x1, y1) = im.bbox()?;
    let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    Some([x0, (y0 + y1) >> 1, x1, y1])
}

fn icon(rom: &Rom, kind: (u32, u32, u32, u32), i: u32, pal: &[Rgb]) -> Option<(Image, u32)> {
    let (tbl, _, stride, _) = kind;
    let r = tbl + stride * i;
    for off in (0..stride).step_by(4) {
        let p = rom.u32(r + off);
        if !(rom.isptr(p) && p & 3 == 0 && matches!(rom.u32(p), 1 | 2) && rom.u32(p + 4) == 256) {
            continue;
        }
        let Ok(d) = rom.lz(p) else { continue };
        if d.len() == 256 {
            return Some((linear_image(&d, 16, 16, pal, false), p));
        }
    }
    None
}

/// `effects` category: coins and the item icon sheet.
pub fn effects(rom: &Rom, out: &mut Content) {
    let pal = palette(rom, OBJ_PALETTE, 256);
    for size in 0..4u32 {
        let d = rom.u32(COIN_DEFS + 4 * size);
        let (w, h) = (rom.u8(d) as u32, rom.u8(d + 1) as u32);
        let n = rom.u32(d + 0x10) as usize;
        let a0 = rom.u16(d + 0x14);
        let bpp = if a0 & 0x2000 != 0 { 8 } else { 4 };
        let frames: Vec<(Image, i32, i32)> = (0..4u32)
            .map(|k| (obj_image(rom.bytes(d + 0x18 + k * n as u32, n), w, h, bpp, &pal, None, 0), (w / 2) as i32, h as i32))
            .collect();
        let id = format!("coin_{size}");
        plain_sheet(
            out,
            &format!("effects/{id}"),
            &id,
            &format!("Zenie coin, size {size} ({} Z)", rom.u32(d + 0xC)),
            &frames,
            json!({
                "value": rom.u32(d + 0xC), "sprite_size": [w, h], "box_size": [rom.u8(d + 2), rom.u8(d + 3)],
                "box_y_offset": rom.s32(d + 8), "shadow": format!("shadow_{}x{}", rom.u8(d + 2), rom.u8(d + 3)),
                "rom": {"def": hx(d), "gfx": hx(d + 0x18), "palette": hx(OBJ_PALETTE)},
                "notes": "cel = ((phase + draws) & 0xFF) >> 3, 0 when > 3 (phase = the ctor's next(); +1 per draw); drawn at (x, y - z) of the coin (anchor = sprite (w/2, h)); not drawn after 480 frames of age while frame counter & 4; shadow effect at the pickup box's top-left (x - w/2, y + box_y_offset)",
            }),
        );
    }
    let mut frames: Vec<(Image, i32, i32)> = Vec::new();
    let mut map = Vec::new();
    let blank = Image::new(16, 16);
    for (kind, def, base) in [("supply", SUPPLIES, 0u32), ("equipment", EQUIPMENT, EQUIPMENT_FRAME0)] {
        for i in 0..def.1 {
            let k = (base + i) as usize;
            while frames.len() <= k {
                frames.push((blank.clone(), 0, 0));
            }
            if let Some((im, p)) = icon(rom, def, i, &pal) {
                frames[k] = (im, 0, 0);
                map.push(json!({"kind": kind, "index": i, "frame": k, "gfx": hx(p)}));
            }
        }
    }
    plain_sheet(
        out,
        "effects/field_items",
        "field_items",
        "Item icons (dropped / map items, item-get)",
        &frames,
        json!({"frame_of": {"supply": "index", "equipment": format!("{EQUIPMENT_FRAME0} + index")}, "icons": map,
               "notes": "16x16, anchor = top-left; dropped item: at (x0, y0 - (z >> 10)) of its box + shadow_10x4 at (x0, y0 + 10); map item: at the def (x, y); item-get: above the player"}),
    );
}

/// `items` category: the dropped items' collision sub-boxes in `icons/index.json`.
pub fn icon_boxes(rom: &Rom, out: &mut Content) {
    let pal = palette(rom, OBJ_PALETTE, 256);
    let Some(Value::Array(mut idx)) = out.json("icons/index.json") else { return };
    for e in idx.iter_mut() {
        let def = match e["kind"].as_str() {
            Some("supply") => SUPPLIES,
            Some("equipment") => EQUIPMENT,
            _ => continue,
        };
        let Some(i) = e["index"].as_u64() else { continue };
        if let Some(b) = icon(rom, def, i as u32, &pal).and_then(|(im, _)| lower_half_bbox(&im)) {
            e["sub_box"] = json!(b);
        }
    }
    out.put_json("icons/index.json", &Value::Array(idx));
}

/// A font glyph's columns (`docs/ui.md` §2): bit k of column c = row k.
fn glyph(rom: &Rom, font: u32, ch: u16) -> Option<Vec<u16>> {
    let t = rom.u32(font);
    let g = rom.u16(t + 2 * ch as u32);
    if g == 0 {
        return None;
    }
    let a = t + 2 * g as u32;
    let w = rom.u16(a) as u32;
    Some((0..w).map(|i| rom.u16(a + 2 + 2 * i)).collect())
}

/// The map sprite's label surface (`0x0800C298`): 32×16, the number centred, shadow (colour 1) at (+1, +1).
pub fn label_image(rom: &Rom, text: &str, colour: u8, pal: &[Rgb]) -> Image {
    let glyphs: Vec<Vec<u16>> = text.encode_utf16().filter_map(|c| glyph(rom, LABEL_FONT, c)).collect();
    let total: i32 = glyphs.iter().map(|g| g.len() as i32 + 1).sum();
    let x0 = (32 - total) / 2;
    let mut idx = [0u8; 32 * 16];
    for (pass, col) in [(1i32, 1u8), (0, colour)] {
        let mut x = x0;
        for g in &glyphs {
            for (c, bits) in g.iter().enumerate() {
                for r in 0..16 {
                    if bits >> r & 1 != 0 {
                        let (px, py) = (x + c as i32 + pass, r + pass);
                        if (0..32).contains(&px) && (0..16).contains(&py) {
                            idx[(py * 32 + px) as usize] = col;
                        }
                    }
                }
            }
            x += g.len() as i32 + 1;
        }
    }
    linear_image(&idx, 32, 16, pal, false)
}

/// `show_title_text`'s strings (table 0x0806BCE8) measured in their font (`[0x08753A1C]`, `0x0803FFE8`: a code
/// >= 0xFF is font +4 + 1 wide, others glyph width + 1) -> `world/title_widths.json` (the object's tiles: vram.rs)
fn title_widths(rom: &Rom, out: &mut Content) {
    let font = rom.u32(0x0875_3A1C);
    let (glyphs, wide) = (rom.u32(font), rom.u32(font + 4));
    let mut v = Vec::new();
    // the string handle table: entries up to the first non-pointer after 0x800 (handles go past 1500)
    for k in 0..0x2000u32 {
        let p = rom.u32(0x0806_BCE8 + 4 * k);
        if !rom.isptr(p) {
            if k > 0x800 {
                break;
            }
            v.push(0);
            continue;
        }
        let mut w = 0u32;
        let mut a = p;
        for _ in 0..256 {
            let c = rom.u16(a) as u32;
            if c == 0 {
                break;
            }
            a += 2;
            w += if c >= 0xFF { wide + 1 } else { rom.u16(glyphs + 2 * rom.u16(glyphs + 2 * c) as u32) as u32 + 1 };
        }
        v.push(w);
    }
    out.put_json("world/title_widths.json", &json!(v));
}

/// `world` category (after wf-world): prop sheets + boxes, label sheet, objects.json annotations.
pub fn world(rom: &Rom, out: &mut Content) {
    title_widths(rom, out);
    let pal = palette(rom, OBJ_PALETTE, 256);
    let areas = wf_maps::rom_maps::areas(rom);
    let maps = wf_maps::maps(rom, &areas);
    let mut labels: Vec<(Image, i32, i32)> = Vec::new();
    let mut label_keys: Vec<(u8, i32)> = Vec::new();
    for m in &maps {
        let path = format!("maps/{}/objects.json", m.id);
        let Some(Value::Object(mut oj)) = out.json(&path) else { continue };
        let h = m.header.addr;
        let n = rom.u32(h + 0x24).min(256);
        let recs = rom.u32(h + 0x28);
        let g = rom.u32(h + 0x2C);
        let alt = rom.u32(h + 0x3C) != 0;
        // the props object's object tiles (`0x0800D15A`: graphics header +4; created when the map has props)
        if n > 0 && rom.isptr(g) {
            oj.insert("prop_tile_bytes".into(), json!(rom.u32(g + 4)));
        }
        let blob = if n > 0 && rom.isptr(g) { rom.lz(g).ok() } else { None };
        let mut frames: Vec<(Image, i32, i32)> = Vec::new();
        let mut props = oj.get("props").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        for k in 0..n {
            let r = recs + 0x18 * k;
            let attr = rom.u32(r + 0x10);
            let (a0, a1) = ((attr & 0xFFFF) as u16, (attr >> 16) as u16);
            let (w, hh) = obj_size(a0 >> 14, a1 >> 14).unwrap_or((8, 8));
            let bpp = if a0 & 0x2000 != 0 { 8 } else { 4 };
            let tile = rom.u16(r + 0x14) as usize;
            let nbytes = (w * hh * bpp / 8) as usize;
            let im = match &blob {
                Some(b) if tile * 32 + nbytes <= b.len() => obj_image(&b[tile * 32..tile * 32 + nbytes], w, hh, bpp, &pal, None, 0),
                _ => Image::new(w, hh),
            };
            let (x0, y0) = (rom.u32(r) as i32, rom.u32(r + 4) as i32);
            let bb = im.bbox().map(|(a, b, c, d)| [a as i32 + x0, b as i32 + y0, c as i32 + x0, d as i32 + y0]);
            let hit = lower_half_bbox(&im).map(|b| [b[0] + x0, b[1] + y0, b[2] + x0, b[3] + y0]);
            if let Some(Value::Object(p)) = props.get_mut(k as usize) {
                p.insert("gfx".into(), json!({"effect": format!("props_{}", m.id), "frame": k}));
                p.insert("visible".into(), json!(rom.u8(r + 0x17) != 0));
                p.insert("priority".into(), json!((rom.u8(r + 0x16) >> 4) & 3));
                p.insert("pixel_bbox".into(), json!(bb));
                p.insert("hit_box".into(), json!(hit));
                if alt {
                    p.insert("alt_class".into(), json!(true));
                }
            }
            frames.push((im, 0, 0));
        }
        if !frames.is_empty() {
            let id = format!("props_{}", m.id);
            plain_sheet(
                out,
                &format!("effects/{id}"),
                &id,
                &format!("Map props of {}", m.id),
                &frames,
                json!({"rom": {"header": hx(h), "records": hx(recs), "gfx": hx(g), "palette": hx(OBJ_PALETTE)},
                       "notes": "frame k = prop k, anchor = the prop rect's top-left; drawn only while the prop is visible (record +0x17 != 0, scripts / breakables change it); a shaking breakable adds its display offset"}),
            );
            oj.insert("props".into(), Value::Array(props));
        }
        // map sprites: level-sign labels
        if let Some(Value::Array(areas)) = oj.get_mut("areas") {
            for a in areas.iter_mut() {
                let Some(Value::Array(objs)) = a.get_mut("objects") else { continue };
                for o in objs.iter_mut() {
                    if o["type"] != "map_sprite" {
                        continue;
                    }
                    let p = &o["params"];
                    let colour = p[0].as_u64().unwrap_or(0) as u8;
                    let num = p[1].as_i64().unwrap_or(0) as i32;
                    let text = if colour == 2 { String::new() } else { num.to_string() };
                    let key = (colour, num);
                    let f = match label_keys.iter().position(|k| *k == key) {
                        Some(f) => f,
                        None => {
                            label_keys.push(key);
                            labels.push((label_image(rom, &text, colour, &pal), 16, 8));
                            labels.len() - 1
                        }
                    };
                    o["label"] = json!({"effect": "field_labels", "frame": f, "text": text, "colour": colour, "prop": o["sprite"]});
                }
            }
        }
        out.put_json(path, &Value::Object(oj));
    }
    if !labels.is_empty() {
        plain_sheet(
            out,
            "effects/field_labels",
            "field_labels",
            "Level-sign labels (map sprites)",
            &labels,
            json!({"font": hx(LABEL_FONT), "palette": hx(OBJ_PALETTE),
                   "notes": "32x16 label, anchor (16, 8): drawn at (x, y + (sine[fc & 0xFF] >> 14)) (sine table 0x087B6EEC, 16.16; fc = frame counter 0x03000F94), only while its prop (`label.prop` = map sprite def +4) is visible"}),
        );
    }
}
