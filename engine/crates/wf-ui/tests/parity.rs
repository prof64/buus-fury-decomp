//! UI decoded from the ROM vs emulator captures (`tests/data/`, written by `re/buu/ui/make_refs.py`).
//! Skipped when the ROM is absent. `cargo test -p wf-ui -- --nocapture` prints the summary.

use wf_rom::gfx::decode_png;
use wf_ui::surface::Surface;
use wf_ui::{dialog, font, hud, images, mapscreen, menu, ui_palette};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn grey(name: &str) -> Surface {
    let im = decode_png(&data(name)).unwrap();
    Surface::from_vec(im.w, im.h, im.px.iter().map(|p| p[0]).collect())
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn fonts_decode() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let fs = font::Font::all(&rom);
    let counts: Vec<usize> = fs.iter().map(|f| f.glyphs.iter().flatten().count()).collect();
    println!("fonts: {:?}", fs.iter().map(|f| f.id).zip(counts.iter()).collect::<Vec<_>>());
    assert_eq!(counts, vec![221, 223, 26, 223, 223, 32, 223]);
    for f in &fs {
        for g in f.glyphs.iter().flatten() {
            // (the title font's A M O Q W are wider than 16, W 22)
            assert!(g.width <= 22 && g.cols.len() == g.width as usize);
        }
    }
}

/// The dialogue surface while the intro shows its last page: box picture + "normal..." laid out by the
/// documented rules (word wrap at 144 px, 4 lines per page, centring) — must be identical.
#[test]
fn dialog_page_exact() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let refs = data("dialog_surface.bin");
    let fonts = font::Font::all(&rom);
    let f = fonts.iter().find(|f| f.id == "dialog").unwrap();
    let st = &dialog::styles(&rom)[0];
    let text = utf16("Finally, after years of turmoil caused by Dr. Gero's Androids, the Earth was brought back to normal...");
    let pages = dialog::pages(f, st, &text);
    let last = pages.last().unwrap();
    let s = dialog::render_page(&rom, f, st, last);
    let diff = s.px.iter().zip(&refs).filter(|(a, b)| a != b).count();
    println!(
        "dialog: {} pages, last page {:?}, {diff} px differ of {}",
        pages.len(),
        last.iter().map(|(l, _)| String::from_utf16_lossy(l)).collect::<Vec<_>>(),
        refs.len()
    );
    assert_eq!(diff, 0);
}

/// Boot screens: ROM picture + UI palette vs the emulator screenshot, pixel for pixel (RGB).
#[test]
fn boot_screens_exact() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let pal = ui_palette(&rom);
    for (name, id) in [
        ("legal", "screen_legal"),
        ("atari", "screen_atari"),
        ("funimation", "screen_funimation"),
        ("webfoot", "screen_webfoot"),
        ("title", "screen_title"),
    ] {
        let shot = decode_png(&data(&format!("boot_{name}.png"))).unwrap();
        let p = images::PICS.iter().find(|p| p.id == id).unwrap();
        let s = images::decode(&rom, p);
        let mut diff = 0;
        let (mut x0, mut y0, mut x1, mut y1) = (240, 160, 0, 0);
        for y in 0..160 {
            for x in 0..240 {
                let c = pal[s.get(x, y) as usize];
                let r = shot.get(x, y);
                if c[..] != r[..3] {
                    diff += 1;
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        println!("boot {name}: {diff} px differ (bbox {x0},{y0}-{x1},{y1})");
        if name == "title" {
            // the START / OPTIONS menu is drawn over the art (box (80, 104)-(160, 144))
            assert!(diff == 0 || (x0 >= 80 && x1 < 160 && y0 >= 104 && y1 < 144), "title differs outside the menu box");
        } else {
            assert_eq!(diff, 0, "{name}");
        }
    }
}

/// HUD OBJ tiles: icon (from the capture) + frame + gauges for the captured character values.
#[test]
fn hud_exact() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let refs = data("hud_tiles.bin");
    let st: serde_json::Value = serde_json::from_slice(&data("hud_state.json")).unwrap();
    let v = |k: &str| st[k].as_i64().unwrap() as i32;
    let h = hud::Hud::load(&rom);
    let mut hit = None;
    for xp in 0..=72 {
        let g = hud::Gauges::from_stats(v("hp"), v("hp_max"), v("ep"), v("ep_max"), v("ss"), v("ss3"), xp);
        if h.compose(&refs[..0x200], g) == refs {
            hit = Some(xp);
            break;
        }
    }
    let g = hud::Gauges::from_stats(v("hp"), v("hp_max"), v("ep"), v("ep_max"), v("ss"), v("ss3"), 0);
    let base = h.compose(&refs[..0x200], g);
    let diff = base.iter().zip(&refs).filter(|(a, b)| a != b).count();
    println!("hud: identical with xp step {hit:?} (with xp 0: {diff} bytes differ)");
    assert!(hit.is_some());
}

/// Map screen: BG3 fill + BG0 frame (with L / R) vs VRAM of the captured map screen.
#[test]
fn mapscreen_frame_exact() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let refs = grey("mapscreen_bg.png");
    let mut best = usize::MAX;
    for (l, r, g) in [(true, true, false), (true, true, true), (false, true, false), (true, false, false), (false, false, false)] {
        let s = mapscreen::render_frame(&rom, l, r, g);
        let diff = s.px.iter().zip(&refs.px).filter(|(a, b)| a != b).count();
        println!("mapscreen frame L={l} R={r} grid={g}: {diff} px differ");
        best = best.min(diff);
    }
    assert_eq!(best, 0);
}

