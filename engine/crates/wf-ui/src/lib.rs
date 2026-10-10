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
pub mod menupics;
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
    // the stat-points mark (object 0x0800BCC8, draw 0x0800B55C): 16x16 8bpp OBJ frames at 0x087144D8 + 0x100 k (2 x 2
    // tiles of 64 bytes), k = (draws >> 3) & 3; side by side
    let mut sm = Surface::new(64, 16);
    for k in 0..4u32 {
        let b = rom.bytes(0x0871_44D8 + 0x100 * k, 0x100);
        for (o, &c) in b.iter().enumerate() {
            let (t, p) = (o as u32 / 64, o as u32 % 64);
            sm.set((k * 16 + (t % 2) * 8 + p % 8) as i32, ((t / 2) * 8 + p / 8) as i32, c);
        }
    }
    out.put("ui/hud/stat_mark.png", sm.png(&pal, true));
    // the script markers (`0x9E` / `0x9F`, object 0x08050690, draw 0x080217AC): defs 0x08055000 / 0x0805500C (s8 origin
    // x / y, w, h, the OAM word, the 8x8 8bpp picture at +8); screen positions; side by side (A, B)
    let mut mk = Surface::new(16, 8);
    for (k, def) in [0x0805_5000u32, 0x0805_500C].into_iter().enumerate() {
        let g = u32::from_le_bytes(rom.bytes(def + 8, 4).try_into().unwrap());
        for (o, &c) in rom.bytes(g, 64).iter().enumerate() {
            mk.set((k as u32 * 8 + o as u32 % 8) as i32, (o as u32 / 8) as i32, c);
        }
    }
    out.put("ui/hud/markers.png", mk.png(&pal, true));
    // the boss HP bar (draw 0x08019658): base 0x200 bytes at 0x087196E4 (8 tiles 8x8 8bpp: two 32x8 OBJs); fill f
    // clears steps f..53 to colour 1 (4 byte offsets each at 0x087198E4 + 16 step); row f of the sheet = fill f
    let base = rom.bytes(0x0871_96E4, 0x200).to_vec();
    let steps = rom.bytes(0x0871_98E4, 53 * 16).to_vec();
    let mut bb = Surface::new(64, 8 * 54);
    for f in 0..54usize {
        let mut b = base.clone();
        for s in f..53 {
            for j in 0..4 {
                let o = u32::from_le_bytes(steps[s * 16 + j * 4..s * 16 + j * 4 + 4].try_into().unwrap()) as usize;
                if let Some(c) = b.get_mut(o) {
                    *c = 1;
                }
            }
        }
        for (o, &c) in b.iter().enumerate() {
            let (t, p) = (o / 64, o % 64);
            bb.set((t * 8 + p % 8) as i32, (f * 8 + p / 8) as i32, c);
        }
    }
    out.put("ui/hud/boss_bar.png", bb.png(&pal, true));
    out.put_json("ui/hud.json", &hud::json(&h));

    // the hit flash (object 0x0800E8E6, vtable 0x0804A7A0, tick 0x0800DE28): a 12-frame counter, decremented each
    // tick; while > 0 the actor's tiles are drawn through the colour-index map 0x080725AC[counter >> 2] (three
    // 256-byte maps at 0x08056C2C: a warm tint, the same, a red ramp), at 0 the plain tiles again. The OBJ palette
    // (0x0805652C) gives the maps' colours.
    let obj_pal: Vec<u32> = (0..256u32)
        .map(|i| {
            let [r, g, b] = bgr555(rom.u16(0x0805_652C + 2 * i));
            (r as u32) << 16 | (g as u32) << 8 | b as u32
        })
        .collect();
    let mut maps: Vec<Vec<u8>> = (0..3u32).map(|k| rom.bytes(rom.u32(0x0807_25AC + 4 * k), 256).to_vec()).collect();
    // the script tints (`tint`, table 0x08752EEC: 20 entries {RGBA key, map}) after them: maps[3 + preset], the
    // identity for a preset without a map (it restores the sprite's own colours)
    for k in 0..20u32 {
        let m = rom.u32(0x0875_2EEC + 8 * k + 4);
        maps.push(if m != 0 { rom.bytes(m, 256).to_vec() } else { (0..=255u8).collect() });
    }
    out.put_json(
        "ui/hit_flash.json",
        &json!({"obj_palette": obj_pal, "maps": maps, "counter": 12,
                "rule": "counter 12 at the hit, -1 per tick; drawn through maps[counter >> 2] while > 0; maps[3 + k]: tint preset k (0x08752EEC)",
                "rom": {"object": "0x0800E8E6", "tick": "0x0800DE28", "maps": "0x080725AC", "obj_palette": "0x0805652C"}}),
    );

    // area-name banner (layout constants from its object, vt 0x0804B528)
    out.put_json("ui/banner.json", &banner_json());

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
    mapscreen::area_maps(rom, out, &pal);
    mapscreen::frame_variants(rom, out, &pal);

    // menu primitives
    out.put("ui/menu/page_skeleton.png", menu::page_skeleton().png(&pal, false));
    menupics::minigame::extract(rom, out, &pal);
    menupics::minigame::scouter(rom, out, &pal);
    menupics::opts::extract(rom, out, &pal);
    menupics::screens::extract(rom, out, &pal);
    // the menu screens' pictures (menupics/): raw indices, frames stacked vertically
    let mut mp = Vec::new();
    for p in menupics::all() {
        let n = (p.w * p.h) as usize;
        let mut px = Vec::with_capacity(n * p.frames as usize);
        for k in 0..p.frames {
            if p.stride == 0 {
                let mut d = rom.lz(p.rom).unwrap_or_default();
                d.resize(n * p.frames as usize, 0);
                px = d;
                break;
            }
            let mut d = rom.lz(p.rom + k * p.stride).unwrap_or_default();
            d.resize(n, 0);
            px.extend_from_slice(&d);
        }
        out.put(format!("ui/menu/{}.png", p.id), Surface::from_vec(p.w, p.h * p.frames, px).png(&pal, true));
        mp.push(json!({"id": p.id, "png": format!("menu/{}.png", p.id), "size": [p.w, p.h], "frames": p.frames,
                       "layout": "frames stacked vertically", "rom": hx(p.rom), "desc": p.desc}));
    }
    let mut mj = menu_json();
    mj["pictures"] = Value::Array(mp);
    out.put_json("ui/menu.json", &mj);

    index.insert(
        "files".into(),
        json!({"palette": "palette.json", "dialog": "dialog.json", "hud": "hud.json", "mapscreen": "mapscreen.json", "menu": "menu.json"}),
    );
    index.insert("palette".into(), json!("palette.json (0x0805652C); every PNG here uses it unless noted"));
    out.put_json("ui/index.json", &Value::Object(index));
    Ok(())
}

