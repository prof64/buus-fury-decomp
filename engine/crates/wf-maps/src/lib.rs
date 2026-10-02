//! Buu's Fury maps straight from the ROM: area records → map headers → BG layer tilemaps, the tileset the
//! map loader builds in VRAM (tile list + shared tile chunks + animated tiles), the global BG palette and
//! the collision grids. No emulator. Format notes: `engine/docs/maps.md`.

pub mod collision;
pub mod render;
pub mod rom_maps;
pub mod tiled;
pub mod worldmap;

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};
use wf_rom::{content::Content, gfx::Rgb, hx, text::StringTable, Rom};

use rom_maps::{Area, Header, Resource, RES_EMPTY, RES_TILEMAP, RES_WRAP};

/// Shared decode state: decompressed blobs and the BG palette.
pub struct Ctx<'a> {
    pub rom: &'a Rom,
    pub pal: Vec<Rgb>,
    blobs: HashMap<u32, Vec<u8>>,
}

impl<'a> Ctx<'a> {
    pub fn new(rom: &'a Rom) -> Ctx<'a> {
        Ctx { rom, pal: bg_palette(rom), blobs: HashMap::new() }
    }
    /// Webfoot-LZ blob (cached; empty on error).
    pub fn blob(&mut self, a: u32) -> &[u8] {
        let rom = self.rom;
        self.blobs.entry(a).or_insert_with(|| if rom.isptr(a) { rom.lz(a).unwrap_or_default() } else { Vec::new() })
    }
    /// A headerless webfoot-LZ stream (the animated-tile frames call the IWRAM decoder on the stream directly):
    /// decoded by giving it a synthetic `type 1` container header.
    pub fn raw_stream(&mut self, a: u32) -> &[u8] {
        let rom = self.rom;
        self.blobs.entry(a | 0x8000_0000).or_insert_with(|| {
            if !rom.isptr(a) {
                return Vec::new();
            }
            let mut buf = vec![1, 0, 0, 0, 0, 0, 0, 0];
            buf.extend_from_slice(rom.bytes(a, 0x10000));
            wf_rom::lz::decompress(&buf, 0).map(|(d, _)| d).unwrap_or_default()
        })
    }
}

/// The 256 BG colours of the global palette (`0x080701B0`, RGB888 triplets) as the hardware shows them:
/// reduced to BGR555 by the fade routine, expanded back like every other extracted image.
pub fn bg_palette(rom: &Rom) -> Vec<Rgb> {
    (0..256u32)
        .map(|i| {
            let b = rom.bytes(rom_maps::GLOBAL_PALETTE_RGB888 + 3 * i, 3);
            let c = (b[0] as u16 >> 3) | ((b[1] as u16 >> 3) << 5) | ((b[2] as u16 >> 3) << 10);
            wf_rom::gfx::bgr555(c)
        })
        .collect()
}

/// The 8bpp BG character data the loader puts in VRAM (charbase 0) for a map header:
/// `tiles[64 * t ..]` = tile t. Also returns the number of tiles from the tile list.
pub struct Tileset {
    /// VRAM tiles after the load (tile list, then frame 0 of every tile animation)
    pub tiles: Vec<u8>,
    pub n_listed: usize,
    /// per animation: frames of `ntiles * 64` bytes
    pub anim_frames: Vec<Vec<Vec<u8>>>,
}

pub const VRAM_TILES: usize = 1024;

pub fn tileset(ctx: &mut Ctx, h: &Header) -> Tileset {
    let list = ctx.blob(h.tile_list).to_vec();
    let n = (list.len() / 2).min(VRAM_TILES);
    let mut ent: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([list[2 * i], list[2 * i + 1]])).collect();
    // delta coded (0x08009C02): e[i] += e[i-1] + 1
    for i in 1..n {
        ent[i] = ent[i].wrapping_add(ent[i - 1]).wrapping_add(1);
    }
    let mut tiles = vec![0u8; n * 64];
    for (i, &e) in ent.iter().enumerate() {
        let chunk = ctx.rom.u32(h.tile_chunks + 4 * (e as u32 >> 8));
        let data = ctx.blob(chunk);
        let t = (e & 0xFF) as usize * 64;
        if let Some(s) = data.get(t..t + 64) {
            tiles[i * 64..i * 64 + 64].copy_from_slice(s);
        }
    }
    let mut anim_frames = Vec::new();
    for a in &h.anims {
        let len = a.ntiles as usize * 64;
        let frames: Vec<Vec<u8>> = a
            .frames
            .iter()
            .map(|&(_, p)| {
                let mut d = ctx.raw_stream(p).to_vec();
                d.resize(len, 0);
                d
            })
            .collect();
        // frame 0 is uploaded at load (0x08009C9E -> 0x08009B12)
        let dst = a.dest_tile as usize * 64;
        if let Some(f0) = frames.first() {
            if dst + len > tiles.len() {
                tiles.resize((dst + len).min(VRAM_TILES * 64), 0);
            }
            let end = (dst + len).min(tiles.len());
            if dst < end {
                tiles[dst..end].copy_from_slice(&f0[..end - dst]);
            }
        }
        anim_frames.push(frames);
    }
    Tileset { tiles, n_listed: n, anim_frames }
}

