//! Shop, save / load, title, character switch, game over.
//!
//! `ui/menu/title_shade.json`: the 256-entry colour remap the title menus darken their box with (`0x0803F9D4`,
//! table `0x08056A2C`, box (80, 104)-(160, 144)).

use super::MenuPic;
use wf_rom::content::Content;
use wf_rom::Rom;

pub const PICS: &[MenuPic] = &[];

pub const TITLE_SHADE: u32 = 0x0805_6A2C;

pub fn extract(rom: &Rom, out: &mut Content) {
    let t: Vec<u8> = (0..256u32).map(|i| rom.u8(TITLE_SHADE + i)).collect();
    out.put_json(
        "ui/menu/title_shade.json",
        &serde_json::json!({"rom": format!("0x{TITLE_SHADE:08X}"), "code": "0x0803F9D4", "rect": [80, 104, 160, 144],
                            "note": "pixel = table[pixel] over the rect (title / Single Player menus)", "table": t}),
    );
}
