//! `maps/<id>/map.json` (Tiled), `maps/tilesets/<hash>.{json,png}`, `maps/<id>/collision.json` (docs/maps.md).

use serde_json::Value;

use crate::{b64, err, image, Error, Packs, Result};

/// Tiled flip flags in a gid.
pub const FLIP_H: u32 = 0x8000_0000;
pub const FLIP_V: u32 = 0x4000_0000;
const GID_MASK: u32 = 0x0FFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Scrolls with the camera (`scroll = (camera * rate + offset) >> 10`), no wrap outside the layer.
    Tilemap,
    /// 256x256 px screen that wraps, drifts by `velocity` per frame (fog, clouds, water overlays).
    Wrap,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    /// GBA BG index 0..3.
    pub bg: u8,
    /// GBA priority 0..3 (0 = front).
    pub priority: u8,
    pub kind: LayerKind,
    /// Size of the layer's tile data (tiles).
    pub w_tiles: u32,
    pub h_tiles: u32,
    /// Row stride of `gids` (the Tiled layer width, padded to the map size).
    pub stride: u32,
    /// Tiled gids (0 = empty, else GBA tile + 1 with flip flags).
    pub gids: Vec<u32>,
    /// Scroll rate / offset / velocity in the game's 1/1024 px units.
    pub rate_x: i32,
    pub rate_y: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub vel_x: i32,
    pub vel_y: i32,
}

impl Layer {
    /// GBA tile index (gid - 1) and flips at tile (tx, ty) of the layer, `None` = transparent.
    #[inline]
    pub fn tile(&self, tx: u32, ty: u32) -> Option<(u32, bool, bool)> {
        if tx >= self.w_tiles || ty >= self.h_tiles {
            return None;
        }
        let g = self.gids[(ty * self.stride + tx) as usize];
        let id = g & GID_MASK;
        if id == 0 {
            return None;
        }
        Some((id - 1, g & FLIP_H != 0, g & FLIP_V != 0))
    }

    /// Layer-space top-left pixel shown at camera (cx, cy) (the game's scroll registers).
    pub fn scroll(&self, cx: i32, cy: i32, anim_frame: u32) -> (i32, i32) {
        let f = anim_frame as i64;
        let sx = (cx as i64 * self.rate_x as i64 + self.offset_x as i64 + self.vel_x as i64 * f) >> 10;
        let sy = (cy as i64 * self.rate_y as i64 + self.offset_y as i64 + self.vel_y as i64 * f) >> 10;
        (sx as i32, sy as i32)
    }
}

/// One GBA tile animation: `tiles` consecutive tiles starting at `dest`, frame k uses the tileset tiles
/// `frames[k].tiles[i]` for `frames[k].duration` 60 Hz frames.
#[derive(Clone, Debug)]
pub struct TileAnim {
    pub dest: u32,
    pub tiles: u32,
    pub durations: Vec<u32>,
    /// `src[k][i]` = tileset tile shown at `dest + i` during frame k.
    pub src: Vec<Vec<u32>>,
}

impl TileAnim {
    pub fn period(&self) -> u32 {
        self.durations.iter().map(|d| (*d).max(1)).sum()
    }
    /// The game's timer (`0x08009B80` init, tick `0x08009CC2` once per logic frame): animation `k` of the map
    /// (0-based, header order) starts on frame 0 with countdown `k + 1`; each tick counts down, at 0 the next frame
    /// is uploaded and the countdown becomes that frame's duration. Returns (frame shown, tick of its upload; 0 =
    /// the load) after `ticks` ticks since the load.
    pub fn after_ticks(&self, k: usize, ticks: u32) -> (usize, u32) {
        let n = self.durations.len();
        let first = k as u32 + 1;
        if n <= 1 || ticks < first {
            return (0, 0);
        }
        // from tick `first` on: frame 1, 2, ..., n-1, 0, 1, ... each for its duration
        let p = self.period();
        let mut t = (ticks - first) % p.max(1);
        let base = ticks - t;
        let mut f = 1 % n;
        let mut at = 0;
        loop {
            let d = self.durations[f].max(1);
            if t < d {
                return (f, base + at);
            }
            t -= d;
            at += d;
            f = (f + 1) % n;
        }
    }

