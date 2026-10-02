//! Dialogue boxes with a speaker portrait vs the emulator (headless; skipped without the ROM or without the captures
//! of `crates/wf-render/re/capture_dialog_portrait.py` in `<repo>/work/frontend/dialog_portrait/`,
//! `WF_DIALOG_PORTRAIT_DIR` overrides). Each capture is a game frame with BG VRAM cleared (the box, the portrait and
//! other OBJs over the backdrop): intro cutscene lines (portrait right, bottom), map-NPC talks from below / above /
//! the left (top-left, bottom-right, top-right) and a Yes / No choice, fully typed and mid open-zoom.
//! Checks, per capture: wf-core's placement rule (`scene::speaker_layout`: top / bottom, portrait side) from the
//! node arg + the talk anchor the game used, the form-slot portrait lookup (arg 1..6), and the renderer's 224-px
//! box + portrait span pixel for pixel (pixels where we draw nothing and another OBJ shows are not judged).
//! `DIALOG_PORTRAIT_OUT=<dir>` writes emulator | ours strips.

use serde_json::Value;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_render::Renderer;
use wf_view::{DialogAnim, DialogPos, DialogView, RenderView};

fn load_png(p: &std::path::Path) -> Option<Vec<u32>> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let ch = info.color_type.samples();
    Some(buf.chunks(ch).take(240 * 160).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect())
}

fn save(p: &std::path::Path, imgs: &[&[u32]]) {
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
    let mut e = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(p).unwrap()), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

