//! Which state built the OAM shown in a world-map capture? Prints the flyer OBJ position predicted from the
//! current / previous camera + flyer next to the captured hardware OAM entries. Dev tool.

use wf_app::wf_content::mode7::Mode7Data;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::mode7::Camera3d;
use wf_view::Mode7View;

fn v7(c: &serde_json::Value) -> Mode7View {
    let g = |i: usize| c[i].as_i64().unwrap() as i32;
    Mode7View { x: g(0), y: g(1), height: g(2), heading: g(3), pitch: g(4), label: None }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let packs = Packs::new(vec![Box::new(DirSource(std::path::PathBuf::from(&a[0]).join("base"))) as Box<dyn FileSource>]).unwrap();
    let d = Mode7Data::load(&packs, "area_00_050").unwrap();
    for f in &a[1..] {
        let j: serde_json::Value = serde_json::from_slice(&std::fs::read(f).unwrap()).unwrap();
        let oam: Vec<u8> = {
            let s = j["oam"].as_str().unwrap();
            (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
        };
        let mut ents = vec![];
        for e in 0..20 {
            let a0 = u16::from_le_bytes([oam[e * 8], oam[e * 8 + 1]]);
            let a1 = u16::from_le_bytes([oam[e * 8 + 2], oam[e * 8 + 3]]);
            if (a0 >> 8) & 3 == 2 {
                continue;
            }
            ents.push((e, a1 & 511, a0 & 255, (a1 >> 9) & 31));
        }
        print!("{f}: oam {ents:?}\n   ");
        for (ck, fk) in [("cam", "flyer"), ("cam_prev", "flyer_prev"), ("cam", "flyer_prev"), ("cam_prev", "flyer")] {
            let c = Camera3d::new(&d, &v7(&j[ck]));
            let fl = &j[fk];
            let p = [(fl["x"].as_i64().unwrap() as i32) << 2, (fl["y"].as_i64().unwrap() as i32) << 2, fl["z"].as_i64().unwrap() as i32];
            let r = c.project(p).map(|(sx, sy, dp)| (sx - 32, (sy - 44) & 255, (dp * 0x222 >> 17).max(0x80)));
            print!(" {ck}+{fk}: {r:?}");
        }
        println!();
        for ck in ["cam", "cam_prev"] {
            let c = Camera3d::new(&d, &v7(&j[ck]));
            let mut b: Vec<_> = d.props.iter().filter(|p| p.visible).filter_map(|p| wf_render::mode7::billboard(&c, p).map(|r| (p.index, r))).collect();
            b.sort_by_key(|x| x.1 .3);
            println!("    billboards {ck}: {:?}", b.iter().map(|(i, (x, y, s, _))| (*i, x & 511, y & 255, *s)).collect::<Vec<_>>());
        }
    }
}
