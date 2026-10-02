//! World-map (mode 7) renderer vs emulator screenshots (headless): `cargo run --release -p wf-app --example
//! wm_parity -- <content dir> <work/frontend_wm dir> [out dir] [scene...]`. Scenes from
//! `crates/wf-render/re/capture_worldmap.py`. Writes `<out>/<scene>.png` (emulator | ours | diff) and prints the
//! differing pixels per region: ground (BG2, below line 56), sky (above), OBJs (label / marker / billboards / flyer).

use std::path::{Path, PathBuf};

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::Renderer;
use wf_view::*;

fn load_png(p: &Path) -> Vec<u32> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    let mut r = d.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    buf.chunks(ch).take(240 * 160).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect()
}

fn save(p: &Path, imgs: &[&[u32]]) {
    let (w, h) = (240 * imgs.len(), 160);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for im in imgs {
            for x in 0..240 {
                let c = im[y * 240 + x];
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

pub struct WmResult {
    pub name: String,
    pub total: usize,
    pub rows: [usize; 3],
}

fn v7(c: &serde_json::Value, label: Option<i32>) -> Mode7View {
    let g = |i: usize| c[i].as_i64().unwrap() as i32;
    Mode7View { x: g(0), y: g(1), height: g(2), heading: g(3), pitch: g(4), label }
}

/// The flyer's sheet + frame from the capture (sprite def address, raw anim, anim step, facing).
fn flyer_view(packs: &Packs, f: &serde_json::Value) -> Option<FlyerView> {
    let idx: serde_json::Value = packs.json("sprites/index.json").ok()?;
    let def = f["def"].as_str()?;
    for e in idx.as_array()? {
        let id = e["id"].as_str()?;
        let key = format!("sprites/{id}");
        let Ok(d) = wf_app::wf_content::sprite::SpriteDef::load(packs, &key) else { continue };
        let j: serde_json::Value = packs.json(&format!("{key}/sprite.json")).ok()?;
        if !j["rom"]["def"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(def)) {
            continue;
        }
        let (_, a) = d.anim_by_raw(f["raw"].as_i64()? as i32)?;
        let st = a.steps(f["dir"].as_u64()? as u8).get(f["step"].as_u64()? as usize)?;
        return Some(FlyerView {
            x: f["x"].as_i64()? as i32,
            y: f["y"].as_i64()? as i32,
            z: f["z"].as_i64()? as i32,
            sheet: key,
            frame: st.frame as u16,
            flip: st.flip,
        });
    }
    None
}

/// `mode`: which captured state builds the picture: "prev" = the frame before the screenshot's last logic frame
/// (objects, camera, BG table), "cur" = objects from the last frame, BG from the one before.
pub fn run(packs: &Packs, work: &Path, out: Option<&Path>, only: &[String], mode: &str) -> Vec<WmResult> {
    if let Some(o) = out {
        std::fs::create_dir_all(o).unwrap();
    }
    let mut names: Vec<String> = std::fs::read_dir(work)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".png")).map(String::from))
        .collect();
    names.sort();
    let mut r = Renderer::new();
    let mut res = Vec::new();
    for name in names {
        if !only.is_empty() && !only.contains(&name) {
            continue;
        }
        let emu = load_png(&work.join(format!("{name}.png")));
        let j: serde_json::Value = serde_json::from_slice(&std::fs::read(work.join(format!("{name}.json"))).unwrap()).unwrap();
        let label = j["label"]["shown"].as_i64().map(|v| v as i32);
        let mut view = RenderView::default();
        let (cam_k, bg_k, fl_k) = match mode {
            "cur" => ("cam", "cam_prev", "flyer"),
            "prev2" => ("cam_prev", "cam_prev2", "flyer_prev"),
            _ => ("cam_prev", "cam_prev", "flyer_prev"),
        };
        view.map = Some(MapView { id: "area_00_050".into(), anim_frame: 0, shake: (0, 0), mode7: Some(v7(&j[cam_k], label)) });
        let mk = j["marker"].as_array().map(|m| m.iter().map(|v| v.as_i64().unwrap() as i32).collect::<Vec<_>>());
        view.world_map = Some(WorldMapView {
            bg_camera: Some(v7(&j[bg_k], None)),
            // the hardware shows the OAM built the frame before (positions, matrices) with the OBJ tiles uploaded
            // this frame: the flyer's frame graphics are the current ones
            flyer: flyer_view(packs, &j[fl_k]).map(|mut f| {
                if let Some(cur) = flyer_view(packs, &j["flyer"]) {
                    f.frame = cur.frame;
                    f.flip = cur.flip;
                }
                f
            }),
            minimap_box: mk.filter(|m| m.len() == 4).map(|m| [m[0] + 8, m[1] + 8, m[0] + 8 + 80, m[1] + 8 + 56]),
            ..Default::default()
        });
        view.cameras.push(CameraView::default());
        let fb = r.render(packs, &view, 0, 240, 160).to_vec();
        let mut rows = [0usize; 3];
        let mut total = 0;
        for y in 0..160 {
            for x in 0..240 {
                let i = y * 240 + x;
                if fb[i] & 0xFFFFFF != emu[i] & 0xFFFFFF {
                    total += 1;
                    rows[if y < 56 { 0 } else if y < 136 { 1 } else { 2 }] += 1;
                }
            }
        }
        if let Some(o) = out {
            let dimg: Vec<u32> = fb.iter().zip(&emu).map(|(a, b)| if a & 0xFFFFFF == b & 0xFFFFFF { (a >> 2) & 0x3F3F3F | 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
            save(&o.join(format!("{name}.png")), &[&emu, &fb, &dimg]);
        }
        println!("{name:16} total {total:5}  lines 0-55 {:5}  56-135 {:5}  136-159 {:5}", rows[0], rows[1], rows[2]);
        res.push(WmResult { name, total, rows });
    }
    for e in r.errors.drain(..) {
        println!("renderer: {e}");
    }
    res
}

#[allow(dead_code)]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let packs = Packs::new(vec![Box::new(DirSource(PathBuf::from(&a[0]).join("base"))) as Box<dyn FileSource>]).unwrap();
    let out = PathBuf::from(a.get(2).cloned().unwrap_or("/tmp/wm_parity".into()));
    let only: Vec<String> = a.iter().skip(3).filter(|s| !s.starts_with("--")).cloned().collect();
    let mode = a.iter().find_map(|s| s.strip_prefix("--mode=")).unwrap_or("prev").to_string();
    run(&packs, Path::new(&a[1]), Some(&out), &only, &mode);
}