#[test]
fn dialog_portraits_match_emulator() {
    let dir = std::env::var("WF_DIALOG_PORTRAIT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/frontend/dialog_portrait"));
    let Ok(idx) = std::fs::read(dir.join("index.json")) else { return };
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let caps: Vec<Value> = serde_json::from_slice(&idx).unwrap();
    let opts = wf_extract::Options { only: ["tables", "portraits", "ui"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let out = std::env::var("DIALOG_PORTRAIT_OUT").ok().map(std::path::PathBuf::from);
    let mut r = Renderer::new();
    let mut bad = vec![];
    for cap in &caps {
        let name = cap["name"].as_str().unwrap();
        let Some(emu) = load_png(&dir.join(cap["png"].as_str().unwrap())) else { continue };
        let arg = i(&cap["arg"]) as u8;
        let global = i(&cap["global"]) as u8;
        let anchor = [i(&cap["anchor"][0]) as i32, i(&cap["anchor"][1]) as i32];
        let cy = i(&cap["centre"][1]) as i32;
        let ox: Vec<i64> = cap["obj_x"].as_array().unwrap().iter().map(i).collect();
        // wf-core: placement from (arg, anchor); portrait left = the portrait OBJ (+0x74) before the box (+0x68)
        let prefix = match cap["prefix"].as_str() {
            Some("!") => Some(wf_core::scene::BoxPos::Top),
            Some("@") => Some(wf_core::scene::BoxPos::Middle),
            Some("#") => Some(wf_core::scene::BoxPos::Bottom),
            _ => None,
        };
        let has_portrait = cap["portrait"].as_bool().unwrap_or(true);
        let (pos, left) = wf_core::scene::speaker_layout(prefix, arg, anchor);
        let want_pos = match cy {
            40 => wf_core::scene::BoxPos::Top,
            80 => wf_core::scene::BoxPos::Middle,
            _ => wf_core::scene::BoxPos::Bottom,
        };
        assert_eq!(pos, want_pos, "{name}: placement (arg {arg}, anchor {anchor:?})");
        if has_portrait {
            assert_eq!(left, ox[3] < ox[0], "{name}: portrait side (arg {arg}, anchor {anchor:?})");
        }
        // arg 1..6: the leader's form slot (the intro plays Goku, form 0)
        if (1..=6).contains(&arg) {
            let f: Value = packs.value("forms/00_goku.json").unwrap();
            assert_eq!(i(&f["portrait_or_anim_bytes"][arg as usize - 1]) as u8, global, "{name}: form portrait slot {arg}");
        }
        let pa = i(&cap["pa"]) as i32;
        let settled = pa == 256;
        let mut text = cap["page_text"].as_str().or(cap["full_text"].as_str()).unwrap().to_string();
        if i(&cap["centred"]) != 0 {
            text.insert(0, '^');
        }
        let mut v = RenderView::default();
        v.dialog = Some(DialogView {
            text,
            style: i(&cap["style"]) as u8,
            page: 0,
            position: match want_pos {
                wf_core::scene::BoxPos::Top => DialogPos::Top,
                wf_core::scene::BoxPos::Middle => DialogPos::Middle,
                wf_core::scene::BoxPos::Bottom => DialogPos::Bottom,
            },
            portrait: has_portrait.then(|| global as u16 - 8),
            portrait_left: left && has_portrait,
        });
        v.dialog_anim = Some(DialogAnim { scale: (2048 / pa) as u8, typed: if settled { None } else { Some(0) } });
        let fb = r.render(&packs, &v, 0, 240, 160).to_vec();
        // other OBJs (HUD, actors, banner): pixels inside them where we draw nothing are not judged
        let foreign: Vec<[i64; 4]> = cap["oam_shown"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| i(&o["i"]) >= if has_portrait { 4 } else { 3 })
            .map(|o| [i(&o["x"]), i(&o["y"]), i(&o["x"]) + i(&o["w"]), i(&o["y"]) + i(&o["h"])])
            .collect();
        // opening zoom: the box surface is still being filled in the game (black / partly striped interior, not
        // modelled): judge the portrait OBJ only (its zoom geometry and pixels); no-portrait zoom frames are skipped
        let oam = cap["oam_shown"].as_array().unwrap();
        let rect = if settled {
            [8, cy - 32, 232, cy + 32]
        } else if has_portrait {
            let o = &oam[3];
            [i(&o["x"]) as i32, i(&o["y"]) as i32, (i(&o["x"]) + i(&o["w"])) as i32, (i(&o["y"]) + i(&o["h"])) as i32]
        } else {
            continue;
        };
        let (mut diff, mut skipped) = (0, 0);
        for y in rect[1].max(0)..rect[3].min(160) {
            for x in rect[0].max(0)..rect[2].min(240) {
                let k = (y * 240 + x) as usize;
                if fb[k] & 0xFF_FFFF == emu[k] & 0xFF_FFFF {
                    continue;
                }
                let (xx, yy) = (x as i64, y as i64);
                if fb[k] & 0xFF_FFFF == 0 && foreign.iter().any(|o| o[0] <= xx && xx < o[2] && o[1] <= yy && yy < o[3]) {
                    skipped += 1;
                } else {
                    diff += 1;
                }
            }
        }
        println!("{name:16} arg {arg:3} portrait {:4} pa {pa:4} {:6?} left {left:5}  diff {diff:5}  (not judged {skipped})", if has_portrait { global as i32 - 8 } else { -1 }, pos);
        if let Some(o) = &out {
            std::fs::create_dir_all(o).unwrap();
            save(&o.join(format!("{name}.png")), &[&emu, &fb]);
        }
        if diff != 0 {
            bad.push(format!("{name}: {diff} px"));
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

/// The core end to end (story mode): talking to the Yemma's Office NPC at (184, 400) from just below it (like the
/// `yemma_below` capture) opens its text (arg 162) at the top with portrait 154 on the left; from above, at the
/// bottom with the portrait on the right.
#[test]
fn core_talk_fills_portrait_and_placement() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let opts = wf_extract::Options {
        only: ["tables", "sprites", "effects", "items", "maps", "ui", "world", "scripts", "portraits"].map(String::from).to_vec(),
    };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::game_data(&packs).unwrap();
    for (pos, facing, want_pos, want_left) in [((185, 409), 1u8, DialogPos::Top, true), ((178, 389), 0u8, DialogPos::Bottom, false)] {
        let cfg = wf_core::field::FieldConfig { world: 1, area: 3, variant: 1, pos: Some(pos), facing, ..Default::default() };
        let s = wf_core::session::Session::new_field(data.clone(), cfg);
        let mut sim = wf_app::story::StorySim::with_session(&packs, s).unwrap();
        sim.skip_area_banner();
        let (mut ev, mut v, mut got) = (vec![], RenderView::default(), None);
        for f in 0..90 {
            wf_view::Sim::step(&mut sim, &[if (2..4).contains(&f) { 1 } else { 0 }], &mut ev);
            wf_view::Sim::view(&sim, &mut v);
            if let Some(d) = &v.dialog {
                got = Some((d.position, d.portrait, d.portrait_left));
            }
        }
        assert_eq!(got, Some((want_pos, Some(154), want_left)), "talk from {pos:?} facing {facing}");
    }
}
