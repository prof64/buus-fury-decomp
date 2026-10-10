//! `maps/<id>/mode7.json` — the world map's mode-7 ground, sky panorama, label bar, mini-map marker and the
//! camera lookup tables (docs/maps.md "World map"); billboard props come from `maps/<id>/objects.json`.

use serde_json::Value;

use crate::{b64, image, Error, Packs, Result};

#[derive(Clone, Debug)]
pub struct Prop {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// record +0x16 (`tile_word` byte 2): 0x28 = high (z 110 px), 0x3C = on the ground, else 60 px
    pub kind: u8,
    pub priority: u8,
    pub visible: bool,
    /// OBJ attr0 | attr1 << 16 of the record
    pub attr: u32,
}

#[derive(Clone, Debug)]
pub struct Mode7Data {
    pub ground_w: u32,
    pub ground_h: u32,
    /// VRAM tile index per 8x8 world tile
    pub ground: Vec<u8>,
    pub ground_bg: u8,
    pub ground_priority: u8,
    /// two 32x32 text screens
    pub pano: [Vec<u16>; 2],
    pub pano_bg: u8,
    pub pano_priority: u8,
    pub sin: Vec<i32>,
    pub csc: Vec<i32>,
    pub line_angle: Vec<i32>,
    pub line_cos: Vec<i32>,
    pub label: Option<image::Indexed>,
    pub label_font: String,
    pub marker: Option<image::Indexed>,
    pub marker_box: [i32; 4],
    pub props: Vec<Prop>,
    /// effect key of the prop graphics (frame k = prop k)
    pub props_sheet: String,
}

fn ints(v: &Value) -> Vec<i32> {
    v.as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default()
}

impl Mode7Data {
    pub fn load(packs: &Packs, map_id: &str) -> Result<Mode7Data> {
        let dir = format!("maps/{map_id}");
        let j: Value = packs.json(&format!("{dir}/mode7.json"))?;
        let g = &j["ground"];
        let ground = b64::decode(g["tiles"].as_str().unwrap_or("")).ok_or_else(|| Error(format!("{dir}/mode7.json: bad ground")))?;
        let p = &j["panorama"];
        let chunk = |k: usize| -> Vec<u16> {
            let d = p["chunks"][k].as_str().and_then(b64::decode).unwrap_or_default();
            let mut v: Vec<u16> = d.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            v.resize(1024, 0);
            v
        };
        let t = &j["tables"];
        let png = |name: &str| packs.read(&format!("{dir}/{name}")).and_then(|b| image::decode_indexed(&b).ok());
        let mb = ints(&j["marker"]["default_box"]);
        let mut props = Vec::new();
        let mut props_sheet = format!("effects/props_{map_id}");
        if let Ok(o) = packs.json::<Value>(&format!("{dir}/objects.json")) {
            for pr in o["props"].as_array().into_iter().flatten() {
                let tw = u32::from_str_radix(pr["tile_word"].as_str().unwrap_or("0").trim_start_matches("0x"), 16).unwrap_or(0);
                let attr = u32::from_str_radix(pr["oam_attr0_1"].as_str().unwrap_or("0").trim_start_matches("0x"), 16).unwrap_or(0);
                if let Some(e) = pr["gfx"]["effect"].as_str() {
                    props_sheet = format!("effects/{e}");
                }
                props.push(Prop {
                    index: pr["index"].as_u64().unwrap_or(0) as usize,
                    x: pr["rect"]["x"].as_i64().unwrap_or(0) as i32,
                    y: pr["rect"]["y"].as_i64().unwrap_or(0) as i32,
                    w: pr["rect"]["w"].as_i64().unwrap_or(0) as i32,
                    h: pr["rect"]["h"].as_i64().unwrap_or(0) as i32,
                    kind: (tw >> 16) as u8,
                    priority: pr["priority"].as_u64().unwrap_or(3) as u8,
                    visible: pr["visible"].as_bool().unwrap_or(true),
                    attr,
                });
            }
        }
        Ok(Mode7Data {
            ground_w: g["width_tiles"].as_u64().unwrap_or(0) as u32,
            ground_h: g["height_tiles"].as_u64().unwrap_or(0) as u32,
            ground,
            ground_bg: g["bg"].as_u64().unwrap_or(2) as u8,
            ground_priority: g["priority"].as_u64().unwrap_or(3) as u8,
            pano: [chunk(0), chunk(1)],
            pano_bg: p["bg"].as_u64().unwrap_or(1) as u8,
            pano_priority: p["priority"].as_u64().unwrap_or(3) as u8,
            sin: ints(&t["sin"]),
            csc: ints(&t["csc"]),
            line_angle: ints(&t["line_angle"]),
            line_cos: ints(&t["line_cos"]),
            label: png(j["label"]["image"].as_str().unwrap_or("mode7_label.png")),
            label_font: j["label"]["font"].as_str().unwrap_or("small").to_string(),
            marker: png(j["marker"]["image"].as_str().unwrap_or("mode7_marker.png")),
            marker_box: if mb.len() == 4 { [mb[0], mb[1], mb[2], mb[3]] } else { [160, 0, 240, 56] },
            props,
            props_sheet,
        })
    }
}
