//! Check the world-map camera math against emulator recordings (headless): `cargo run -p wf-app --example
//! mode7_check -- <content dir> <rec.json> <rec.json>`. rec.json: per frame `m` = the map object words
//! (0x030019A4..) and `tabs` (BG2 scanline tables); oam.json: `m` + `oam` (shadow OAM hex) + `fl` (flyer).
//! Recorders: crates/wf-render/re/wm_record.py.

use wf_app::wf_content::mode7::Mode7Data;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::mode7::{billboard, flyer_from_camera, Camera3d};
use wf_view::Mode7View;

fn view(m: &[i64]) -> Mode7View {
    Mode7View { x: m[0] as i32, y: m[1] as i32, height: m[2] as i32, heading: m[3] as i32, pitch: m[4] as i32, label: None, label_drawn: 0 }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let packs = Packs::new(vec![Box::new(DirSource(std::path::PathBuf::from(&a[0]).join("base"))) as Box<dyn FileSource>]).unwrap();
    let d = Mode7Data::load(&packs, "area_00_050").expect("mode7");
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
    let rows = rows.as_array().unwrap();
    let ms: Vec<Vec<i64>> = rows.iter().map(|r| r["m"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()).collect();
    for lag in [0usize, 1] {
        let (mut ok_rot, mut ok_mat, mut ok_far, mut ok_bb, mut ok_tab, mut n) = (0, 0, 0, 0, 0, 0);
        let mut first_bad: Option<String> = None;
        for k in lag..ms.len() {
            let src = &ms[k - lag];
            let dst = &ms[k];
            let c = Camera3d::new(&d, &view(src));
            n += 1;
            let rot: Vec<i64> = c.rot.iter().map(|&v| v as i64).collect();
            ok_rot += (rot[..] == dst[17..21]) as usize;
            let mat: Vec<i64> = c.mat.iter().map(|&v| v as i64).collect();
            if mat[..] == dst[23..35] {
                ok_mat += 1;
            } else if first_bad.is_none() {
                first_bad = Some(format!("row {k}: mat {mat:?}\n          want {:?}", &dst[23..35]));
            }
            ok_far += (c.far as i64 == dst[5]) as usize;
            let bb: Vec<i64> = c.bbox.iter().map(|&v| v as i64).collect();
            ok_bb += (bb[..] == dst[6..10]) as usize;
            if let Some(tab) = rows[k]["tabs"].as_array().and_then(|t| t.first()) {
                let mut all = true;
                for line in 56..160 {
                    let (pa, pc, x, y) = c.scanline(&d, line);
                    let e = &tab[line];
                    let g = |i: usize| e[i].as_i64().unwrap();
                    let epa = ((g(0) & 0xFFFF) | ((g(1) & 0xFFFF) << 16)) as u32 as i32;
                    let epc = ((g(2) & 0xFFFF) | ((g(3) & 0xFFFF) << 16)) as u32 as i32;
                    if (pa, pc, x, y) != (epa, epc, g(4) as i32, g(5) as i32) {
                        all = false;
                    }
                }
                ok_tab += all as usize;
            }
        }
        println!("lag {lag}: rows {n}: rot {ok_rot} mat {ok_mat} far {ok_far} bbox {ok_bb} tables {ok_tab}");
        if let Some(b) = first_bad {
            println!("  {b}");
        }
    }
    // billboards + flyer vs shadow OAM
    if let Some(p) = a.get(2) {
        let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
        let rows = rows.as_array().unwrap();
        let ms: Vec<Vec<i64>> = rows.iter().map(|r| r["m"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()).collect();
        for lag in [0usize, 1] {
            let (mut ok, mut n, mut fly_ok) = (0, 0, 0);
            let mut shown = None;
            for k in lag.max(1)..rows.len() {
                let c = Camera3d::new(&d, &view(&ms[k - lag]));
                let oam = hex(rows[k]["oam"].as_str().unwrap());
                // the game's affine double-size prio-3 8bpp OBJs with a uniform scale matrix (pa == pd, pb == pc == 0)
                let mut want: Vec<(i32, i32, i32)> = vec![];
                for e in 0..128 {
                    let a0 = u16::from_le_bytes([oam[e * 8], oam[e * 8 + 1]]);
                    let a1 = u16::from_le_bytes([oam[e * 8 + 2], oam[e * 8 + 3]]);
                    let a2 = u16::from_le_bytes([oam[e * 8 + 4], oam[e * 8 + 5]]);
                    if (a0 >> 8) & 3 != 3 || (a2 >> 10) & 3 != 3 {
                        continue;
                    }
                    let mi = ((a1 >> 9) & 31) as usize;
                    let pa = i16::from_le_bytes([oam[mi * 32 + 6], oam[mi * 32 + 7]]) as i32;
                    want.push(((a1 & 511) as i32, (a0 & 255) as i32, pa));
                }
                let mut got: Vec<(i32, i32, i32, i32)> = d
                    .props
                    .iter()
                    .filter(|p| p.visible)
                    .filter_map(|p| billboard(&c, p))
                    .collect();
                got.sort_by_key(|g| g.3);
                let mut got: Vec<(i32, i32, i32)> = got.into_iter().take(14).map(|(x, y, s, _)| (x & 511, y & 255, s)).collect();
                // flyer + halo are in `want` too (affine double, prio 3): compare billboards as a subset
                got.sort();
                let mut w2 = want.clone();
                w2.sort();
                let sub = got.iter().all(|g| w2.contains(g)) && w2.len() >= got.len();
                n += 1;
                ok += sub as usize;
                if !sub && shown.is_none() {
                    shown = Some(format!("row {k}: got {got:?}\n   want {w2:?}"));
                }
                // flyer: derived from the camera of the same row
                let fl = rows[k]["fl"].as_array().unwrap();
                let (fx, fy, fz) = flyer_from_camera(&d, &view(&ms[k]));
                if (fx as i64, fy as i64, fz as i64) == (fl[0].as_i64().unwrap(), fl[1].as_i64().unwrap(), fl[2].as_i64().unwrap()) {
                    fly_ok += 1;
                }
            }
            println!("billboards lag {lag}: {ok}/{n} frames match; flyer from camera {fly_ok}/{n}");
            if let Some(s) = shown {
                println!("  {s}");
            }
        }
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}
