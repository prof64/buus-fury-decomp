//! Emulator parity of the frontend renderer (headless): `cargo run --release -p wf-app --example parity --
//! <content dir> <work/frontend dir> [out dir] [scene...]`. Scenes come from `crates/wf-render/re/capture.py`
//! (emulator screenshot + player / camera / record state). For each scene the map is identified, the player's
//! sprite frame, halo, shadow and HUD are rebuilt from content into a `RenderView`, rendered 240x160 and diffed
//! against the emulator screen: whole frame, HUD rectangle, the player's box and the rest. Writes
//! `<out>/<scene>.png` (emulator | ours | diff).

use std::path::{Path, PathBuf};

use wf_app::wf_content::sprite::{SpriteDef, DIRS};
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

fn diff(a: &[u32], b: &[u32], rect: Option<(i32, i32, i32, i32)>, exclude: &[(i32, i32, i32, i32)]) -> usize {
    let mut n = 0;
    for y in 0..160i32 {
        for x in 0..240i32 {
            let inr = |r: &(i32, i32, i32, i32)| x >= r.0 && y >= r.1 && x < r.2 && y < r.3;
            if rect.is_some_and(|r| !inr(&r)) || exclude.iter().any(inr) {
                continue;
            }
            let i = (y * 240 + x) as usize;
            if a[i] & 0xFFFFFF != b[i] & 0xFFFFFF {
                n += 1;
            }
        }
    }
    n
}

struct Scene {
    af: u32,
    map: String,
    cam: (i32, i32),
    player: Option<(String, i32, i32, u8, i32, usize)>,
    hud: Option<HudView>,
    dialog: Option<DialogView>,
}

fn view(s: &Scene, packs: &Packs) -> RenderView {
    let mut v = RenderView::default();
    v.map = Some(MapView { id: s.map.clone(), anim_frame: s.af, tile_frame: None, shake: (0, 0), shake_sub: (0, 0), mode7: None, layers: None, frame: None });
    v.cameras.push(CameraView { x: s.cam.0, y: s.cam.1 });
    if let Some((key, x, y, dir, raw, step)) = &s.player {
        if let Ok(def) = SpriteDef::load(packs, key) {
            if let Some((_, a)) = def.anim_by_raw(*raw) {
                let steps = a.steps(*dir);
                if let Some(st) = steps.get(*step).or(steps.last()) {
                    // OAM order of an actor: halo, body, shadow (lower index on top)
                    if let (Some(at), Some(_)) = (st.attach, &def.attachment) {
                        v.sprites.push(SpriteDraw {
                            entity: 1,
                            sheet: "effects/halo".into(),
                            frame: FrameSel::Frame { index: 0, flip: false },
                            x: x + at[0],
                            y: y + at[1],
                            priority: 3,
                            mirror: false,
                            effect: SpriteEffect::None,
                        });
                    }
                    v.sprites.push(SpriteDraw {
                        entity: 1,
                        sheet: key.clone(),
                        frame: FrameSel::Frame { index: st.frame as u16, flip: st.flip },
                        x: *x,
                        y: *y,
                        priority: 3,
                        mirror: false,
                        effect: SpriteEffect::None,
                    });
                    if let Some(sh) = &def.shadow {
                        v.sprites.push(SpriteDraw {
                            entity: 1,
                            sheet: format!("effects/{sh}"),
                            frame: FrameSel::Frame { index: 0, flip: false },
                            x: *x,
                            y: *y,
                            priority: 3,
                            mirror: false,
                            effect: SpriteEffect::None,
                        });
                    }
                }
            }
        }
    }
    if let Some(h) = &s.hud {
        v.hud.push(h.clone());
    }
    v.dialog = s.dialog.clone();
    v
}

#[allow(dead_code)]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let content = PathBuf::from(&a[0]);
    let work = PathBuf::from(&a[1]);
    let out = PathBuf::from(a.get(2).cloned().unwrap_or("/tmp/parity".into()));
    let only: Vec<String> = a.iter().skip(3).cloned().collect();
    let packs = Packs::new(vec![Box::new(DirSource(content.join("base"))) as Box<dyn FileSource>]).unwrap();
    run(&packs, &work, Some(&out), &only);
}

/// Per scene: (name, total differing pixels, differing pixels outside OBJs not rebuilt (other actors)).
pub struct ParityResult {
    pub name: String,
    pub total: usize,
    pub hud: usize,
    pub player: usize,
    pub background: usize,
}

