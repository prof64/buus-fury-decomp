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
    /// differing scanlines (>= 1 px) — the HBlank-IRQ jitter shows as a whole scanline with its neighbour's
    /// parameters
    pub lines: Vec<usize>,
}

fn v7(c: &serde_json::Value, label: Option<i32>) -> Mode7View {
    let g = |i: usize| c[i].as_i64().unwrap() as i32;
    Mode7View { x: g(0), y: g(1), height: g(2), heading: g(3), pitch: g(4), label, label_drawn: 0 }
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
        // objects: the state whose shadow OAM the hardware shows; BG: the camera that reproduces the RAM table
        let hist = j["history"].as_array().cloned().unwrap_or_default();
        let hw = j["oam"].as_str().unwrap_or("");
        let obj_state = hist.iter().rev().find(|h| h["shadow_oam"].as_str() == Some(hw)).cloned();
        let data = wf_app::wf_content::mode7::Mode7Data::load(packs, "area_00_050").unwrap();
        let table_cam = hist.iter().rev().map(|h| h["cam"].clone()).chain([j["cam_prev2"].clone()]).find(|c| {
            let cam = wf_render::mode7::Camera3d::new(&data, &v7(c, None));
            (56..160).all(|line| {
                let (pa, pc, x, y) = cam.scanline(&data, line);
                let e = &j["table"][line - 56];
                let g = |i: usize| e[i].as_i64().unwrap();
                (pa as u32 as i64, pc as u32 as i64, x as i64, y as i64) == (g(0) as u32 as i64, g(1) as u32 as i64, g(2), g(3))
            })
        });
        if mode == "debug" {
            println!("{name}: obj state {}, table cam found {}", obj_state.is_some(), table_cam.is_some());
        }
        let _ = &obj_state;
        let bg_cam = table_cam.clone().unwrap_or_else(|| j["cam_prev"].clone());
        let mk = j["marker"].as_array().map(|m| m.iter().map(|v| v.as_i64().unwrap() as i32).collect::<Vec<_>>());
        let minimap_box = mk.filter(|m| m.len() == 4).map(|m| [m[0] + 8, m[1] + 8, m[0] + 8 + 80, m[1] + 8 + 56]);
        // timing candidates: the camera used for the OAM build, the flyer position and the frame whose tiles are
        // in VRAM can come from different logic frames (the game's camera update runs at the end of an
        // iteration and may spill into the next video frame); "auto" picks the combination that matches
        let mut cams: Vec<serde_json::Value> = hist.iter().map(|h| h["cam"].clone()).collect();
        cams.push(j["cam_prev2"].clone());
        let fls: Vec<serde_json::Value> = hist.iter().map(|h| h["flyer"].clone()).collect();
        let combos: Vec<(usize, usize, usize)> = if mode == "auto" {
            let mut v = vec![];
            for c in 0..cams.len() {
                for f in 0..fls.len() {
                    for g in 0..fls.len() {
                        v.push((c, f, g));
                    }
                }
            }
            v
        } else {
            vec![(cams.len().saturating_sub(2), fls.len().saturating_sub(1), fls.len().saturating_sub(1))]
        };
        let mut best: Option<(usize, Vec<u32>, (usize, usize, usize))> = None;
        for &(c, f, g) in &combos {
            let mut view = RenderView::default();
            view.map = Some(MapView { id: "area_00_050".into(), anim_frame: 0, tile_frame: None, shake: (0, 0), shake_sub: (0, 0), mode7: Some(v7(&cams[c], label)), layers: None, frame: None });
            view.world_map = Some(WorldMapView {
                bg_camera: Some(v7(&bg_cam, None)),
                flyer: flyer_view(packs, &fls[f]).map(|mut fv| {
                    if let Some(gv) = flyer_view(packs, &fls[g]) {
                        fv.frame = gv.frame;
                        fv.flip = gv.flip;
                    }
                    fv
                }),
                minimap_box,
                ..Default::default()
            });
            view.cameras.push(CameraView::default());
            let fb = r.render(packs, &view, 0, 240, 160).to_vec();
            let d = fb.iter().zip(&emu).filter(|(a, b)| *a & 0xFFFFFF != *b & 0xFFFFFF).count();
            if best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, fb, (c, f, g)));
            }
        }
        let (_, fb, pick) = best.unwrap();
        // the PC zoomed-out view of the same state (visual check only): out/<scene>_pc.png
        if let (Some(o), true) = (out, std::env::var("WM_PC").is_ok()) {
            let (c, f, g) = pick;
            let mut view = RenderView::default();
            view.map = Some(MapView { id: "area_00_050".into(), anim_frame: 0, tile_frame: None, shake: (0, 0), shake_sub: (0, 0), mode7: Some(v7(&cams[c], label)), layers: None, frame: None });
            view.world_map = Some(WorldMapView {
                bg_camera: Some(v7(&bg_cam, None)),
                flyer: flyer_view(packs, &fls[f]).map(|mut fv| {
                    if let Some(gv) = flyer_view(packs, &fls[g]) {
                        fv.frame = gv.frame;
                        fv.flip = gv.flip;
                    }
                    fv
                }),
                minimap_box,
                ..Default::default()
            });
            view.cameras.push(CameraView::default());
            let (pw, ph) = (480usize, 270usize);
            let pfb = r.render(packs, &view, 0, pw, ph).to_vec();
            let mut rgb = Vec::with_capacity(pw * ph * 3);
            for c in &pfb {
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, *c as u8]);
            }
            let file = std::fs::File::create(o.join(format!("{name}_pc.png"))).unwrap();
            let mut e = png::Encoder::new(std::io::BufWriter::new(file), pw as u32, ph as u32);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&rgb).unwrap();
        }
        let n = fls.len() as i64;
        if mode == "auto" {
            println!("{name}: objects: camera of frame {}, flyer position of frame {}, flyer tiles of frame {} (0 = last)", n - 1 - pick.0 as i64, n - 1 - pick.1 as i64, n - 1 - pick.2 as i64);
        }

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
        let lines: Vec<usize> = (0..160).filter(|&y| (0..240).any(|x| fb[y * 240 + x] & 0xFFFFFF != emu[y * 240 + x] & 0xFFFFFF)).collect();
        if let Some(o) = out {
            let dimg: Vec<u32> = fb.iter().zip(&emu).map(|(a, b)| if a & 0xFFFFFF == b & 0xFFFFFF { (a >> 2) & 0x3F3F3F | 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
            save(&o.join(format!("{name}.png")), &[&emu, &fb, &dimg]);
        }
        println!("{name:16} total {total:5}  lines 0-55 {:5}  56-135 {:5}  136-159 {:5}", rows[0], rows[1], rows[2]);
        res.push(WmResult { name, total, rows, lines });
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
