//! Parity against the Python / emulator outputs (skips when the ROM or the references are missing):
//! (a) collision grids vs data/battle/buu_collision.json, (b) layer + composite renders vs the emulator
//! captures in site/public/assets/buu/maps, (c) map list / sizes vs data/buu/sprites.json.
//! Run with `--nocapture` to see the summary (also written to target/wf-maps-parity.json).

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};
use wf_maps::{collision, render, rom_maps, Ctx};
use wf_rom::{gfx, testutil};

fn load_json(p: &std::path::Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

#[test]
fn collision_identical() {
    let Some(rom) = testutil::rom() else { return };
    let root = testutil::repo_root();
    let Some(refj) = load_json(&root.join("data/battle/buu_collision.json")) else { return };
    let mut ctx = Ctx::new(&rom);
    let (mut same, mut total, mut outside) = (0, 0, 0);
    let mut bad = Vec::new();
    for (key, e) in refj["maps"].as_object().unwrap() {
        total += 1;
        let h = u32::from_str_radix(e["header"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        let hdr = rom_maps::header(&rom, h);
        let g = collision::grids(&mut ctx, &hdr);
        let (w, hh) = (e["w"].as_u64().unwrap() as usize, e["h"].as_u64().unwrap() as usize);
        let mut ok = true;
        for (name, mine) in [("walk", &g.walk), ("shot", &g.shot)] {
            let theirs = collision::unrle(e[name].as_str().unwrap(), w * hh);
            for y in 0..256 {
                for x in 0..256 {
                    let m = mine[y * 256 + x];
                    if x < w && y < hh {
                        if theirs[y * w + x] != m {
                            ok = false;
                        }
                    } else if m == 1 {
                        outside += 1; // reference clips to w*h; the ROM grid has passable bits beyond it
                    }
                }
            }
        }
        if ok {
            same += 1;
        } else {
            bad.push(key.clone());
        }
    }
    eprintln!("collision: {same}/{total} maps identical within the reference w*h; passable bits outside the reference clip: {outside}; bad: {bad:?}");
    assert_eq!(same, total);
}

struct Diff {
    pixels: usize,
    anim: usize,
    beyond: usize,
    other: usize,
    faded_ok: bool,
    anim_frame_match: usize,
}

/// Compare a rendered layer (palette indices) with the reference RGBA; classify differing 8x8 cells by the
/// tile they show: an animated tile, a tile past the loaded VRAM data, or other.
fn cmp_layer(l: &render::Layer, idx: &[u8], refimg: &gfx::Image, pal: &[[u8; 3]], hdr: &rom_maps::Header, nv: usize, ts: &wf_maps::Tileset) -> Option<Diff> {
    let (pw, ph) = (l.w * 8, l.h * 8);
    if refimg.w != pw || refimg.h != ph {
        return None;
    }
    let mut d = Diff { pixels: 0, anim: 0, beyond: 0, other: 0, faded_ok: true, anim_frame_match: 0 };
    let mut cells: HashMap<(u32, u32), ()> = HashMap::new();
    let mut fade_px = 0usize;
    for y in 0..ph {
        for x in 0..pw {
            let c = idx[(y * pw + x) as usize];
            let mine = if c == 0 { [0, 0, 0, 0] } else { [pal[c as usize][0], pal[c as usize][1], pal[c as usize][2], 255] };
            let r = refimg.get(x, y);
            let r = if r[3] == 0 { [0, 0, 0, 0] } else { r };
            if mine != r {
                d.pixels += 1;
                let faded = c != 0 && {
                    let m = pal[c as usize];
                    r[3] == 255 && (0..3).all(|k| r[k] <= m[k] && (r[k] as u32) * 3 >= (m[k] as u32) / 2 && (r[k] as u32) * 3 <= (m[k] as u32) * 2) && m != [r[0], r[1], r[2]]
                };
                if faded {
                    fade_px += 1;
                } else {
                    cells.insert((x / 8, y / 8), ());
                }
            }
        }
    }
    d.faded_ok = fade_px * 2 > d.pixels;
    for (tx, ty) in cells.keys() {
        let t = (l.entries[(ty * l.w + tx) as usize] & 0x3FF) as usize;
        if let Some(ai) = hdr.anims.iter().position(|a| (a.dest_tile as usize..a.dest_tile as usize + a.ntiles as usize).contains(&t)) {
            d.anim += 1;
            // does the captured cell equal one of the animation frames?
            let se = l.entries[(ty * l.w + tx) as usize];
            let k = t - hdr.anims[ai].dest_tile as usize;
            let hit = ts.anim_frames[ai].iter().any(|fr| {
                (0..64u32).all(|i| {
                    let (x, y) = (i % 8, i / 8);
                    let sx = if se & 0x400 != 0 { 7 - x } else { x };
                    let sy = if se & 0x800 != 0 { 7 - y } else { y };
                    let c = fr[k * 64 + (sy * 8 + sx) as usize];
                    let r = refimg.get(tx * 8 + x, ty * 8 + y);
                    if c == 0 { r[3] == 0 } else { r[3] == 255 && r[..3] == pal[c as usize] }
                })
            });
            if hit {
                d.anim_frame_match += 1;
            }
        } else if t >= nv {
            d.beyond += 1;
        } else {
            d.other += 1;
        }
    }
    Some(d)
}

#[test]
fn renders_vs_emulator() {
    let Some(rom) = testutil::rom() else { return };
    let root = testutil::repo_root();
    let Some(sprites) = load_json(&root.join("data/buu/sprites.json")) else { return };
    let assets = root.join("site/public");
    if !assets.join("assets/buu/maps").exists() {
        return;
    }
    let areas = rom_maps::areas(&rom);
    let maps = wf_maps::maps(&rom, &areas);
    // resource set -> map
    let mut by_res: HashMap<Vec<u32>, usize> = HashMap::new();
    for (i, m) in maps.iter().enumerate() {
        let mut k: Vec<u32> = m.header.res.to_vec();
        k.sort();
        by_res.insert(k, i);
    }
    let mut ctx = Ctx::new(&rom);
    let mut stats: BTreeMap<&str, usize> = BTreeMap::new();
    let mut examples: Vec<Value> = Vec::new();
    let (mut size_ok, mut size_n, mut comp_same, mut comp_n, mut comp_world, mut matched) = (0, 0, 0, 0, 0, 0);
    let mut unmatched = Vec::new();
    let mut python_maps = 0;
    let coll = load_json(&root.join("data/battle/buu_collision.json"));
    let mut origins: Vec<Value> = Vec::new();
    let mut size_diffs: Vec<Value> = Vec::new();
    for pm in sprites["maps"].as_array().unwrap() {
        if pm.get("same_as").is_some() {
            continue;
        }
        python_maps += 1;
        let lays = pm["layers"].as_array().unwrap();
        let pres: Vec<u32> = lays
            .iter()
            .map(|l| u32::from_str_radix(l["rom_field_resource"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap())
            .collect();
        // find my map whose header contains all of these resources at the same BG
        let mi = maps.iter().position(|m| {
            lays.iter().zip(&pres).all(|(l, r)| m.header.res[l["bg"].as_u64().unwrap() as usize] == *r)
                && m.areas.contains(&pm["map"].as_str().unwrap().to_string())
        });
        let Some(mi) = mi.or_else(|| {
            maps.iter().position(|m| lays.iter().zip(&pres).all(|(l, r)| m.header.res[l["bg"].as_u64().unwrap() as usize] == *r))
        }) else {
            unmatched.push(pm["map"].clone());
            continue;
        };
        matched += 1;
        let m = &maps[mi];
        let ts = wf_maps::tileset(&mut ctx, &m.header);
        let nv = ts.n_listed;
        let mut rendered = Vec::new();
        let mut all_same = true;
        for l in lays {
            let bg = l["bg"].as_u64().unwrap() as usize;
            let r = rom_maps::resource(&rom, m.header.res[bg]).unwrap();
            let lay = render::decode_layer(&mut ctx, &r);
            let idx = render::render_layer(&lay, &ts.tiles);
            let png = std::fs::read(assets.join(l["png"].as_str().unwrap())).ok();
            let Some(png) = png else { *stats.entry("layer_ref_missing").or_default() += 1; continue };
            let refimg = gfx::decode_png(&png).unwrap();
            *stats.entry("layers").or_default() += 1;
            match cmp_layer(&lay, &idx, &refimg, &ctx.pal, &m.header, nv, &ts) {
                None => {
                    *stats.entry("layer_size_differs").or_default() += 1;
                    all_same = false;
                }
                Some(d) if d.pixels == 0 => *stats.entry("layer_identical").or_default() += 1,
                Some(d) => {
                    all_same = false;
                    let cls = if d.faded_ok && d.other == 0 && d.beyond == 0 {
                        "layer_diff_capture_mid_fade_50pct_plus_animated_tiles"
                    } else if d.other == 0 && d.beyond == 0 {
                        "layer_diff_only_animated_tiles"
                    } else if d.other == 0 && d.anim == 0 {
                        "layer_diff_only_tiles_past_loaded_data"
                    } else if d.other == 0 {
                        "layer_diff_animated_or_past_loaded"
                    } else {
                        "layer_diff_other"
                    };
                    *stats.entry(cls).or_default() += 1;
                    *stats.entry("animated_cells_differing").or_default() += d.anim;
                    *stats.entry("animated_cells_equal_to_another_frame").or_default() += d.anim_frame_match;
                    if cls == "layer_diff_other" && examples.len() < 40 {
                        examples.push(json!({"map": pm["map"], "mine": m.id, "bg": bg, "pixels": d.pixels,
                            "cells_anim": d.anim, "cells_past": d.beyond, "cells_other": d.other}));
                    }
                }
            }
            rendered.push((bg, r, lay, idx));
        }
        // composite: compare when the Python composite is world-aligned (all 1x layers, same size)
        let (w, h) = m.header.world_px();
        size_n += 1;
        let psize = [pm["size"][0].as_u64().unwrap() as u32, pm["size"][1].as_u64().unwrap() as u32];
        if psize == [w, h] {
            size_ok += 1;
        } else if size_diffs.len() < 80 {
            let rates: Vec<_> = rendered.iter().map(|(bg, r, ..)| (*bg, r.rate.0, r.rate.1)).collect();
            size_diffs.push(json!({"map": pm["map"], "python": psize, "world": [w, h], "rates": format!("{rates:?}")}));
        }
        let one_x = rendered.iter().all(|(_, r, ..)| r.rate == (1024, 1024));
        // Python crops its composite to the union bbox of the opaque layer pixels
        let mut bb: Option<(i32, i32, i32, i32)> = None;
        for (_, r, l, idx) in &rendered {
            let (ox, oy) = render::layer_origin(r);
            let pw = l.w as i32 * 8;
            for (i, &c) in idx.iter().enumerate() {
                if c != 0 {
                    let (x, y) = (i as i32 % pw + ox, i as i32 / pw + oy);
                    bb = Some(match bb {
                        None => (x, y, x + 1, y + 1),
                        Some((a, b, c2, d2)) => (a.min(x), b.min(y), c2.max(x + 1), d2.max(y + 1)),
                    });
                }
            }
        }
        let (bx0, by0, bx1, by1) = bb.unwrap_or((0, 0, 1, 1));
        let (w, h) = ((bx1 - bx0) as u32, (by1 - by0) as u32);
        if let Some(o) = coll.as_ref().and_then(|c| c["maps"][pm["map"].as_str().unwrap()].get("origin").cloned()) {
            origins.push(json!({"map": pm["map"], "measured_origin": o, "rom_composite_crop_origin": [bx0, by0], "one_x": one_x}));
        }
        if one_x && psize == [w, h] {
            comp_world += 1;
            let comp = render::composite_rect(&rendered, &m.header, bx0, by0, w, h);
            if let Ok(png) = std::fs::read(assets.join(pm["composite"].as_str().unwrap())) {
                let refimg = gfx::decode_png(&png).unwrap();
                comp_n += 1;
                let mut diff = 0;
                for y in 0..h {
                    for x in 0..w {
                        let c = ctx.pal[comp[(y * w + x) as usize] as usize];
                        let r = refimg.get(x, y);
                        if [c[0], c[1], c[2]] != [r[0], r[1], r[2]] {
                            diff += 1;
                        }
                    }
                }
                if diff == 0 && all_same {
                    comp_same += 1;
                } else if diff != 0 && all_same && examples.len() < 60 {
                    examples.push(json!({"map": pm["map"], "composite_pixels_differ": diff}));
                }
            }
        }
    }
    let summary = json!({
        "python_maps_rendered": python_maps,
        "matched_to_rom_header": matched,
        "unmatched": unmatched,
        "layers": stats,
        "world_size_equals_python_composite_size": [size_ok, size_n],
        "composites_1x_same_crop": comp_world,
        "composites_compared": comp_n,
        "composites_identical": comp_same,
        "examples": examples,
        "measured_origins": origins,
        "size_diffs": size_diffs,
        "rust_maps": maps.len(),
    });
    eprintln!("{}", serde_json::to_string_pretty(&summary).unwrap());
    let _ = std::fs::write(root.join("engine/target/wf-maps-parity.json"), serde_json::to_vec_pretty(&summary).unwrap());
    assert_eq!(matched, python_maps);
}
