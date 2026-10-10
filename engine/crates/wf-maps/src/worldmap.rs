//! The world map's (area 0-50, header +0x3C != 0) two special BG layers and its screen pieces, exported as
//! `maps/<id>/mode7.json` (+ PNGs). Format and renderer rules: `engine/docs/maps.md` §World map.
//!
//! * **Ground** (resource create fn `0x080088BD` → ctor `0x080080C0`, vtable `0x08049C84`): affine BG2, 8bpp,
//!   512x512 wrapping ring at screen base `0x0600F000`. Resource +4 u8 chunk cols, +5 rows, +8 → webfoot-LZ
//!   chunks of 32x32 tile bytes (0x400 B) covering the 3584x2560 world (14x10 chunks). The game streams the chunks
//!   around the camera into the ring (`0x08008598` / `0x08008192`); every on-screen sample equals the world tile
//!   at its true world position (verified against VRAM), so the export is the whole world tile map.
//!   Per-scanline affine parameters: `0x080086D8` (see wf-render `mode7.rs`).
//! * **Panorama** (create fn `0x08007B63` → `0x08007A60`, vtable `0x08049604`): text BG1, 8bpp, two 0x800-byte LZ
//!   chunks (32x32 screen entries) at resource +4 / +8 (0-50: the same sky chunk twice); `0x08007B26` scrolls it by
//!   the camera heading: hofs = (heading << 3) >> 10, copying columns of both chunks into the 256 px ring
//!   (`0x08007AB4`).
//! * Bottom **label** (message trigger 0x17, ctor `0x0800C0C8`, vtable `0x0804D8C8`): a 240x16 8bpp surface — the
//!   bar picture (webfoot-LZ `0x0807248C`) + the area name (string id) in the `small` font (`0x0806E050`), pen
//!   (120, 11) centred, colour 2 — shown as 7 OBJs 32x16 + one 16x16 at y 136, priority 1.
//! * Mini-map **marker** (script native `spawn_box(x0, y0, x1, y1)`, ctor `0x080279E0`, vtable `0x0804D8A0`):
//!   8x8 8bpp tile `0x0874EA00`, affine double-size OBJ, priority 0, rotated by the camera heading.

use serde_json::{json, Value};
use wf_rom::{content::Content, gfx::Rgb, hx, Rom};

use crate::rom_maps::{Header, Resource};
use crate::Ctx;

/// Resource create functions handled here.
pub const RES_PANORAMA: u32 = 0x0800_7B63;
pub const RES_MODE7_GROUND: u32 = 0x0800_88BD;

pub const LABEL_PICTURE: u32 = 0x0807_248C;
pub const LABEL_FONT: u32 = 0x0806_E050;
pub const MARKER_TILE: u32 = 0x0874_EA00;
pub const SIN_TABLE: u32 = 0x087B_6EEC;
pub const CSC_TABLE: u32 = 0x087B_72EC;
pub const LINE_ANGLE: u32 = 0x0807_1420;
pub const LINE_COS: u32 = 0x0807_1560;

pub fn handles(kind: u32) -> bool {
    kind == RES_PANORAMA || kind == RES_MODE7_GROUND
}

/// The OBJ half of the global palette (`0x080701B0` + 256 colours), as the hardware shows it.
pub fn obj_palette(rom: &Rom) -> Vec<Rgb> {
    (256..512u32)
        .map(|i| {
            let b = rom.bytes(crate::rom_maps::GLOBAL_PALETTE_RGB888 + 3 * i, 3);
            let c = (b[0] as u16 >> 3) | ((b[1] as u16 >> 3) << 5) | ((b[2] as u16 >> 3) << 10);
            wf_rom::gfx::bgr555(c)
        })
        .collect()
}

fn b64(d: &[u8]) -> String {
    crate::collision::b64(d)
}

