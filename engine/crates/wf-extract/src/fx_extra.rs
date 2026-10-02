//! Effects that are not plain record runs (Rust twin of `tools/extract/buu_fx.py`):
//! beams (one entity per beam level definition; animation = piece, step index = VRAM slot variant),
//! record tables the run scanner misses (mummy bandage, flame), bomb debris and the Big Bang Kamehameha flare.
//! Beam / flare pieces are anchored on the piece position (the record origin is *added*, unlike runs).

use crate::effects::{single_entity, FxFrame};
use crate::sheet::Builder;
use crate::sprites::{compose, Obj, DIRS, OBJ_PALETTE};
use serde_json::{json, Map, Value};
use wf_rom::content::Content;
use wf_rom::gfx::{obj_image, obj_size, palette, Image, Rgb};
use wf_rom::{hx, Rom};

const BEAMS: &[(&str, &str, u32)] = &[
    ("kamehameha_1", "Kamehameha Lv1", 0x0871_DA68),
    ("kamehameha_2", "Kamehameha Lv2", 0x0871_DA78),
    ("kamehameha_3", "Kamehameha Lv3", 0x0871_DA88),
    ("electric_kamehameha_1", "Electric Kamehameha Lv1", 0x0872_5018),
    ("electric_kamehameha_2", "Electric Kamehameha Lv2", 0x0872_5028),
    ("electric_kamehameha_3", "Electric Kamehameha Lv3", 0x0872_5038),
    ("kamekameha_1", "Kamekameha Lv1", 0x0873_15A8),
    ("kamekameha_2", "Kamekameha Lv2", 0x0873_15B8),
    ("kamekameha_3", "Kamekameha Lv3", 0x0873_15C8),
    ("banshee_blast", "Banshee Blast", 0x0876_BA2C),
    ("big_bang_kamehameha", "Big Bang Kamehameha", 0x0877_10FC),
    ("enemy_beam_red", "Enemy beam (red)", 0x0877_C41C),
];
const CHARGE_RUNS: [u32; 2] = [0x0872_1164, 0x0877_B4F8];
const EXTRA_RUNS: &[(u32, u32, &str, &str)] = &[
    (0x0875_71F4, 12, "Mummy bandage", "Bandage (class 0x0802D79C): straight records 0-2 (left/right) / 3-5 (down/up, also the stuck pose 0x08757218), diagonal 6-11"),
    (0x0872_D6B8, 20, "Flame", "Flame / Pikkon barrage (class 0x080246B4): 5 records per direction (60 bytes), record = 12*(5*dir + (t >> 2)), shown while t >> 2 <= 4"),
];
const DEBRIS: u32 = 0x0807_2500;
const BBK_FLARE: u32 = 0x0877_0D0C;
const BBK_BALL: u32 = 0x0877_0CF4;

#[derive(Clone, Copy)]
struct Rec {
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
    gfx: u32,
    hflip: bool,
    vflip: bool,
    addr: u32,
}

fn rec(rom: &Rom, a: u32) -> Rec {
    let (a0, a1) = (rom.u16(a + 4), rom.u16(a + 6));
    let (w, h) = if a0 != 0 || a1 != 0 { obj_size(a0 >> 14, a1 >> 14).unwrap_or((8, 8)) } else { (0, 0) };
    Rec {
        ox: rom.s8(a) as i32,
        oy: rom.s8(a + 1) as i32,
        w,
        h,
        gfx: rom.u32(a + 8),
        hflip: a1 & 0x1000 != 0,
        vflip: a1 & 0x2000 != 0,
        addr: a,
    }
}

/// Render `s`'s shape with tiles from `gfx`; `sign` +1 = origin added (pieces), -1 = subtracted (runs).
fn piece_image(rom: &Rom, pal: &[Rgb], s: &Rec, gfx: u32, sign: i32) -> (Image, i32, i32) {
    let im = obj_image(rom.bytes(gfx, (s.w * s.h) as usize), s.w, s.h, 8, pal, None, 0);
    let o = Obj {
        dx: sign * s.ox,
        dy: sign * s.oy,
        attr0: 0,
        attr1: 0,
        gfx,
        w: s.w,
        h: s.h,
        bpp: 8,
        hflip: s.hflip,
        vflip: s.vflip,
    };
    compose(std::slice::from_ref(&o), &[im])
}

