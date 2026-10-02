//! Tiled-compatible JSON (map + external tileset) for the decoded maps.

use serde_json::{json, Value};
use wf_rom::{gfx::Rgb, hx};

use crate::render::{layer_origin, Layer};
use crate::rom_maps::{Header, Resource, RES_TILEMAP, RES_WRAP};
use crate::{MapInfo, Tileset};

pub const COLUMNS: usize = 16;
const FLIP_H: u32 = 0x8000_0000;
const FLIP_V: u32 = 0x4000_0000;

/// Tiled layer data: little-endian u32 gids, zlib, base64.
pub fn encode_gids(gids: &[u32]) -> String {
    let raw: Vec<u8> = gids.iter().flat_map(|g| g.to_le_bytes()).collect();
    crate::collision::b64(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6))
}

/// Inverse of [`encode_gids`].
pub fn decode_gids(s: &str) -> Vec<u32> {
    let z = crate::collision::unb64(s);
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&z).unwrap_or_default();
    raw.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn prop(name: &str, ty: &str, v: Value) -> Value {
    json!({"name": name, "type": ty, "value": v})
}

/// Bytes identifying a tileset (tiles + animation frames + timing), for deduplication.
pub fn tileset_key(ts: &Tileset, h: &Header) -> Vec<u8> {
    let mut k = ts.tiles.clone();
    for (a, frames) in h.anims.iter().zip(&ts.anim_frames) {
        k.extend_from_slice(&a.dest_tile.to_le_bytes());
        k.push(a.ntiles);
        for (f, d) in a.frames.iter().zip(frames) {
            k.extend_from_slice(&f.0.to_le_bytes());
            k.extend_from_slice(d);
        }
    }
    k
}

/// Tileset image (VRAM tiles, then every animation frame's tiles) and the Tiled tileset JSON.
pub fn tileset_files(ts: &Tileset, h: &Header, pal: &[Rgb], id: &str) -> (Vec<u8>, Value) {
    let n_vram = ts.tiles.len() / 64;
    let mut tiles: Vec<&[u8]> = ts.tiles.chunks(64).collect();
    let mut anim_tiles = Vec::new();
    for (ai, (a, frames)) in h.anims.iter().zip(&ts.anim_frames).enumerate() {
        let mut per_tile: Vec<Vec<Value>> = vec![Vec::new(); a.ntiles as usize];
        for (fi, data) in frames.iter().enumerate() {
            let base = tiles.len();
            for k in 0..a.ntiles as usize {
                tiles.push(&data[k * 64..k * 64 + 64]);
                let dur = a.frames[fi].0 as u32;
                per_tile[k].push(json!({"tileid": base + k, "duration": (dur * 1000 + 30) / 60}));
            }
        }
        for (k, frames) in per_tile.into_iter().enumerate() {
            anim_tiles.push(json!({
                "id": a.dest_tile as usize + k,
                "animation": frames,
                "properties": [prop("animation", "int", json!(ai))],
            }));
        }
    }
    let total = tiles.len().max(1);
    let rows = total.div_ceil(COLUMNS);
    let (iw, ih) = (COLUMNS * 8, rows * 8);
    let mut idx = vec![0u8; iw * ih];
    for (t, d) in tiles.iter().enumerate() {
        let (ox, oy) = ((t % COLUMNS) * 8, (t / COLUMNS) * 8);
        for i in 0..64 {
            idx[(oy + i / 8) * iw + ox + i % 8] = d[i];
        }
    }
    let png = wf_rom::gfx::encode_png_indexed(iw as u32, ih as u32, &idx, pal, true);
    let anims: Vec<Value> = h
        .anims
        .iter()
        .map(|a| {
            json!({
                "dest_tile": a.dest_tile,
                "tiles": a.ntiles,
                "frames_duration": a.frames.iter().map(|f| f.0).collect::<Vec<_>>(),
                "rom": hx(a.addr),
            })
        })
        .collect();
    let tsj = json!({
        "type": "tileset",
        "version": "1.10",
        "tiledversion": "1.10.2",
        "name": id,
        "tilewidth": 8,
        "tileheight": 8,
        "tilecount": tiles.len(),
        "columns": COLUMNS,
        "margin": 0,
        "spacing": 0,
        "image": format!("{id}.png"),
        "imagewidth": iw,
        "imageheight": ih,
        "tiles": anim_tiles,
        "properties": [
            prop("bpp", "int", json!(8)),
            prop("vram_tiles", "int", json!(n_vram)),
            prop("listed_tiles", "int", json!(ts.n_listed)),
            prop("tile_animations", "string", json!(serde_json::to_string(&anims).unwrap())),
            prop("note", "string", json!("tiles 0..vram_tiles-1 = BG character data as loaded (frame 0 of every \
                animation applied); the rest are animation frames. Palette = global BG palette; index 0 transparent. \
                Animation durations: 60 Hz frames in tile_animations, milliseconds in the Tiled animations.")),
        ],
    });
    (png, tsj)
}

