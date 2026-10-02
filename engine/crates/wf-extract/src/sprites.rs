//! Sprite definitions → `sprites/<id>/{sheet.png, sprite.json}` + `sprites/index.json`.
//! Format: `tools/extract/GRAPHICS.md` §3.2 (Buu's Fury); Rust twin of `wf_sprites.py` + `sprites_wf_export.py`.

use crate::annotations::{Names, Timing};
use crate::sheet::Builder;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use wf_rom::content::Content;
use wf_rom::gfx::{obj_image, obj_size, palette, Image, Rgb};
use wf_rom::{hx, Rom};

pub const DEF_TABLE: u32 = 0x086B_6BD8;
pub const DEF_COUNT: u32 = 298;
pub const OBJ_PALETTE: u32 = 0x0805_652C;
pub const DIRS: [&str; 4] = ["down", "up", "left", "right"];

#[derive(Clone, Debug)]
pub struct Obj {
    pub dx: i32,
    pub dy: i32,
    pub attr0: u16,
    pub attr1: u16,
    pub gfx: u32,
    pub w: u32,
    pub h: u32,
    pub bpp: u32,
    pub hflip: bool,
    pub vflip: bool,
}

pub fn obj_at(rom: &Rom, p: u32) -> Option<Obj> {
    let a0 = rom.u16(p + 4);
    let a1 = rom.u16(p + 6);
    let (w, h) = obj_size(a0 >> 14, a1 >> 14)?;
    Some(Obj {
        dx: rom.s8(p) as i32,
        dy: rom.s8(p + 1) as i32,
        attr0: a0,
        attr1: a1,
        gfx: rom.u32(p + 8),
        w,
        h,
        bpp: if a0 & 0x2000 != 0 { 8 } else { 4 },
        hflip: a1 & 0x1000 != 0,
        vflip: a1 & 0x2000 != 0,
    })
}

fn valid_gfx(rom: &Rom, gp: u32) -> bool {
    if !rom.isptr(gp) || gp & 3 != 0 {
        return false;
    }
    let n = rom.u32(gp + 4);
    rom.u32(gp) <= 2 && n > 0 && n <= 0x2000
}

pub fn valid_frame(rom: &Rom, p: u32) -> bool {
    if !rom.isptr(p) || p & 3 != 0 {
        return false;
    }
    let n = rom.u32(p);
    if !(1..=16).contains(&n) {
        return false;
    }
    (0..n).all(|i| match obj_at(rom, p + 8 + 12 * i) {
        Some(o) => valid_gfx(rom, o.gfx) && o.attr0 & 0x0300 == 0,
        None => false,
    })
}

/// A frame: bounding box + OBJ pieces.
pub fn frame_objs(rom: &Rom, p: u32) -> Vec<Obj> {
    let n = rom.u32(p);
    (0..n).filter_map(|i| obj_at(rom, p + 8 + 12 * i)).collect()
}

pub struct Anim {
    pub addr: u32,
    pub nframes: usize,
    /// [dir][frame] → frame address
    pub dirs: [Vec<u32>; 4],
}

pub struct Def {
    pub index: u32,
    pub addr: u32,
    pub nanims: usize,
    pub flags: [u8; 3],
    pub shadow: [u8; 4],
    pub word12: u32,
    pub remap_addr: u32,
    pub boxes: [[i32; 4]; 4],
    pub anims: Vec<Option<Anim>>,
    /// halo attachment: (table address, anim index → [dir][frame] (dx, dy))
    pub attach: Option<(u32, HashMap<usize, Vec<Vec<(i8, i8)>>>)>,
}