    pub fn frame_at(&self, t: u32) -> usize {
        let p = self.period();
        if p == 0 {
            return 0;
        }
        let mut t = t % p;
        for (k, d) in self.durations.iter().enumerate() {
            let d = (*d).max(1);
            if t < d {
                return k;
            }
            t -= d;
        }
        0
    }
}

/// Indexed 8x8 tiles (8bpp) + the BG palette.
#[derive(Clone, Debug)]
pub struct Tileset {
    pub id: String,
    pub count: u32,
    /// 64 palette indices per tile (row-major); index 0 = transparent.
    pub tiles: Vec<u8>,
    pub palette: Vec<u32>,
    pub vram_tiles: u32,
    pub anims: Vec<TileAnim>,
}

impl Tileset {
    pub fn load(packs: &Packs, json_path: &str) -> Result<Tileset> {
        let j: Value = packs.json(json_path)?;
        let dir = json_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let img_name = j["image"].as_str().ok_or_else(|| Error(format!("{json_path}: no image")))?;
        let png = packs.read(&format!("{dir}/{img_name}")).ok_or_else(|| Error(format!("{json_path}: missing {img_name}")))?;
        let img = image::decode_indexed(&png)?;
        let cols = j["columns"].as_u64().unwrap_or(16) as u32;
        let count = j["tilecount"].as_u64().unwrap_or(0) as u32;
        let mut tiles = vec![0u8; count as usize * 64];
        for t in 0..count {
            let (ox, oy) = ((t % cols) * 8, (t / cols) * 8);
            for y in 0..8 {
                for x in 0..8 {
                    let (px, py) = (ox + x, oy + y);
                    if px < img.w && py < img.h {
                        tiles[(t * 64 + y * 8 + x) as usize] = img.idx[(py * img.w + px) as usize];
                    }
                }
            }
        }
        let props = props(&j);
        let vram_tiles = prop_i(&props, "vram_tiles").unwrap_or(count as i64) as u32;
        // GBA timing from the tile_animations property; tile sources from Tiled's per-tile animations
        let mut anims = Vec::new();
        let tiled_anim = |tile: u32| -> Option<Vec<u32>> {
            j["tiles"].as_array()?.iter().find(|t| t["id"].as_u64() == Some(tile as u64)).and_then(|t| {
                t["animation"].as_array().map(|a| a.iter().filter_map(|f| f["tileid"].as_u64().map(|x| x as u32)).collect())
            })
        };
        if let Some(s) = prop_s(&props, "tile_animations") {
            if let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&s) {
                for a in list {
                    let dest = a["dest_tile"].as_u64().unwrap_or(0) as u32;
                    let n = a["tiles"].as_u64().unwrap_or(0) as u32;
                    let durations: Vec<u32> =
                        a["frames_duration"].as_array().map(|v| v.iter().filter_map(|d| d.as_u64().map(|d| d as u32)).collect()).unwrap_or_default();
                    let per_tile: Vec<Vec<u32>> = (0..n).map(|i| tiled_anim(dest + i).unwrap_or_default()).collect();
                    let src = (0..durations.len())
                        .map(|k| (0..n as usize).map(|i| per_tile[i].get(k).copied().unwrap_or(dest + i as u32)).collect())
                        .collect();
                    anims.push(TileAnim { dest, tiles: n, durations, src });
                }
            }
        }
        Ok(Tileset { id: j["name"].as_str().unwrap_or("").to_string(), count, tiles, palette: img.palette, vram_tiles, anims })
    }
}

#[derive(Clone, Debug)]
pub struct Map {
    pub id: String,
    pub world_w: i32,
    pub world_h: i32,
    pub camera_max_x: i32,
    pub camera_max_y: i32,
    /// Back to front (GBA draw order).
    pub layers: Vec<Layer>,
    pub tileset: Tileset,
    pub bldcnt: u16,
    pub eva: u8,
    pub evb: u8,
    /// Backdrop colour (palette 0).
    pub backdrop: u32,
}

fn props(j: &Value) -> Vec<(String, Value)> {
    j["properties"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| Some((p["name"].as_str()?.to_string(), p["value"].clone()))).collect())
        .unwrap_or_default()
}
fn prop<'a>(p: &'a [(String, Value)], k: &str) -> Option<&'a Value> {
    p.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}