pub fn map_json(
    m: &MapInfo,
    layers: &[(usize, Resource, Layer, Vec<u8>)],
    res: &[Option<Resource>],
    ts_id: &str,
    ts: &Tileset,
    pal: &[Rgb],
) -> Value {
    let h = &m.header;
    let (ww, wh) = h.world_px();
    let mw = layers.iter().map(|l| l.2.w).max().unwrap_or(0).max(ww.div_ceil(8));
    let mh = layers.iter().map(|l| l.2.h).max().unwrap_or(0).max(wh.div_ceil(8));
    let mut order: Vec<&(usize, Resource, Layer, Vec<u8>)> = layers.iter().collect();
    order.sort_by_key(|l| (std::cmp::Reverse(h.priority[l.0]), std::cmp::Reverse(l.0)));
    let mut tl = Vec::new();
    for (i, (bg, r, l, _)) in order.iter().enumerate() {
        let mut data = vec![0u32; (mw * mh) as usize];
        for y in 0..l.h {
            for x in 0..l.w {
                let se = l.entries[(y * l.w + x) as usize];
                let mut gid = (se & 0x3FF) as u32 + 1;
                if se & 0x400 != 0 {
                    gid |= FLIP_H;
                }
                if se & 0x800 != 0 {
                    gid |= FLIP_V;
                }
                data[(y * mw + x) as usize] = gid;
            }
        }
        let (ox, oy) = layer_origin(r);
        let kind = if r.kind == RES_WRAP { "wrap" } else { "tilemap" };
        let mut props = vec![
            prop("bg", "int", json!(bg)),
            prop("priority", "int", json!(h.priority[*bg])),
            prop("kind", "string", json!(kind)),
            prop("layer_w_tiles", "int", json!(l.w)),
            prop("layer_h_tiles", "int", json!(l.h)),
            prop("scroll_rate_x", "float", json!(r.rate.0 as f64 / 1024.0)),
            prop("scroll_rate_y", "float", json!(r.rate.1 as f64 / 1024.0)),
            prop("scroll_offset_x_px", "float", json!(r.offset.0 as f64 / 1024.0)),
            prop("scroll_offset_y_px", "float", json!(r.offset.1 as f64 / 1024.0)),
            prop("png", "file", json!(format!("bg{bg}.png"))),
            prop("rom_resource", "string", json!(hx(r.addr))),
        ];
        if r.kind == RES_WRAP {
            props.push(prop("wrap", "bool", json!(true)));
            props.push(prop("velocity_x_px_per_frame", "float", json!(r.velocity.0 as f64 / 1024.0)));
            props.push(prop("velocity_y_px_per_frame", "float", json!(r.velocity.1 as f64 / 1024.0)));
        }
        tl.push(json!({
            "type": "tilelayer",
            "id": i + 1,
            "name": format!("bg{bg}"),
            "x": 0, "y": 0,
            "width": mw,
            "height": mh,
            "offsetx": ox,
            "offsety": oy,
            "parallaxx": r.rate.0 as f64 / 1024.0,
            "parallaxy": r.rate.1 as f64 / 1024.0,
            "opacity": 1,
            "visible": r.kind == RES_TILEMAP || r.kind == RES_WRAP,
            "encoding": "base64",
            "compression": "zlib",
            "data": encode_gids(&data),
            "properties": props,
        }));
    }
    let unsupported: Vec<Value> = res
        .iter()
        .enumerate()
        .filter_map(|(bg, r)| {
            let r = r.as_ref()?;
            (r.kind != RES_TILEMAP && r.kind != RES_WRAP && r.kind != crate::rom_maps::RES_EMPTY && !crate::worldmap::handles(r.kind))
                .then(|| json!({"bg": bg, "resource": hx(r.addr), "kind": hx(r.kind)}))
        })
        .collect();
    let bgc = pal[0];
    json!({
        "type": "map",
        "version": "1.10",
        "tiledversion": "1.10.2",
        "orientation": "orthogonal",
        "renderorder": "right-down",
        "infinite": false,
        "width": mw,
        "height": mh,
        "tilewidth": 8,
        "tileheight": 8,
        "backgroundcolor": format!("#{:02x}{:02x}{:02x}", bgc[0], bgc[1], bgc[2]),
        "nextlayerid": tl.len() + 1,
        "nextobjectid": 1,
        "tilesets": [{"firstgid": 1, "source": format!("../tilesets/{ts_id}.json")}],
        "layers": tl,
        "properties": [
            prop("id", "string", json!(m.id)),
            prop("areas", "string", json!(m.areas.join(","))),
            prop("world_w_px", "int", json!(ww)),
            prop("world_h_px", "int", json!(wh)),
            prop("camera_max_x", "int", json!(h.cam_w as i32 - 1)),
            prop("camera_max_y", "int", json!(h.cam_h as i32 - 1)),
            prop("display_mode", "int", json!(h.mode)),
            prop("bldcnt", "int", json!(h.bldcnt)),
            prop("blend_eva", "int", json!(h.eva)),
            prop("blend_evb", "int", json!(h.evb)),
            prop("tileset", "string", json!(ts_id)),
            prop("vram_tiles", "int", json!(ts.tiles.len() / 64)),
            prop("composite", "file", json!("composite.png")),
            prop("collision", "file", json!("collision.json")),
            prop("unsupported_layers", "string", json!(serde_json::to_string(&unsupported).unwrap())),
            prop("mode7", "string", json!(if res.iter().flatten().any(|r| crate::worldmap::handles(r.kind)) { "mode7.json" } else { "" })),
            prop("rom_header", "string", json!(hx(h.addr))),
            prop("rom_minimap", "string", json!(hx(h.minimap))),
            prop("rom_rects", "string", json!(format!("{} x 0x18 at {}", h.n_rects, hx(h.rects)))),
            prop("note", "string", json!("Layers in draw order (back to front, GBA priority). offsetx/offsety = layer \
                position at camera (0,0); the game scrolls a layer to (camera * rate + offset) >> 10 (1/1024 units). \
                gid = GBA tile index + 1 with Tiled flip flags. World pixel (0,0) = map pixel (0,0).")),
        ],
    })
}