pub fn parse_def(rom: &Rom, index: u32, dp: u32) -> Option<Def> {
    let na = rom.u8(dp) as usize + 1;
    let rp = rom.u32(dp + 4);
    if na > 64 || (rp != 0 && !rom.isptr(rp)) {
        return None;
    }
    let mut boxes = [[0i32; 4]; 4];
    for (d, b) in boxes.iter_mut().enumerate() {
        for (k, v) in b.iter_mut().enumerate() {
            *v = rom.s32(dp + 16 + 16 * d as u32 + 4 * k as u32);
        }
    }
    let mut anims = Vec::with_capacity(na);
    let mut attach_ptr = 0;
    for a in 0..na {
        let ap = rom.u32(dp + 0x50 + 4 * a as u32);
        if a == 0 && ap != 0 {
            attach_ptr = ap;
            anims.push(None);
            continue;
        }
        if ap == 0 {
            anims.push(None);
            continue;
        }
        let nf = rom.u32(ap) as usize;
        if nf == 0 || nf > 64 {
            return None;
        }
        let dirs: [Vec<u32>; 4] =
            std::array::from_fn(|d| (0..nf).map(|f| rom.u32(ap + 4 + 16 * f as u32 + 4 * d as u32)).collect());
        if dirs.iter().flatten().any(|&f| f != 0 && !valid_frame(rom, f)) {
            return None;
        }
        anims.push(Some(Anim { addr: ap, nframes: nf, dirs }));
    }
    if anims.iter().all(|a| a.is_none()) {
        return None;
    }
    let attach = if attach_ptr != 0 && rom.isptr(attach_ptr) {
        let mut offs = HashMap::new();
        for (k, a) in anims.iter().enumerate() {
            let Some(a) = a else { continue };
            let tp = rom.u32(attach_ptr.wrapping_add((4 * k as u32).wrapping_sub(4)));
            if rom.isptr(tp) {
                let v = (0..4u32)
                    .map(|d| {
                        (0..a.nframes as u32)
                            .map(|f| (rom.s8(tp + 8 * f + 2 * d), rom.s8(tp + 8 * f + 2 * d + 1)))
                            .collect()
                    })
                    .collect();
                offs.insert(k, v);
            }
        }
        Some((attach_ptr, offs))
    } else {
        None
    };
    Some(Def {
        index,
        addr: dp,
        nanims: na,
        flags: [rom.u8(dp + 1), rom.u8(dp + 2), rom.u8(dp + 3)],
        shadow: [rom.u8(dp + 8), rom.u8(dp + 9), rom.u8(dp + 10), rom.u8(dp + 11)],
        word12: rom.u32(dp + 12),
        remap_addr: rp,
        boxes,
        anims,
        attach,
    })
}

pub fn all_defs(rom: &Rom) -> Vec<Def> {
    (0..DEF_COUNT)
        .filter_map(|i| {
            let dp = rom.u32(DEF_TABLE + 4 * i);
            if rom.isptr(dp) {
                parse_def(rom, i, dp)
            } else {
                None
            }
        })
        .collect()
}

/// Every frame address used by a sprite definition (effects scanning excludes them).
pub fn known_frames(defs: &[Def]) -> BTreeSet<u32> {
    defs.iter().flat_map(|d| d.anims.iter().flatten().flat_map(|a| a.dirs.iter().flatten().copied())).collect()
}

/// Decompressed-gfx cache shared across entities.
#[derive(Default)]
pub struct GfxCache(HashMap<u32, Vec<u8>>);

impl GfxCache {
    pub fn get<'a>(&'a mut self, rom: &Rom, gp: u32) -> &'a [u8] {
        self.0.entry(gp).or_insert_with(|| rom.lz(gp).unwrap_or_default())
    }
}

/// Compose OBJ pieces (earlier OBJ on top). Returns (image, origin x, origin y).
pub fn compose(objs: &[Obj], images: &[Image]) -> (Image, i32, i32) {
    if objs.is_empty() {
        return (Image::new(1, 1), 0, 0);
    }
    let x0 = objs.iter().map(|o| o.dx).min().unwrap();
    let y0 = objs.iter().map(|o| o.dy).min().unwrap();
    let x1 = objs.iter().map(|o| o.dx + o.w as i32).max().unwrap();
    let y1 = objs.iter().map(|o| o.dy + o.h as i32).max().unwrap();
    let mut out = Image::new((x1 - x0) as u32, (y1 - y0) as u32);
    for (o, im) in objs.iter().zip(images).rev() {
        let mut im = im.clone();
        if o.hflip {
            im = im.flip_h();
        }
        if o.vflip {
            im = im.flip_v();
        }
        out.draw(&im, o.dx - x0, o.dy - y0);
    }
    (out, -x0, -y0)
}

pub fn render_frame(rom: &Rom, cache: &mut GfxCache, pal: &[Rgb], remap: Option<&[u8]>, fa: u32) -> (Image, i32, i32) {
    let objs = frame_objs(rom, fa);
    let ims: Vec<Image> = objs
        .iter()
        .map(|o| {
            let data = cache.get(rom, o.gfx);
            obj_image(data, o.w, o.h, o.bpp, pal, remap, 0)
        })
        .collect();
    compose(&objs, &ims)
}

/// Resolve the timing of one animation: decoded (from code) > curated lock > measured over curated default.
fn timing_for(names: &Names, timing: &Timing, idx: u32, ai: usize, a: &Anim) -> Option<Map<String, Value>> {
    if let Some(t) = timing.decoded(a.addr) {
        return Some(t);
    }
    let base = names.default_timing(idx, ai);
    if let Some(b) = &base {
        if b.get("lock").and_then(|v| v.as_bool()).unwrap_or(false) {
            return base;
        }
    }
    let meas = timing.measured(a.addr, a.nframes);
    match (base, meas) {
        (None, None) => None,
        (b, m) => {
            let mut t = b.unwrap_or_default();
            for (k, v) in m.unwrap_or_default() {
                t.insert(k, v);
            }
            Some(t)
        }
    }
}