/// One map = one distinct map header.
pub struct MapInfo {
    pub id: String,
    pub header: Header,
    pub areas: Vec<String>,
}

/// Distinct headers in area-table order; id = `area_WW_AAA` of the first area using it (`_vN` for variant N > 1).
pub fn maps(rom: &Rom, areas: &[Area]) -> Vec<MapInfo> {
    let mut out: Vec<MapInfo> = Vec::new();
    let mut by_hdr: HashMap<u32, usize> = HashMap::new();
    for a in areas {
        for (k, &h) in a.headers.iter().enumerate() {
            if !rom.isptr(h) {
                continue;
            }
            if let Some(&i) = by_hdr.get(&h) {
                if !out[i].areas.contains(&a.key()) {
                    out[i].areas.push(a.key());
                }
                continue;
            }
            let id = if k == 0 { a.label() } else { format!("{}_v{}", a.label(), k + 1) };
            by_hdr.insert(h, out.len());
            out.push(MapInfo { id, header: rom_maps::header(rom, h), areas: vec![a.key()] });
        }
    }
    out
}

fn fnv64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Write `maps/...` and `areas.json` into `out` (paths relative to the content root).
pub fn extract(rom: &Rom, out: &mut Content) -> wf_rom::Result<()> {
    let strings = StringTable::load(rom);
    let areas = rom_maps::areas(rom);
    let maps = maps(rom, &areas);
    let mut ctx = Ctx::new(rom);
    let mut tilesets_written: BTreeMap<String, usize> = BTreeMap::new();
    let mut index = Vec::new();
    let mut area_maps: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for m in &maps {
        for a in &m.areas {
            area_maps.entry(a.clone()).or_default().push(m.id.clone());
        }
        let h = &m.header;
        let ts = tileset(&mut ctx, h);
        let ts_bytes = tiled::tileset_key(&ts, h);
        let ts_id = format!("{:016x}", fnv64(&ts_bytes));
        if !tilesets_written.contains_key(&ts_id) {
            let (png, tsj) = tiled::tileset_files(&ts, h, &ctx.pal, &ts_id);
            out.put(format!("maps/tilesets/{ts_id}.png"), png);
            out.put_json(format!("maps/tilesets/{ts_id}.json"), &tsj);
        }
        *tilesets_written.entry(ts_id.clone()).or_default() += 1;

        let res: Vec<Option<Resource>> = h.res.iter().map(|&r| rom_maps::resource(rom, r)).collect();
        let mut layers = Vec::new();
        for (bg, r) in res.iter().enumerate() {
            let Some(r) = r else { continue };
            match r.kind {
                RES_TILEMAP | RES_WRAP => {
                    let lay = render::decode_layer(&mut ctx, r);
                    let idx = render::render_layer(&lay, &ts.tiles);
                    out.put(
                        format!("maps/{}/bg{bg}.png", m.id),
                        wf_rom::gfx::encode_png_indexed(lay.w * 8, lay.h * 8, &idx, &ctx.pal, true),
                    );
                    layers.push((bg, r.clone(), lay, idx));
                }
                RES_EMPTY => {}
                k if worldmap::handles(k) => {}
                k => out.warn(format!(
                    "maps/{}: bg{bg} resource {} has an unsupported layer kind {} (not rendered)",
                    m.id,
                    hx(r.addr),
                    hx(k)
                )),
            }
        }
        if res.iter().flatten().any(|r| worldmap::handles(r.kind)) {
            worldmap::export(&mut ctx, h, &res, &m.id, out);
        }
        let (w, hgt) = h.world_px();
        let comp = render::composite(&layers, h, w, hgt);
        out.put(format!("maps/{}/composite.png", m.id), wf_rom::gfx::encode_png_indexed(w, hgt, &comp, &ctx.pal, false));

        let coll = collision::grids(&mut ctx, h);
        out.put_json(format!("maps/{}/collision.json", m.id), &collision::to_json(&coll, h));

        let map_json = tiled::map_json(m, &layers, &res, &ts_id, &ts, &ctx.pal);
        out.put_json(format!("maps/{}/map.json", m.id), &map_json);

        index.push(json!({
            "id": m.id,
            "areas": m.areas,
            "header": hx(h.addr),
            "size_px": [w, hgt],
            "tileset": ts_id,
            "layers": layers.iter().map(|(bg, ..)| *bg).collect::<Vec<_>>(),
            "tile_animations": h.anims.len(),
        }));
    }

    let area_json: Vec<Value> = areas
        .iter()
        .map(|a| {
            let song = if a.song == 0xFF {
                json!(null)
            } else if rom.isptr(a.song) {
                json!({"script": hx(a.song)})
            } else {
                json!(a.song)
            };
            json!({
                "key": a.key(),
                "world": a.world,
                "area": a.area,
                "name": if a.name_id != 0 { strings.get(a.name_id as usize).map(String::from) } else { None },
                "name_string_id": if a.name_id != 0 { json!(a.name_id) } else { json!(null) },
                "song": song,
                "maps": area_maps.get(&a.key()).cloned().unwrap_or_default(),
                "rom": {
                    "record": hx(a.addr),
                    "variants": a.variants,
                    "header_list": hx(a.headers_list),
                    "headers": a.headers.iter().map(|&h| hx(h)).collect::<Vec<_>>(),
                    "cond_objects": [a.n_cond_objects, hx(a.cond_objects)],
                    "objects": [a.n_objects, hx(a.objects)],
                    "objects2": [a.n_objects2, hx(a.objects2)],
                    "word08": a.flag8,
                    "word28": hx(a.word28),
                    "load_script": if a.script != 0 { json!(hx(a.script)) } else { json!(null) },
                    "word34": hx(a.word34),
                },
            })
        })
        .collect();
    out.put_json(
        "areas.json",
        &json!({
            "format": "engine/docs/maps.md",
            "note": "area table 0x0808E2E0 (452 x 0x38). key = world-area as used by exits/warps. song: song id, null = \
                     silence (0xFF), {script} = a bytecode script picks it. maps: map ids per variant (variant 1 first).",
            "areas": area_json,
        }),
    );
    out.put_json(
        "maps/index.json",
        &json!({
            "format": "engine/docs/maps.md",
            "tile_px": 8,
            "palette": "global BG palette 0x080701B0 (shared by every map)",
            "maps": index,
            "tilesets": tilesets_written.iter().map(|(k, v)| json!({"id": k, "maps": v})).collect::<Vec<_>>(),
        }),
    );
    // the global palette's original RGB888 values (BG 0..255, OBJ 256..511): the fade routine 0x08007074 computes
    // palette RAM from these, `(c * (256 - level) + target * level) >> 11` per channel (frontend palette fades)
    let pal: Vec<String> = (0..512u32)
        .map(|i| {
            let b = rom.bytes(rom_maps::GLOBAL_PALETTE_RGB888 + 3 * i, 3);
            format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2])
        })
        .collect();
    out.put_json(
        "maps/palette.json",
        &json!({
            "format": "engine/docs/maps.md",
            "rom": "0x080701B0",
            "note": "global palette, RGB888 as stored (BG 0..255, OBJ 256..511); images use the BGR555 expansion (c >> 3 << 3 | c >> 3 >> 2)",
            "rgb888": pal,
        }),
    );
    Ok(())
}
