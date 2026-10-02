//! Parity of the graphics extraction with the Python pipeline (`tools/extract/extract_all.py buu`):
//! `data/buu/sprites.json` + `site/public/assets/buu/{sprites,effects,portraits,items,battle/icons}`.
//! Pixel-exact for every frame / sheet / portrait / icon; equal values for frames, anchors, animations,
//! timings and hitboxes. Skips when the ROM or the reference assets are missing.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use wf_extract::{extract_rom, Options};
use wf_rom::gfx::{decode_png, Image};
use wf_rom::testutil;

fn load_png(p: &Path) -> Option<Image> {
    decode_png(&std::fs::read(p).ok()?).ok()
}

fn same_pixels(a: &Image, b: &Image) -> bool {
    // fully transparent pixels compare equal whatever their colour
    a.w == b.w
        && a.h == b.h
        && a.px.iter().zip(&b.px).all(|(x, y)| x == y || (x[3] == 0 && y[3] == 0))
}

fn close_pixels(a: &Image, b: &Image, tol: i32) -> bool {
    a.w == b.w
        && a.h == b.h
        && a.px.iter().zip(&b.px).all(|(x, y)| {
            (x[3] == 0 && y[3] == 0) || (x[3] == y[3] && (0..3).all(|i| (x[i] as i32 - y[i] as i32).abs() <= tol))
        })
}

fn trimmed(im: &Image) -> Image {
    match im.bbox() {
        Some((x0, y0, x1, y1)) => im.crop(x0, y0, x1, y1),
        None => Image::new(1, 1),
    }
}

struct Ctx {
    content: wf_extract::Content,
    assets: std::path::PathBuf,
    refdoc: Value,
    sprite_ids: HashMap<u64, String>,
}

fn setup() -> Option<Ctx> {
    let rom = testutil::rom()?;
    let root = testutil::repo_root();
    let refdoc: Value = serde_json::from_slice(&std::fs::read(root.join("data/buu/sprites.json")).ok()?).ok()?;
    let assets = root.join("site/public");
    if !assets.join("assets/buu/sprites").exists() {
        return None;
    }
    let opts = Options { only: ["sprites", "effects", "portraits", "items"].map(String::from).to_vec() };
    let (content, _) = extract_rom(&rom, &opts, &mut |_| {});
    let sprite_ids = content
        .json("sprites/index.json")?
        .as_array()?
        .iter()
        .map(|e| (e["sprite_index"].as_u64().unwrap(), e["id"].as_str().unwrap().to_string()))
        .collect();
    Some(Ctx { content, assets, refdoc, sprite_ids })
}

/// Compare one entity (sprite or effect) against its Python record. Returns a list of problems.
fn compare_entity(c: &Ctx, py: &Value, dir: &str, file: &str) -> Vec<String> {
    let mut errs = Vec::new();
    let id = py["id"].as_str().unwrap();
    let Some(mine) = c.content.json(&format!("{dir}/{file}")) else {
        return vec![format!("{id}: missing {dir}/{file}")];
    };
    let sheet = decode_png(c.content.get(&format!("{dir}/sheet.png")).unwrap()).unwrap();
    let py_sheet = load_png(&c.assets.join(py["sheet"].as_str().unwrap())).unwrap();
    if !same_pixels(&sheet, &py_sheet) {
        errs.push(format!("{id}: sheet differs ({}x{} vs {}x{})", sheet.w, sheet.h, py_sheet.w, py_sheet.h));
    }
    let (mf, pf) = (mine["frames"].as_array().unwrap(), py["frames"].as_array().unwrap());
    if mf.len() != pf.len() {
        errs.push(format!("{id}: {} frames vs {}", mf.len(), pf.len()));
        return errs;
    }
    for (k, (m, p)) in mf.iter().zip(pf).enumerate() {
        for key in ["x", "y", "w", "h", "anchor", "rom"] {
            if m[key] != p[key] {
                errs.push(format!("{id} frame {k}: {key} {} vs {}", m[key], p[key]));
            }
        }
        if let Some(png) = p["png"].as_str() {
            let r = load_png(&c.assets.join(png)).unwrap();
            let (x, y, w, h) = (m["x"].as_u64().unwrap() as u32, m["y"].as_u64().unwrap() as u32,
                                m["w"].as_u64().unwrap() as u32, m["h"].as_u64().unwrap() as u32);
            if !same_pixels(&sheet.crop(x, y, x + w, y + h), &r) {
                errs.push(format!("{id} frame {k}: pixels differ"));
            }
        }
    }
    let (ma, pa) = (mine["animations"].as_object().unwrap(), py["animations"].as_object().unwrap());
    if ma.keys().collect::<Vec<_>>() != pa.keys().collect::<Vec<_>>() {
        errs.push(format!("{id}: animation names {:?} vs {:?}", ma.keys(), pa.keys()));
    }
    for (name, p) in pa {
        let Some(m) = ma.get(name) else { continue };
        for key in ["raw_index", "loop", "timing_source", "directions"] {
            if m[key] != p[key] {
                errs.push(format!("{id} anim {name}: {key} differs"));
            }
        }
        let po = p.get("observed_frame_orders").cloned().unwrap_or(Value::Null);
        if m["observed_frame_orders"] != po {
            errs.push(format!("{id} anim {name}: observed_frame_orders differ"));
        }
    }
    if let Some(hb) = py["hitboxes"]["body_per_direction"].as_array() {
        for b in hb {
            let d = b["dir"].as_str().unwrap();
            let want = serde_json::json!([b["x0"], b["y0"], b["x1"], b["y1"]]);
            if mine["body_box"][d] != want {
                errs.push(format!("{id}: body box {d} differs"));
            }
        }
        for key in ["header", "sprite_index"] {
            if mine[key] != py[key] {
                errs.push(format!("{id}: {key} {} vs {}", mine[key], py[key]));
            }
        }
    }
    errs
}