/// Area-name banner (object ctor `0x0800BF34`, vt `0x0804B528`, surface `0x0800BE90`, draw `0x0800BFCE`).
fn banner_json() -> Value {
    json!({
        "image": "images/banner.png",
        "surface": {"size": [240, 16], "background": "images/banner.png (LZ 0x0807248C, type 1, 0xF00 bytes)"},
        "text": {"font": "fonts/small.json", "string": "the area's name string id (areas.json name_string_id)",
                 "align": "centre: x0 = 120 - (width + 1) / 2, width = sum(glyph width + 1) (string drawer 0x0803DADA mode 1)",
                 "pen_y": 11, "glyph_top": "pen y - 12", "color": 2, "shadow_color": 1, "shadow_offset": [1, 1],
                 "note": "per glyph the shadow pass is drawn first, then the colour pass (glyph fn 0x0803F5A4)"},
        "screen": {"objs": "7 x 32x16 + 1 x 16x16 8bpp affine OBJs at x 0, 32, .. 224, y 128", "priority": 0, "mode": "normal (no blending)"},
        "timing": {"timer_start": 240, "update": "timer -= 1 before each draw; removed when it reaches 0 (239 visible frames)",
                   "squash": "affine pa = 0x100, pd = 0x100 while timer > 16, else 0x1000 / timer (the strip collapses to its centre line)",
                   "frame_t": "t = 239 - timer = frames since the first draw (0..238)"},
        "created": "EnterArea 0x08008D44 after the player and the HUD, when the new area's name id (area +0xC) differs from the last shown one (0x03001984) and is not 0; the 3D world map (map +0x3C) creates its own label object 0x0800C0C8 instead and only records the id",
        "rom": {"ctor": "0x0800BF34", "vtable": "0x0804B528", "surface": "0x0800BE90", "update": "0x0800BF94", "draw": "0x0800BFCE"}
    })
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