pub fn extract(rom: &Rom, out: &mut Content) -> Vec<Def> {
    let pal = palette(rom, OBJ_PALETTE, 256);
    let defs = all_defs(rom);
    let mut names = Names::load();
    let timing = Timing::load();
    let mut cache = GfxCache::default();
    let mut index = Vec::new();
    for d in &defs {
        let info = names.entity(d.index);
        let remap: Option<Vec<u8>> = (d.remap_addr != 0).then(|| rom.bytes(d.remap_addr, 256).to_vec());
        let mut b = Builder::new();
        let mut refs: HashMap<u32, (usize, bool)> = HashMap::new();
        let mut anims = Map::new();
        for (ai, a) in d.anims.iter().enumerate() {
            let Some(a) = a else { continue };
            let aname = names.anim_name(d.index, ai);
            let t = timing_for(&names, &timing, d.index, ai, a);
            let durs: Vec<i64> = t
                .as_ref()
                .and_then(|t| t.get("durations"))
                .and_then(|v| v.as_array())
                .map(|v| v.iter().filter_map(|x| x.as_i64()).collect())
                .unwrap_or_default();
            let default = t.as_ref().and_then(|t| t.get("default")).and_then(|v| v.as_i64()).unwrap_or(8);
            let mut dirs = Map::new();
            for (di, fl) in a.dirs.iter().enumerate() {
                let mut seq = Vec::new();
                for (k, &fa) in fl.iter().enumerate() {
                    if fa == 0 {
                        out.warn(format!("sprite {}: anim {ai} dir {di} frame {k} is null", d.index));
                        continue;
                    }
                    let (fi, flip) = *refs.entry(fa).or_insert_with(|| {
                        let (im, ox, oy) = render_frame(rom, &mut cache, &pal, remap.as_deref(), fa);
                        b.add(&im, ox, oy, Some(fa))
                    });
                    let dur = durs.get(k).copied().unwrap_or(default);
                    let mut st = json!({"frame": fi, "duration_frames": dur, "flip": flip, "rom": hx(fa)});
                    if let Some((_, offs)) = &d.attach {
                        if let Some(att) = offs.get(&ai) {
                            let (x, y) = att[di][k];
                            st["attach"] = json!([x, y]);
                        }
                    }
                    seq.push(st);
                }
                dirs.insert(DIRS[di].into(), seq.into());
            }
            let t = t.unwrap_or_default();
            let observed: Vec<Value> = t
                .get("observed_sequences")
                .and_then(|v| v.as_array())
                .map(|v| v.iter().map(|p| p[0].clone()).collect())
                .unwrap_or_default();
            anims.insert(
                aname,
                json!({
                    "raw_index": ai,
                    "loop": t.get("loop").and_then(|v| v.as_bool()).unwrap_or(true),
                    "timing_source": t.get("source").and_then(|v| v.as_str()).unwrap_or("default"),
                    "observed_frame_orders": if observed.is_empty() { Value::Null } else { observed.into() },
                    "rom": hx(a.addr),
                    "directions": dirs,
                }),
            );
        }
        let (sheet, pos) = b.pack();
        let dir = format!("sprites/{}", info.slug);
        out.put(format!("{dir}/sheet.png"), sheet.to_png());
        let boxes: Map<String, Value> =
            d.boxes.iter().enumerate().map(|(i, bx)| (DIRS[i].to_string(), json!(bx))).collect();
        let mut e = json!({
            "id": info.slug,
            "sprite_index": d.index,
            "name": info.name,
            "category": info.category,
            "name_confidence": info.confidence,
            "notes": info.notes,
            "sheet": "sheet.png",
            "frames": b.frame_json(&pos),
            "animations": anims,
            "body_box": boxes,
            "footprint": [d.shadow[0], d.shadow[1]],
            "shadow": format!("shadow_{}x{}", d.shadow[2], d.shadow[3]),
            "header": {"nanims": d.nanims, "flags1": d.flags[0], "flags2": d.flags[1], "flags3": d.flags[2],
                       "shadow": d.shadow, "word12": d.word12},
            "rom": {"def": hx(d.addr), "remap_table": (d.remap_addr != 0).then(|| hx(d.remap_addr)),
                    "palette": hx(OBJ_PALETTE)},
        });
        if let Some((tab, _)) = &d.attach {
            e["attachment"] = json!({"kind": "halo", "effect": "halo", "table": hx(*tab)});
        }
        out.put_json(format!("{dir}/sprite.json"), &e);
        index.push(json!({"id": info.slug, "sprite_index": d.index, "name": e["name"], "category": e["category"]}));
    }
    out.put_json("sprites/index.json", &Value::Array(index));
    defs
}