fn write_multi(out: &mut Content, id: &str, b: &Builder, mut e: Value) {
    let (sheet, pos) = b.pack();
    out.put(format!("effects/{id}/sheet.png"), sheet.to_png());
    e["sheet"] = json!("sheet.png");
    e["frames"] = json!(b.frame_json(&pos));
    out.put_json(format!("effects/{id}/effect.json"), &e);
}

fn beams(rom: &Rom, pal: &[Rgb], out: &mut Content, index: &mut Vec<Value>) {
    for &(slug, label, d) in BEAMS {
        let (step_v, step_h, mult) = (rom.u8(d), rom.u8(d + 1), rom.u16(d + 2));
        let (flags, mode, sub) = (rom.u8(d + 4), rom.u8(d + 5), rom.u16(d + 6));
        let charge = rom.u32(d + 8);
        let table = rom.u32(d + 12);
        let g = |base: u32, i: u32| rec(rom, base + 12 * i).gfx;
        // gfx variants per piece kind, for a direction's record base
        let variants = |kind: &str, b: u32| -> Vec<u32> {
            match (mode, kind) {
                (2, "start") | (2, "tail") => (0..3).map(|k| g(b, 0) + 64 * k).collect(),
                (2, "head") => (0..3).map(|k| g(b, 7) + 64 * k).collect(),
                (2, "body") => (0..3).map(|k| g(b, 5) + 256 * k).collect(),
                (_, "impact") => vec![g(b, 10), g(b, 11)],
                (_, "start") => vec![g(b, 0), g(b, 1)],
                (_, "head") => vec![g(b, 7), g(b, 8)],
                (_, "tail") => vec![g(b, 13), g(b, 14)],
                (1, "body") => (0..4).map(|p| g(b, 5) + 64 * p).collect(),
                (_, "body") => vec![g(b, 5), g(b, 6)],
                _ => vec![],
            }
        };
        let mut pieces: Vec<(&str, u32, &str)> =
            vec![("start", 0, "start"), ("head", 7, "head"), ("impact", 10, "impact"), ("tail", 13, "tail"),
                 ("body32", 5, "body"), ("body16", 4, "body"), ("body8", 3, "body")];
        for (bit, p) in [(1u8, ("start2", 2, "start")), (2, ("tail2", 15, "tail")), (4, ("head2", 9, "head")), (8, ("impact2", 12, "impact"))] {
            if flags & bit != 0 {
                pieces.push(p);
            }
        }
        let mut b = Builder::new();
        let mut anims = Map::new();
        'piece: for (name, idx, kind) in pieces {
            let mut dirs = Map::new();
            for (di, dn) in DIRS.iter().enumerate() {
                let base = table + 0xC0 * di as u32;
                let s = rec(rom, base + 12 * idx);
                if s.w == 0 {
                    continue 'piece;
                }
                let mut gl = variants(kind, base);
                if kind == "body" {
                    // a body piece reads the body slot at its own gfx offset from record 5's gfx
                    let off = g(base, idx).wrapping_sub(g(base, 5));
                    gl = gl.into_iter().map(|x| x.wrapping_add(off)).collect();
                }
                let mut steps = Vec::new();
                for gi in gl {
                    let (im, ax, ay) = piece_image(rom, pal, &s, gi, 1);
                    let (fi, flip) = b.add(&im, ax, ay, Some(s.addr));
                    steps.push(json!({"frame": fi, "duration_frames": 4, "flip": flip, "rom": hx(gi)}));
                }
                dirs.insert(dn.to_string(), steps.into());
            }
            anims.insert(
                name.into(),
                json!({"raw_index": idx, "loop": true, "timing_source": "code 0x0802084C (variant = slot upload)",
                       "directions": dirs}),
            );
        }
        let (charge_fx, charge_index) = CHARGE_RUNS
            .iter()
            .find(|&&r| charge >= r && charge < r + 0x100 && (charge - r) % 12 == 0)
            .map(|&r| (json!(format!("fx_{r:08x}")), json!((charge - r) / 12)))
            .unwrap_or((Value::Null, Value::Null));
        let id = format!("beam_{slug}");
        write_multi(
            out,
            &id,
            &b,
            json!({"id": id, "name": format!("Beam: {label}"), "category": "effect", "name_confidence": "high",
                   "animations": anims,
                   "beam": {"mode": mode, "flags": flags, "step": [step_v, step_h], "mult": mult, "attack_sub": sub,
                            "charge_fx": charge_fx, "charge_index": charge_index},
                   "rom": {"level_def": hx(d), "piece_table": hx(table), "charge_record": hx(charge), "palette": hx(OBJ_PALETTE)},
                   "notes": "beam pieces: animation = piece, step index = VRAM slot variant (phase = t >> 2); anchor = piece position (record origin added)"}),
        );
        index.push(json!({"id": id, "name": format!("Beam: {label}"), "kind": "beam"}));
    }
}