fn prop_i(p: &[(String, Value)], k: &str) -> Option<i64> {
    prop(p, k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
}
fn prop_f(p: &[(String, Value)], k: &str) -> Option<f64> {
    prop(p, k).and_then(|v| v.as_f64())
}
fn prop_s(p: &[(String, Value)], k: &str) -> Option<String> {
    prop(p, k).and_then(|v| v.as_str().map(String::from))
}
fn fx1024(v: f64) -> i32 {
    (v * 1024.0).round() as i32
}

impl Map {
    pub fn load(packs: &Packs, id: &str) -> Result<Map> {
        let path = format!("maps/{id}/map.json");
        let j: Value = packs.json(&path)?;
        let mp = props(&j);
        let map_w = j["width"].as_u64().unwrap_or(0) as u32;
        let ts_src = j["tilesets"][0]["source"].as_str().ok_or_else(|| Error(format!("{path}: no tileset")))?;
        let ts_path = normalize(&format!("maps/{id}/{ts_src}"));
        let tileset = Tileset::load(packs, &ts_path)?;
        let mut layers = Vec::new();
        for l in j["layers"].as_array().into_iter().flatten() {
            if l["type"] != "tilelayer" {
                continue;
            }
            let lp = props(l);
            let stride = l["width"].as_u64().unwrap_or(map_w as u64) as u32;
            let gids = decode_layer(l).map_err(|e| Error(format!("{path} {}: {e}", l["name"])))?;
            let kind = if prop_s(&lp, "kind").as_deref() == Some("wrap") { LayerKind::Wrap } else { LayerKind::Tilemap };
            let (w_tiles, h_tiles) = match kind {
                LayerKind::Wrap => (32, 32),
                LayerKind::Tilemap => (
                    prop_i(&lp, "layer_w_tiles").unwrap_or(stride as i64) as u32,
                    prop_i(&lp, "layer_h_tiles").unwrap_or(l["height"].as_i64().unwrap_or(0)) as u32,
                ),
            };
            layers.push(Layer {
                name: l["name"].as_str().unwrap_or("").to_string(),
                bg: prop_i(&lp, "bg").unwrap_or(0) as u8,
                priority: prop_i(&lp, "priority").unwrap_or(3) as u8,
                kind,
                w_tiles: w_tiles.min(stride),
                h_tiles,
                stride,
                gids,
                rate_x: fx1024(prop_f(&lp, "scroll_rate_x").unwrap_or(1.0)),
                rate_y: fx1024(prop_f(&lp, "scroll_rate_y").unwrap_or(1.0)),
                offset_x: fx1024(prop_f(&lp, "scroll_offset_x_px").unwrap_or(0.0)),
                offset_y: fx1024(prop_f(&lp, "scroll_offset_y_px").unwrap_or(0.0)),
                vel_x: fx1024(prop_f(&lp, "velocity_x_px_per_frame").unwrap_or(0.0)),
                vel_y: fx1024(prop_f(&lp, "velocity_y_px_per_frame").unwrap_or(0.0)),
            });
        }
        let backdrop = tileset.palette.first().copied().unwrap_or(0xFF00_0000) | 0xFF00_0000;
        Ok(Map {
            id: id.to_string(),
            world_w: prop_i(&mp, "world_w_px").unwrap_or(map_w as i64 * 8) as i32,
            world_h: prop_i(&mp, "world_h_px").unwrap_or(0) as i32,
            camera_max_x: prop_i(&mp, "camera_max_x").unwrap_or(0) as i32,
            camera_max_y: prop_i(&mp, "camera_max_y").unwrap_or(0) as i32,
            layers,
            tileset,
            bldcnt: prop_i(&mp, "bldcnt").unwrap_or(0) as u16,
            eva: prop_i(&mp, "blend_eva").unwrap_or(16) as u8,
            evb: prop_i(&mp, "blend_evb").unwrap_or(0) as u8,
            backdrop,
        })
    }
}

fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            ".." => {
                out.pop();
            }
            "." | "" => {}
            c => out.push(c),
        }
    }
    out.join("/")
}

