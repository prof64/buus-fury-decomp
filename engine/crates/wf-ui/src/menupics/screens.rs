//! Shop, save / load, title, character switch, game over.
//!
//! * `ui/menu/title_shade.json`: the 256-entry colour remap the title menus darken their box with (`0x0803F9D4`,
//!   table `0x08056A2C`, box (80, 104)-(160, 144)).
//! * `ui/menu/silhouette.json`: the OBJ remap of party members that cannot be chosen (`0x0800EECC`, `0x0805682C`).
//! * `ui/menu/slot_corner.png`: the 4 corner markers around the chosen character of a save slot (raw 8x8 8bpp, table
//!   `0x080561FC`: {s8 dx, s8 dy, w, h, ..., ptr}; drawn at base − (dx, dy)), frames stacked vertically.

use super::MenuPic;
use crate::surface::Surface;
use wf_rom::content::Content;
use wf_rom::gfx::Rgb;
use wf_rom::Rom;

pub const PICS: &[MenuPic] = &[];

pub const TITLE_SHADE: u32 = 0x0805_6A2C;
pub const SILHOUETTE: u32 = 0x0805_682C;
pub const CORNERS: u32 = 0x0805_61FC;

pub fn extract(rom: &Rom, out: &mut Content, pal: &[Rgb]) {
    let t: Vec<u8> = (0..256u32).map(|i| rom.u8(TITLE_SHADE + i)).collect();
    out.put_json(
        "ui/menu/title_shade.json",
        &serde_json::json!({"rom": format!("0x{TITLE_SHADE:08X}"), "code": "0x0803F9D4", "rect": [80, 104, 160, 144],
                            "note": "pixel = table[pixel] over the rect (title / Single Player menus)", "table": t}),
    );
    let s: Vec<u8> = (0..256u32).map(|i| rom.u8(SILHOUETTE + i)).collect();
    out.put_json(
        "ui/menu/silhouette.json",
        &serde_json::json!({"rom": format!("0x{SILHOUETTE:08X}"), "code": "0x0800EECC", "note": "OBJ pixel index -> table[index]", "table": s}),
    );
    let mut px = Vec::new();
    let mut offs = Vec::new();
    for k in 0..4u32 {
        let e = CORNERS + 12 * k;
        offs.push([rom.u8(e) as i8 as i32, rom.u8(e + 1) as i8 as i32]);
        let p = rom.u32(e + 8);
        px.extend((0..64u32).map(|i| rom.u8(p + i)));
    }
    out.put("ui/menu/slot_corner.png", Surface::from_vec(8, 32, px).png(pal, true));
    out.put_json("ui/menu/slot_corner.json", &serde_json::json!({"rom": format!("0x{CORNERS:08X}"), "offsets": offs, "note": "OBJ at base - offset"}));
}