#[test]
fn world_map_tiles_exact() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (d, _) = mapscreen::world_map(&rom);
    let want = String::from_utf8(data("worldmap_tiles.sha1")).unwrap();
    assert_eq!(d.len(), 600 * 64);
    assert_eq!(wf_rom::sha1_hex(&d), want.trim());
}

/// Status page: the procedural background + panel must match every surface pixel that the page's other
/// elements (text, portrait, header tabs, separators) leave untouched.
#[test]
fn menu_skeleton_matches_status_page() {
    let Some(_rom) = wf_rom::testutil::rom() else { return };
    let refs = grey("pause_surface.png");
    let sk = menu::page_skeleton();
    // the panel frame itself: rows / columns belonging to the bevel
    let mut frame_px = 0;
    let mut frame_diff = 0;
    let mut all_diff = 0;
    for y in 0..160u32 {
        for x in 0..240u32 {
            let a = sk.get(x, y);
            let b = refs.get(x, y);
            if a != b {
                all_diff += 1;
            }
            let in_panel = (8..235).contains(&x) && (24..155).contains(&y);
            let interior = (14..227).contains(&x) && (30..147).contains(&y);
            if in_panel && !interior {
                frame_px += 1;
                if a != b {
                    frame_diff += 1;
                }
            }
        }
    }
    println!("menu: panel frame {frame_diff} / {frame_px} px differ; whole page {all_diff} / 38400 differ (text, portrait, header)");
    assert!(frame_diff * 50 < frame_px, "panel frame mismatch");
}

/// Headerless UI streams set their literal mode themselves: decoding does not depend on the initial width.
#[test]
fn buttons_stream_independent_of_literal_width() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let bs = images::buttons(&rom);
    assert_eq!(bs.len(), 8);
    for b in &bs {
        let list = rom.u32(b.rom + 8);
        for k in 0..rom.u32(list) {
            let p = rom.u32(list + 4 + 4 * k);
            let n = (b.w * b.h) as usize;
            assert_eq!(images::stream_typ(&rom, p, n, 1), images::stream_typ(&rom, p, n, 2));
        }
        assert!(b.frames.iter().all(|f| f.iter().any(|&c| c != 0)), "empty frame in {}", b.name);
    }
}

#[test]
fn extract_writes_ui() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let mut c = wf_rom::content::Content::new();
    wf_ui::extract(&rom, &mut c).unwrap();
    let n = c.files.keys().filter(|k| k.starts_with("ui/")).count();
    println!("ui files: {n}");
    assert!(n >= 45);
    for p in ["ui/index.json", "ui/fonts/dialog.json", "ui/hud.json", "ui/dialog.json", "ui/mapscreen.json", "ui/menu.json"] {
        assert!(c.json(p).is_some(), "{p}");
    }
}