pub fn run(packs: &Packs, work: &Path, out: Option<&Path>, only: &[String]) -> Vec<ParityResult> {
    let packs = packs;
    if let Some(o) = out {
        std::fs::create_dir_all(o).unwrap();
    }
    let mut results = Vec::new();
    let maps: Vec<String> = {
        let j: serde_json::Value = packs.json("maps/index.json").unwrap();
        j["maps"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap().to_string()).collect()
    };
    let mut r = Renderer::new();
    let mut names: Vec<String> = std::fs::read_dir(&work)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".png")).map(String::from))
        .collect();
    names.sort();
    let mut map_cache: std::collections::HashMap<String, String> = Default::default();
    for name in names {
        if !only.is_empty() && !only.iter().any(|o| *o == name) {
            continue;
        }
        let emu = load_png(&work.join(format!("{name}.png")));
        let j: serde_json::Value = serde_json::from_slice(&std::fs::read(work.join(format!("{name}.json"))).unwrap()).unwrap();
        let ram_cam = (j["camera"][0].as_i64().unwrap() as i32, j["camera"][1].as_i64().unwrap() as i32);
        let mut cam = ram_cam;
        let p = &j["player"];
        let dialog_scene = name.starts_with("dialog");
        let hud_rect = (8, 8, 88, 24);
        let dlg_rect = (40, 8, 200, 72);
        // 1. map: best background match (outside HUD / dialogue / OBJs)
        let state = j["state"].as_str().unwrap().to_string();
        let mut excl: Vec<(i32, i32, i32, i32)> = vec![hud_rect, dlg_rect];
        for o in j["oam"].as_array().unwrap() {
            let (x, y) = (o["x"].as_i64().unwrap() as i32, o["y"].as_i64().unwrap() as i32);
            let x = if x >= 256 { x - 512 } else { x };
            let y = if y >= 160 { y - 256 } else { y };
            excl.push((x, y, x + 64, y + 64));
        }
        let mut af = 0u32;
        let map = map_cache.get(&state).cloned().unwrap_or_else(|| {
            let mut best = (usize::MAX, String::new());
            for m in &maps {
                let s = Scene { af, map: m.clone(), cam, player: None, hud: None, dialog: None };
                let v = view(&s, packs);
                r.draw_hud = false;
                let fb = r.render(packs, &v, 0, 240, 160).to_vec();
                let d = diff(&fb, &emu, None, &excl);
                if d < best.0 {
                    best = (d, m.clone());
                }
            }
            r.draw_hud = true;
            best.1
        });
        map_cache.insert(state.clone(), map.clone());
        // the screen shows the camera of the previous frame (BG scroll and OAM are written before the camera
        // update): pick the camera within 1 px of the RAM value that matches the background
        let mut best_cam = (usize::MAX, cam);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let c = (ram_cam.0 + dx, ram_cam.1 + dy);
                let s = Scene { af, map: map.clone(), cam: c, player: None, hud: None, dialog: None };
                r.draw_hud = false;
                let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
                let d = diff(&fb, &emu, None, &excl);
                if d < best_cam.0 {
                    best_cam = (d, c);
                }
            }
        }
        r.draw_hud = true;
        cam = best_cam.1;
        // tile-animation phase (the map's animation timers are not captured): best background match
        let mut best_af = (best_cam.0, 0u32);
        for t in 1..720u32 {
            af = t;
            let s = Scene { af, map: map.clone(), cam, player: None, hud: None, dialog: None };
            r.draw_hud = false;
            let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
            let d = diff(&fb, &emu, None, &excl);
            if d < best_af.0 {
                best_af = (d, t);
            }
            if d == 0 {
                break;
            }
        }
        r.draw_hud = true;
        af = best_af.1;
        // 2. player sprite: Goku forms (with / without halo)
        let raw = p["raw"].as_i64().unwrap_or(-1) as i32;
        let (px, py, dir, step) =
            (p["x"].as_i64().unwrap() as i32, p["y"].as_i64().unwrap() as i32, p["dir"].as_u64().unwrap() as u8, p["step"].as_u64().unwrap() as usize);
        let rec = &j["record"];
        let iv = |k: &str| rec[k].as_i64().unwrap_or(0) as i32;
        let mut hud = HudView { hp: iv("hp"), hp_max: iv("hp_max"), ep: iv("ep"), ep_max: iv("ep_max"), ss: iv("ss"), ss3: iv("ss3"), ..Default::default() };
        let player_box = (px - cam.0 - 24, py - cam.1 - 48, px - cam.0 + 40, py - cam.1 + 16);
        let mut best_sprite = (usize::MAX, None);
        if !dialog_scene && raw >= 0 {
            let forced = std::env::var("PARITY_SPRITE").ok();
            let cands: Vec<String> = match &forced {
                Some(f) => vec![f.clone()],
                None => ["sprites/106_goku", "sprites/107_goku_other_world", "sprites/108_goku_halo", "sprites/109_ss_goku", "sprites/110_ss_goku_halo"].map(String::from).to_vec(),
            };
            for key in &cands {
                let s = Scene { af, map: map.clone(), cam, player: Some((key.clone(), px, py, dir, raw, step)), hud: None, dialog: None };
                r.draw_hud = false;
                let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
                let d = diff(&fb, &emu, Some(player_box), &[hud_rect]);
                if d < best_sprite.0 {
                    best_sprite = (d, Some(key.clone()));
                }
            }
            r.draw_hud = true;
        }
        let player = best_sprite.1.clone().map(|k| (k, px, py, dir, raw, step));
        // 3. HUD: the selected skill icon / experience steps are not in the capture: search them
        let mut best_hud = (usize::MAX, hud.clone());
        if !dialog_scene {
            let mut skills: Vec<Option<u8>> = vec![None];
            skills.extend((1..=30u8).map(Some));
            for sk in skills {
                for alt in [false, true] {
                    if sk.is_none() && alt {
                        continue;
                    }
                    for xp in [0u8, 72] {
                        hud.skill = sk;
                        hud.skill_alt = alt;
                        hud.xp72 = xp;
                        let s = Scene { af, map: map.clone(), cam, player: player.clone(), hud: Some(hud.clone()), dialog: None };
                        let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
                        let d = diff(&fb, &emu, Some(hud_rect), &[]);
                        if d < best_hud.0 {
                            best_hud = (d, hud.clone());
                        }
                    }
                }
            }
            // refine xp
            let mut h = best_hud.1.clone();
            for xp in 0..=72u8 {
                h.xp72 = xp;
                let s = Scene { af, map: map.clone(), cam, player: player.clone(), hud: Some(h.clone()), dialog: None };
                let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
                let d = diff(&fb, &emu, Some(hud_rect), &[]);
                if d < best_hud.0 {
                    best_hud = (d, h.clone());
                }
            }
        }
        let dialog = dialog_scene.then(|| DialogView { text: "normal...".into(), style: 0, page: 0, position: DialogPos::Top, ..Default::default() });
        let s = Scene { af, map: map.clone(), cam, player, hud: (!dialog_scene).then(|| best_hud.1.clone()), dialog };
        let fb = r.render(packs, &view(&s, packs), 0, 240, 160).to_vec();
        let total = diff(&fb, &emu, None, &[]);
        let d_hud = diff(&fb, &emu, Some(hud_rect), &[]);
        let d_dlg = diff(&fb, &emu, Some(dlg_rect), &[]);
        let d_player = diff(&fb, &emu, Some(player_box), &[hud_rect]);
        let d_rest = diff(&fb, &emu, None, &excl.iter().copied().chain([player_box]).collect::<Vec<_>>());
        println!(
            "{name:22} cam {:?}{} t {af:3} map {map:14} sprite {:24} total {total:5}  hud {d_hud:4}  dialog {d_dlg:5}  player {d_player:4}  bg-only {d_rest:5}  (skill {:?}{} xp {})",
            cam,
            if cam != ram_cam { "*" } else { " " },
            best_sprite.1.unwrap_or("-".into()),
            best_hud.1.skill,
            if best_hud.1.skill_alt { " alt" } else { "" },
            best_hud.1.xp72
        );
        let dimg: Vec<u32> = fb.iter().zip(&emu).map(|(a, b)| if a & 0xFFFFFF == b & 0xFFFFFF { (a >> 2) & 0x3F3F3F | 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
        if let Some(o) = out {
            save(&o.join(format!("{name}.png")), &[&emu, &fb, &dimg]);
        }
        let _ = DIRS;
        results.push(ParityResult { name, total, hud: d_hud, player: d_player, background: d_rest });
    }
    results
}