/// Write `maps/<map_id>/mode7.json` (+ `mode7_label.png`, `mode7_marker.png`) for the special layers of a map.
pub fn export(ctx: &mut Ctx, h: &Header, res: &[Option<Resource>], map_id: &str, out: &mut Content) {
    let rom = ctx.rom;
    let mut doc = json!({
        "format": "engine/docs/maps.md (World map)",
        "map": map_id,
        "display_mode": h.mode,
    });
    for (bg, r) in res.iter().enumerate() {
        let Some(r) = r else { continue };
        match r.kind {
            RES_MODE7_GROUND => {
                let (cols, rows) = (rom.u8(r.addr + 4) as usize, rom.u8(r.addr + 5) as usize);
                let (w, hh) = (cols * 32, rows * 32);
                let mut tiles = vec![0u8; w * hh];
                let mut chunks = Vec::new();
                for cy in 0..rows {
                    for cx in 0..cols {
                        let p = rom.u32(r.addr + 8 + 4 * (cy * cols + cx) as u32);
                        chunks.push(hx(p));
                        let d = ctx.blob(p).to_vec();
                        for ty in 0..32 {
                            for tx in 0..32 {
                                tiles[(cy * 32 + ty) * w + cx * 32 + tx] = d.get(ty * 32 + tx).copied().unwrap_or(0);
                            }
                        }
                    }
                }
                doc["ground"] = json!({
                    "bg": bg,
                    "priority": h.priority[bg],
                    "width_tiles": w,
                    "height_tiles": hh,
                    "tiles": b64(&tiles),
                    "encoding": "u8 VRAM tile index per 8x8 world tile, row-major, base64",
                    "rom": {"resource": hx(r.addr), "chunks": chunks},
                });
            }
            RES_PANORAMA => {
                let mut chunks = Vec::new();
                let mut roms = Vec::new();
                for k in 0..2u32 {
                    let p = rom.u32(r.addr + 4 + 4 * k);
                    roms.push(hx(p));
                    let d = if rom.isptr(p) { ctx.blob(p).to_vec() } else { vec![0; 0x800] };
                    let mut d = d;
                    d.resize(0x800, 0);
                    chunks.push(Value::String(b64(&d)));
                }
                doc["panorama"] = json!({
                    "bg": bg,
                    "priority": h.priority[bg],
                    "chunks": chunks,
                    "encoding": "two 32x32 text-BG screens (u16 LE entries: tile | hflip 0x400 | vflip 0x800), base64",
                    "scroll": "hofs = (heading << 3) >> 10 (0x08007B26); columns >= (hofs >> 3) & 0x1E come from chunk (hofs >> 8) & 1, the others from the other chunk (0x08007AB4); vofs 0",
                    "rom": {"resource": hx(r.addr), "chunks": roms},
                });
            }
            _ => {}
        }
    }
    // lookup tables of the 3D camera / scanline builders (the renderer needs them; ROM data)
    let s32s = |a: u32, n: u32| (0..n).map(|i| rom.s32(a + 4 * i)).collect::<Vec<i32>>();
    doc["tables"] = json!({
        "sin": s32s(SIN_TABLE, 256),
        "csc": s32s(CSC_TABLE, 256),
        "line_angle": (0..160u32).map(|i| rom.s16(LINE_ANGLE + 2 * i) as i32).collect::<Vec<_>>(),
        "line_cos": (0..160u32).map(|i| rom.u16(LINE_COS + 2 * i) as i32).collect::<Vec<_>>(),
        "doc": "sin = s32[256] (65536 = 1.0, 256 = a turn), csc = 65536 / sin (same index), line_angle / line_cos = per-scanline pitch offset (22.10 angle) and its cosine (1024 = 1.0) used by the ground table builder 0x080086D8",
        "rom": {"sin": hx(SIN_TABLE), "csc": hx(CSC_TABLE), "line_angle": hx(LINE_ANGLE), "line_cos": hx(LINE_COS)},
    });
    let opal = obj_palette(rom);
    // bottom label bar picture (240x16 8bpp, webfoot-LZ)
    let pic = rom.lz(LABEL_PICTURE).unwrap_or_default();
    if pic.len() >= 240 * 16 {
        out.put(format!("maps/{map_id}/mode7_label.png"), wf_rom::gfx::encode_png_indexed(240, 16, &pic[..240 * 16], &opal, true));
        doc["label"] = json!({
            "image": "mode7_label.png",
            "x": 0, "y": 136, "priority": 1,
            "font": "small",
            "text_x": 120, "text_y": 11, "align": "center", "color": 2,
            "rom": {"picture": hx(LABEL_PICTURE), "font": hx(LABEL_FONT), "ctor": "0x0800C0C8", "draw": "0x0800C144"},
        });
    }
    // mini-map marker tile (8x8 8bpp, raw)
    let tile = rom.bytes(MARKER_TILE, 64).to_vec();
    out.put(format!("maps/{map_id}/mode7_marker.png"), wf_rom::gfx::encode_png_indexed(8, 8, &tile, &opal, true));
    doc["marker"] = json!({
        "image": "mode7_marker.png",
        "priority": 0,
        "box_from_script": "spawn_box(x0, y0, x1, y1) in the map's on-enter script (0-50: inline_0x08072A48 spawn_box(160, 0, 240, 56))",
        "default_box": [160, 0, 240, 56],
        "rule": "x = ((player_x_px * ((x1 - x0) << 22) / W) >> 22) + x0 - 8, y likewise with H (W, H = world size); 8x8 affine double-size OBJ, matrix = the camera's heading rotation >> 2",
        "rom": {"tile": hx(MARKER_TILE), "ctor": "0x080279E0", "draw": "0x08027A74"},
    });
    out.put_json(format!("maps/{map_id}/mode7.json"), &doc);
}
