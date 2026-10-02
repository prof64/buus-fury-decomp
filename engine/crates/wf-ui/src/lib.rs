//! Buu's Fury user interface decoded from the ROM: fonts, dialogue boxes, HUD, menu primitives, map screen,
//! boot / title screens, button prompts, icons. See `engine/docs/ui.md`.
//!
//! The game draws all UI in software into 8bpp surfaces (menus: a 240×160 surface shown as BG2; dialogue: a
//! 160×64 surface shown as OBJs; HUD: an 80×16 OBJ picture rebuilt every frame) from LZ pictures, 1bpp fonts and
//! a few procedural primitives. Pictures and fonts are exported as PNG; layouts are code constants, exported as
//! JSON with the code addresses they come from.

pub mod dialog;
pub mod font;
pub mod hud;
pub mod images;
pub mod mapscreen;
pub mod menu;
pub mod surface;

use serde_json::{json, Value};
use surface::Surface;
use wf_rom::content::Content;
use wf_rom::gfx::{bgr555, Rgb};
use wf_rom::{hx, Rom};

/// The UI palette: 256 BGR555 colours at `0x0805652C`. Menus and the boot / title / map screens load it into
/// both palette halves (`0x08024BD0`); the in-game OBJ half of the global palette (`0x080701B0`) is the same
/// colours (255 / 256 entries identical), which is what the HUD and dialogue boxes use.
pub const UI_PALETTE: u32 = 0x0805_652C;

pub fn ui_palette(rom: &Rom) -> Vec<Rgb> {
    (0..256).map(|i| bgr555(rom.u16(UI_PALETTE + 2 * i))).collect()
}