fn decode_layer(l: &Value) -> Result<Vec<u32>> {
    match &l["data"] {
        Value::Array(a) => Ok(a.iter().map(|v| v.as_u64().unwrap_or(0) as u32).collect()),
        Value::String(s) => {
            let raw = b64::decode(s).ok_or_else(|| Error("bad base64".into()))?;
            let bytes = match l["compression"].as_str() {
                Some("zlib") => miniz_oxide::inflate::decompress_to_vec_zlib(&raw).map_err(|e| Error(format!("zlib: {e:?}")))?,
                Some("") | None => raw,
                Some(c) => return err(format!("unsupported compression {c}")),
            };
            Ok(bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
        }
        _ => err("no layer data"),
    }
}

/// `collision.json`: 256x256 grids of 8 px tiles, 1 = passable.
#[derive(Clone, Debug)]
pub struct Collision {
    pub w: u32,
    pub h: u32,
    pub walk: Vec<u8>,
    pub shot: Vec<u8>,
    /// Tile bbox of passable walk tiles `[x0, y0, x1, y1)`.
    pub walk_bounds: [i32; 4],
}

impl Collision {
    pub fn load(packs: &Packs, id: &str) -> Result<Collision> {
        let path = format!("maps/{id}/collision.json");
        let j: Value = packs.json(&path)?;
        if let Some(rows) = j["rows"].as_array() {
            // mod-friendly form: one string per tile row, '.' open, '#' wall, '~' blocks walking but not shots
            let rows: Vec<&str> = rows.iter().filter_map(|r| r.as_str()).collect();
            let w = rows.iter().map(|r| r.len()).max().unwrap_or(0) as u32;
            let h = rows.len() as u32;
            let (mut walk, mut shot) = (vec![0u8; (w * h) as usize], vec![0u8; (w * h) as usize]);
            for (y, r) in rows.iter().enumerate() {
                for (x, c) in r.bytes().enumerate() {
                    let k = y * w as usize + x;
                    walk[k] = (c == b'.') as u8;
                    shot[k] = (c == b'.' || c == b'~') as u8;
                }
            }
            return Ok(Collision { w, h, walk, shot, walk_bounds: [0, 0, w as i32, h as i32] });
        }
        let w = j["width"].as_u64().unwrap_or(256) as u32;
        let h = j["height"].as_u64().unwrap_or(256) as u32;
        let grid = |k: &str| -> Result<Vec<u8>> {
            let s = j[k].as_str().ok_or_else(|| Error(format!("{path}: no {k}")))?;
            decode_runs(s, (w * h) as usize).ok_or_else(|| Error(format!("{path}: bad {k} grid")))
        };
        let wb = j["walk_bounds"].as_array().map(|a| a.iter().map(|v| v.as_i64().unwrap_or(0) as i32).collect::<Vec<_>>()).unwrap_or_default();
        Ok(Collision {
            w,
            h,
            walk: grid("walk")?,
            shot: grid("shot")?,
            walk_bounds: if wb.len() == 4 { [wb[0], wb[1], wb[2], wb[3]] } else { [0, 0, w as i32, h as i32] },
        })
    }

    /// The game's grid test: every 8 px tile under the box `[x0, x1) x [y0, y1)` (px, rounded out to tiles)
    /// is passable.
    pub fn test_walk(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        test(&self.walk, self.w, self.h, x0, y0, x1, y1)
    }
    pub fn test_shot(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        test(&self.shot, self.w, self.h, x0, y0, x1, y1)
    }
}

fn test(g: &[u8], w: u32, h: u32, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    if x0 < 0 || y0 < 0 || x1 > 2048 || y1 > 2048 {
        return false;
    }
    let (tx1, ty1) = ((x1 + 7) >> 3, (y1 + 7) >> 3);
    for ty in (y0 >> 3)..ty1 {
        for tx in (x0 >> 3)..tx1 {
            if tx as u32 >= w || ty as u32 >= h || g[(ty as u32 * w + tx as u32) as usize] != 1 {
                return false;
            }
        }
    }
    true
}

/// Run lengths (LEB128) over the grid, first run = blocked (0), alternating.
fn decode_runs(s: &str, n: usize) -> Option<Vec<u8>> {
    let b = b64::decode(s)?;
    let mut out = Vec::with_capacity(n);
    let mut v = 0u8;
    let mut i = 0;
    while i < b.len() && out.len() < n {
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let c = *b.get(i)?;
            i += 1;
            len |= ((c & 0x7F) as usize) << shift;
            shift += 7;
            if c & 0x80 == 0 {
                break;
            }
        }
        out.extend(std::iter::repeat_n(v, len.min(n - out.len())));
        v ^= 1;
    }
    out.resize(n, 0);
    Some(out)
}