#[test]
fn sprites_effects_portraits_items_match_python() {
    let Some(c) = setup() else {
        eprintln!("skipped: ROM or reference assets missing");
        return;
    };
    let ents = c.refdoc["entities"].as_array().unwrap();
    let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
    let mut errs = Vec::new();
    for py in ents {
        let id = py["id"].as_str().unwrap();
        let cat = py["category"].as_str().unwrap();
        let (kind, e) = if py.get("sprite_index").is_some() {
            {
            let si = py["sprite_index"].as_u64().unwrap();
            let my_id = c.sprite_ids.get(&si).cloned().unwrap_or_default();
            ("sprites", compare_entity(&c, py, &format!("sprites/{my_id}"), "sprite.json"))
        }
        } else if cat == "effect" {
            let my_id = id.strip_prefix("buu_").unwrap_or(id);
            ("effects", compare_entity(&c, py, &format!("effects/{my_id}"), "effect.json"))
        } else if cat == "portrait" {
            let p = c.assets.join(py["frames"][0]["png"].as_str().unwrap());
            // the reference keeps index 0 opaque (palette entry 0 = black); ours makes it transparent (an OBJ)
            let mine = c.content.get(&format!("portraits/{id}.png")).map(|b| {
                let mut im = decode_png(b).unwrap();
                for px in im.px.iter_mut().filter(|px| px[3] == 0) {
                    *px = [0, 0, 0, 255];
                }
                im
            });
            let ok = matches!((&mine, load_png(&p)), (Some(m), Some(r)) if same_pixels(&trimmed(m), &r));
            ("portraits", if ok { vec![] } else { vec![format!("{id}: portrait differs/missing")] })
        } else if cat == "item" {
            let k = py["item_kind"].as_str().unwrap();
            let i = py["item_id"].as_u64().unwrap();
            let p = c.assets.join(py["frames"][0]["png"].as_str().unwrap());
            let mine = c.content.get(&format!("icons/{k}/{i:03}.png")).map(|b| decode_png(b).unwrap());
            let ok = matches!((&mine, load_png(&p)), (Some(m), Some(r)) if same_pixels(&trimmed(m), &r));
            ("items", if ok { vec![] } else { vec![format!("{id}: icon differs/missing")] })
        } else {
            ("other", vec![format!("{id}: unhandled category {cat}")])
        };
        let ent = counts.entry(kind).or_default();
        ent.0 += 1;
        if e.is_empty() {
            ent.1 += 1;
        }
        errs.extend(e);
    }
    // skill icons (tools/battle/extract_buu_icons.py expands BGR555 as c*255/31; we use the c<<3|c>>2 rule of
    // every other asset, so allow 4 levels per channel)
    let (mut n, mut ok) = (0, 0);
    for id in 1..=23 {
        for (s, py) in [("", "n"), ("_alt", "alt")] {
            n += 1;
            let r = load_png(&c.assets.join(format!("assets/buu/battle/icons/skill_{id}_{py}.png")));
            let m = decode_png(c.content.get(&format!("icons/skills/skill_{id:02}{s}.png")).unwrap()).unwrap();
            if matches!(r, Some(r) if close_pixels(&m, &r, 4)) {
                ok += 1;
            } else {
                errs.push(format!("skill icon {id}{s} differs"));
            }
        }
    }
    counts.insert("skill_icons", (n, ok));
    let mut summary: Vec<_> = counts.iter().map(|(k, (n, ok))| format!("{k}: {ok}/{n} identical")).collect();
    summary.sort();
    eprintln!("{}", summary.join(", "));
    for e in errs.iter().take(40) {
        eprintln!("  {e}");
    }
    assert!(errs.is_empty(), "{} differences", errs.len());
}