fn hexrgb(c: &Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Write everything under `ui/`.
pub fn extract(rom: &Rom, out: &mut Content) -> wf_rom::Result<()> {
    let pal = ui_palette(rom);
    let mut index = serde_json::Map::new();
    out.put_json(
        "ui/palette.json",
        &json!({"rom": hx(UI_PALETTE), "format": "BGR555 x 256 (expanded c<<3|c>>2)",
                "note": "loaded into both palette halves by menus / boot / map screen; equals the OBJ half of the in-game palette 0x080701B0 (HUD, dialogue)",
                "colors": pal.iter().map(hexrgb).collect::<Vec<_>>()}),
    );

    // fonts
    let mut fonts = Vec::new();
    let mono: Vec<Rgb> = vec![[0, 0, 0], [255, 255, 255]];
    for f in font::Font::all(rom) {
        let (atlas, meta) = font::atlas(&f);
        out.put(format!("ui/fonts/{}.png", f.id), atlas.png(&mono, true));
        out.put_json(format!("ui/fonts/{}.json", f.id), &meta);
        fonts.push(json!({"id": f.id, "json": format!("fonts/{}.json", f.id), "atlas": format!("fonts/{}.png", f.id),
                          "glyphs": f.glyphs.iter().flatten().count(), "rom": hx(f.rom)}));
    }
    index.insert("fonts".into(), fonts.into());

    // pictures
    let mut pics = Vec::new();
    for p in images::PICS {
        let s = images::decode(rom, p);
        out.put(format!("ui/images/{}.png", p.id), s.png(&pal, !p.opaque));
        pics.push(json!({"id": p.id, "png": format!("images/{}.png", p.id), "size": [p.w, p.h], "frames": p.frames,
                         "layout": if p.frames > 1 { "frames stacked vertically" } else { "single" },
                         "transparent_index0": !p.opaque, "desc": p.desc, "rom": hx(p.rom), "code": p.code}));
    }
    // dialogue boxes
    let styles = dialog::styles(rom);
    for st in &styles {
        let s = dialog::box_image(rom, st);
        let id = format!("dialog_box_{}", st.index);
        out.put(format!("ui/images/{id}.png"), s.png(&pal, true));
        pics.push(json!({"id": id, "png": format!("images/{id}.png"), "size": [160, 64], "frames": 1,
                         "transparent_index0": true, "desc": format!("dialogue box style {} ({} lines)", st.index, st.max_lines),
                         "rom": hx(st.image), "code": "style table 0x08071730"}));
    }
    index.insert("images".into(), pics.into());
    out.put_json("ui/dialog.json", &dialog::json(&styles));

    // button prompts
    let mut btns = Vec::new();
    for b in images::buttons(rom) {
        let n = b.frames.len() as u32;
        let mut s = Surface::new(b.w * n, b.h);
        for (k, f) in b.frames.iter().enumerate() {
            s.blit(&Surface::from_vec(b.w, b.h, f.clone()), (k as u32 * b.w) as i32, 0, false);
        }
        out.put(format!("ui/buttons/{}.png", b.name), s.png(&pal, true));
        btns.push(json!({"id": b.id, "button": b.name, "key_mask": hx(b.key as u32), "png": format!("buttons/{}.png", b.name),
                         "size": [b.w, b.h], "frames": n, "layout": "frames side by side", "rom": hx(b.rom)}));
    }
    index.insert("buttons".into(), json!({"table": hx(images::BUTTONS), "entries": btns}));

    // HUD
    let h = hud::Hud::load(rom);
    let mut frame = vec![0u8; 0x500];
    frame[0x200..].copy_from_slice(&h.frame);
    out.put("ui/hud/frame.png", hud::to_picture(&frame).png(&pal, true));
    out.put("ui/hud/gauge_ss.png", hud::to_picture(&h.img_ss).png(&pal, true));
    out.put("ui/hud/gauge_ss3.png", hud::to_picture(&h.img_ss3).png(&pal, true));
    out.put_json("ui/hud.json", &hud::json(&h));

    // map screen
    let fr = mapscreen::render_frame(rom, true, true, true);
    out.put("ui/mapscreen/frame.png", fr.png(&pal, false));
    let tiles = rom.lz(mapscreen::FRAME_TILES).unwrap_or_default();
    let nt = (tiles.len() / 64) as u32;
    out.put("ui/mapscreen/frame_tiles.png", Surface::from_tiles(&tiles, nt.min(16), nt.div_ceil(16)).png(&pal, true));
    let map = rom.lz(mapscreen::FRAME_MAP).unwrap_or_default();
    let ents: Vec<u16> = map.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    out.put_json(
        "ui/mapscreen/frame_map.json",
        &json!({"width": 32, "height": 32, "tile_base_added": 0x378, "tiles_png": "frame_tiles.png",
                "entries": "u16 GBA text-BG entries before adding tile_base (bits 0-9 tile, 10 hflip, 11 vflip)", "data": ents}),
    );
    for (name, a) in [("button_l", mapscreen::BTN_L), ("button_r", mapscreen::BTN_R)] {
        let d = rom.lz(a).unwrap_or_default();
        out.put(format!("ui/mapscreen/{name}.png"), Surface::from_tiles(&d[..0x100.min(d.len())], 2, 2).png(&pal, true));
    }
    let (_, wm) = mapscreen::world_map(rom);
    out.put("ui/mapscreen/world_map.png", wm.png(&pal, false));
    out.put_json("ui/mapscreen.json", &mapscreen::json());

    // menu primitives
    out.put("ui/menu/page_skeleton.png", menu::page_skeleton().png(&pal, false));
    out.put_json("ui/menu.json", &menu_json());

    index.insert(
        "files".into(),
        json!({"palette": "palette.json", "dialog": "dialog.json", "hud": "hud.json", "mapscreen": "mapscreen.json", "menu": "menu.json"}),
    );
    index.insert("palette".into(), json!("palette.json (0x0805652C); every PNG here uses it unless noted"));
    out.put_json("ui/index.json", &Value::Object(index));
    Ok(())
}

fn menu_json() -> Value {
    json!({
        "surface": {"size": [240, 160], "bg": 2, "tiles": "1..600, identity tilemap (screenblock 30)", "palette": "palette.json",
                    "note": "every pause-menu page is drawn by code into this surface, then uploaded"},
        "primitives": {
            "background": {"code": "0x08004F1C", "rule": "rows alternate colour 0x01 / 0x30, starting with 0x01"},
            "panel": {"code": "0x08004F50", "rule": "bevelled frame: outer 0x1D, 0x6E, 0x3F, dark 0x01 (2 rows), inner line 0x30; 3-px 0x01 shadow right and below; see wf-ui menu.rs"},
            "skeleton_png": "menu/page_skeleton.png (background (0,0,240,160) + panel (8,24)-(232,152))",
        },
        "pages": {
            "order": ["Status", "Inventory", "Equip", "Options", "Journal"],
            "navigation": "L / R switch pages; header shows the previous page name left, next page right, with the L / R button prompts (buttons/l.png, buttons/r.png)",
            "status": {"code": "0x08006718", "background": [0, 0, 240, 160], "panel": [8, 24, 232, 152],
                        "title": {"font": "bold", "align": "center", "x": 120, "y": 16, "colour": 5, "string_table": "page name ids"},
                        "portrait": {"rect": [16, 64, 80, 128], "source": "portraits/<character>.png (64x64)", "code_rect": "0x086B805C"}},
        },
        "fonts": {"titles": "bold", "body": "menu", "captions": "small"},
        "text_colours": {"2": "normal", "3": "level / highlight", "5": "labels (yellow)", "6": "values", "7": "highlight", "0x1D": "stat names"},
        "gaps": "only the shared skeleton and the Status constants are exported; other pages' element positions are documented in engine/docs/ui.md or still code-only",
    })
}