fn bbk_flare(rom: &Rom, pal: &[Rgb], out: &mut Content, index: &mut Vec<Value>) {
    let mut b = Builder::new();
    let mut add = |s: &Rec, gfx: u32| {
        let (im, ax, ay) = piece_image(rom, pal, s, gfx, 1);
        let (fi, flip) = b.add(&im, ax, ay, Some(s.addr));
        json!({"frame": fi, "duration_frames": 4, "flip": flip, "rom": hx(gfx)})
    };
    let (mut flare, mut mirror) = (Map::new(), Map::new());
    for (di, dn) in DIRS.iter().enumerate() {
        let base = BBK_FLARE + 60 * di as u32;
        let recs: Vec<Rec> = (0..4).map(|k| rec(rom, base + 12 * k)).collect();
        let m = rec(rom, base + 48);
        flare.insert(dn.to_string(), recs.iter().map(|s| add(s, s.gfx)).collect::<Vec<_>>().into());
        mirror.insert(dn.to_string(), recs.iter().map(|s| add(&m, s.gfx)).collect::<Vec<_>>().into());
    }
    let ball: Vec<Value> = (0..2).map(|k| { let s = rec(rom, BBK_BALL + 12 * k); add(&s, s.gfx) }).collect();
    let t = "code 0x080387DC";
    write_multi(
        out,
        "bbk_flare",
        &b,
        json!({"id": "bbk_flare", "name": "Big Bang Kamehameha flare", "category": "effect", "name_confidence": "high",
               "animations": {"flare": {"raw_index": 0, "loop": true, "timing_source": t, "directions": flare},
                              "mirror": {"raw_index": 4, "loop": true, "timing_source": t, "directions": mirror},
                              "ball": {"raw_index": 0, "loop": true, "timing_source": t, "directions": {"any": ball}}},
               "rom": {"flare_records": hx(BBK_FLARE), "ball_records": hx(BBK_BALL), "palette": hx(OBJ_PALETTE)},
               "notes": "flare / mirror step = (t >> 2) & 3; ball step (t >> 2) & 1; anchor = piece position (record origin added)"}),
    );
    index.push(json!({"id": "bbk_flare", "name": "Big Bang Kamehameha flare", "kind": "beam"}));
}

pub fn extract(rom: &Rom, out: &mut Content, index: &mut Vec<Value>) {
    let pal = palette(rom, OBJ_PALETTE, 256);
    beams(rom, &pal, out, index);
    for &(base, n, name, notes) in EXTRA_RUNS {
        let frames: Vec<FxFrame> = (0..n)
            .map(|i| {
                let s = rec(rom, base + 12 * i);
                let (im, ax, ay) = piece_image(rom, &pal, &s, s.gfx, -1);
                (im, ax, ay, Some(base + 12 * i), Some(json!([s.ox, s.oy, s.w, s.h, s.hflip, s.vflip])))
            })
            .collect();
        let id = format!("fx_{base:08x}");
        single_entity(out, &format!("effects/{id}"), "effect.json", &id, name, "effect", "high", &frames, 4, "default",
            json!({"rom": {"frame_records": hx(base), "count": n, "palette": hx(OBJ_PALETTE),
                           "gfx_format": "raw (uncompressed) 8bpp tiles, shared OBJ palette"}, "notes": notes}));
        index.push(json!({"id": id, "name": name, "kind": "fx"}));
    }
    let im = obj_image(rom.bytes(DEBRIS, 64), 8, 8, 8, &pal, None, 0);
    single_entity(out, "effects/debris", "effect.json", "debris", "Debris", "effect", "high",
        &[(im, 0, 0, Some(DEBRIS), None)], 4, "default",
        json!({"rom": {"gfx": hx(DEBRIS), "obj": "8x8 8bpp"},
               "notes": "bomb / breakable-object debris (8x8, top-left = position)"}));
    index.push(json!({"id": "debris", "name": "Debris", "kind": "fx"}));
    bbk_flare(rom, &pal, out, index);
}
