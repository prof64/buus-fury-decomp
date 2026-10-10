//! Options / Journal / Skills pages.
//!
//! * `ui/menu/skill_icon_NNN.png` (24x11): what `0x08029138(dst, skill)` builds for the Skills page and its popup —
//!   the frame picture record `0x0874F058` (raw, 24x11) with the 16x8 window (3, 3)..(19, 11) of the skill's 32x16
//!   8bpp tiled icon (skill table `0x0871DAF8` +4) copied in at (4, 2), zeros included (the page blit then skips
//!   them, so the icon's transparent pixels show the page behind).
//! * `ui/menu/opts.json`: the skill table (24 × 24 bytes: +0 level byte, 0xFF = no levels; +0xC name string id,
//!   +0xE description string id) and the quest table `0x08720564` (40 × 16 bytes: u8 star (0 gold, 1 silver,
//!   2 bronze), u16 string id, then the hidden / completed / goal condition scripts, inline ids).

use super::MenuPic;
use crate::surface::Surface;
use serde_json::json;
use wf_rom::content::Content;
use wf_rom::gfx::Rgb;
use wf_rom::Rom;

pub const PICS: &[MenuPic] = &[];

pub const SKILLS: u32 = 0x0871_DAF8;
pub const SKILL_FRAME: u32 = 0x0874_F058;
pub const QUESTS: u32 = 0x0872_0564;

pub fn skill_icon(rom: &Rom, skill: u32) -> Vec<u8> {
    let mut d = rom.lz(SKILL_FRAME).unwrap_or_default();
    d.resize(24 * 11, 0);
    let tiles = rom.u32(SKILLS + 24 * skill + 4);
    for r in 0..8u32 {
        for c in 0..16u32 {
            let (cc, rr) = (c + 3, r + 3);
            let off = (cc & 7) + (cc >> 3) * 64 + (rr & 7) * 8 + if rr >= 8 { 256 } else { 0 };
            d[(0x34 + r * 24 + c) as usize] = rom.u8(tiles + off);
        }
    }
    d
}

pub fn extract(rom: &Rom, out: &mut Content, pal: &[Rgb]) {
    let mut skills = Vec::new();
    for i in 0..64u32 {
        let a = SKILLS + 24 * i;
        if !rom.isptr(rom.u32(a + 4)) {
            break;
        }
        let png = format!("ui/menu/skill_icon_{i:03}.png");
        out.put(png.clone(), Surface::from_vec(24, 11, skill_icon(rom, i)).png(pal, true));
        skills.push(json!({"skill": i, "level_byte": rom.u8(a), "name": rom.u16(a + 0xC), "desc": rom.u16(a + 0xE), "icon": png}));
    }
    let mut quests = Vec::new();
    for i in 0..40u32 {
        let a = QUESTS + 16 * i;
        let id = |p: u32| format!("inline_0x{:08X}", rom.u32(p));
        quests.push(json!({"star": rom.u8(a), "text": rom.u16(a + 2), "hidden": id(a + 4), "done": id(a + 8), "goal": id(a + 12)}));
    }
    out.put_json("ui/menu/opts.json", &json!({"skills": skills, "quests": quests,
        "rom": {"skills": "0x0871DAF8", "quests": "0x08720564", "skill_frame": "0x0874F058"}}));
}
