//! In-game HUD (draw routine `0x0800B99C..0x0800BCC0`, called every frame).
//!
//! The HUD is an 80×16 8bpp picture uploaded to OBJ tiles 880.. and shown as three OBJs at the HUD position
//! (8, 8): 32×16 (tiles +0), 16×16 at +32 (tiles +16), 32×16 at +48 (tiles +24), priority 1. It is rebuilt
//! each frame in a 0x500-byte buffer in OBJ tile order:
//! 1. `0x000..0x200` ← the selected skill's 32×16 icon (`icons/skills/skill_NN[_alt].png`; `_alt` = not enough EP);
//! 2. `0x200..0x500` ← the frame `0x087137D8` (raw, 0x300 bytes) — it already shows full HP and EP bars;
//! 3. HP: for step `i` in `hp41 .. 41` the 3 pixels of step `i` (table `0x08071CB0`, 16-byte records of 3 buffer
//!    offsets) are cleared to colour 1;
//! 4. EP / Super Saiyan gauges (table `0x08071F40`, same format): steps `< ss3_41` take their pixels from the
//!    full-HUD image `0x08713FD8`, steps `< ss_41` from `0x08713AD8`, steps `>= max(ep41, …)` are cleared to 1;
//!    between them the frame's own (EP) pixels stay;
//! 5. the 72-step line (table `0x080721D0`, 8-byte records of 2 offsets): steps `>= v72` cleared to 1, where
//!    `v72 = 0x0801363E(player, 72)` (experience progress);
//! 6. the Super Saiyan cooldown: the first `n` pixels of the icon's corner triangle (table `0x08072410`, buffer
//!    offsets) are cleared to colour 1, `n = 0x08071CA8[cooldown >> 15]` (record +0x04; 0, 7, 13, .. 28 - the
//!    triangle empties after a transformation and refills as the cooldown runs out).
//!
//! `hp41 = HP·41 / maxHP`, `ep41 = EP·41 / maxEP`, `ss_41 = SS gauge·41 / maxEP`, `ss3_41 = SS3 gauge·41 / maxEP`
//! (character record +0x16/+0x18/+0x1A/+0x1C/+0x1E/+0x20); then `ss_41 = max(ss_41, ss3_41)`,
//! `ep41 = max(ep41, ss_41)`. HP at or below 1/5 plays sound `0x08319488` every 60 frames.

use crate::surface::Surface;
use serde_json::{json, Value};
use wf_rom::{hx, Rom};

pub const FRAME: u32 = 0x0871_37D8;
pub const IMG_SS: u32 = 0x0871_3AD8;
pub const IMG_SS3: u32 = 0x0871_3FD8;
pub const HP_STEPS: u32 = 0x0807_1CB0;
pub const EP_STEPS: u32 = 0x0807_1F40;
pub const XP_STEPS: u32 = 0x0807_21D0;
pub const CD_COUNT: u32 = 0x0807_1CA8;
pub const CD_PIXELS: u32 = 0x0807_2410;

#[derive(Clone, Copy, Debug, Default)]
pub struct Gauges {
    pub hp41: u32,
    pub ep41: u32,
    pub ss41: u32,
    pub ss3_41: u32,
    pub xp72: u32,
    /// the SS cooldown's step (record +0x04 >> 15)
    pub cd: u32,
}

impl Gauges {
    /// From character-record values, as `0x0800BA6E..0x0800BB22` computes them.
    pub fn from_stats(hp: i32, hp_max: i32, ep: i32, ep_max: i32, ss: i32, ss3: i32, xp72: u32) -> Gauges {
        let d = |a: i32, b: i32| if b == 0 { 0 } else { (a * 41 / b).max(0) as u32 };
        let hp41 = d(hp, hp_max);
        let mut ep41 = d(ep, ep_max);
        let mut ss41 = d(ss, ep_max);
        let ss3_41 = d(ss3, ep_max);
        if ss41 < ss3_41 {
            ss41 = ss3_41;
        }
        if ep41 < ss41 {
            ep41 = ss41;
        }
        Gauges { hp41, ep41, ss41, ss3_41, xp72, cd: 0 }
    }
}

pub struct Hud {
    pub frame: Vec<u8>,
    pub img_ss: Vec<u8>,
    pub img_ss3: Vec<u8>,
    pub hp: Vec<[u32; 3]>,
    pub ep: Vec<[u32; 3]>,
    pub xp: Vec<[u32; 2]>,
    /// pixels cleared per cooldown step (`CD_COUNT`, 8 entries) and the pixels in clearing order (`CD_PIXELS`)
    pub cd_count: Vec<u32>,
    pub cd: Vec<u32>,
}

impl Hud {
    pub fn load(rom: &Rom) -> Hud {
        let rd = |a: u32, n: usize| rom.bytes(a, n).to_vec();
        Hud {
            frame: rd(FRAME, 0x300),
            img_ss: rd(IMG_SS, 0x500),
            img_ss3: rd(IMG_SS3, 0x500),
            hp: (0..41).map(|i| [0, 4, 8].map(|k| rom.u32(HP_STEPS + 16 * i + k))).collect(),
            ep: (0..41).map(|i| [0, 4, 8].map(|k| rom.u32(EP_STEPS + 16 * i + k))).collect(),
            xp: (0..72).map(|i| [0, 4].map(|k| rom.u32(XP_STEPS + 8 * i + k))).collect(),
            cd_count: (0..8).map(|i| rom.bytes(CD_COUNT + i, 1)[0] as i8 as i32).map(|n| n.max(0) as u32).collect(),
            cd: (0..28).map(|i| rom.u32(CD_PIXELS + 4 * i)).collect(),
        }
    }

    /// The 0x500-byte OBJ tile buffer, exactly as the game builds it.
    pub fn compose(&self, icon: &[u8], g: Gauges) -> Vec<u8> {
        let mut b = vec![0u8; 0x500];
        b[..0x200].copy_from_slice(&icon[..0x200]);
        b[0x200..].copy_from_slice(&self.frame);
        for i in g.hp41.min(41)..41 {
            for &o in &self.hp[i as usize] {
                b[o as usize] = 1;
            }
        }
        let mut i = 0u32;
        while i < g.ss3_41.min(41) {
            for &o in &self.ep[i as usize] {
                b[o as usize] = self.img_ss3[o as usize];
            }
            i += 1;
        }
        while i < g.ss41.min(41) {
            for &o in &self.ep[i as usize] {
                b[o as usize] = self.img_ss[o as usize];
            }
            i += 1;
        }
        let mut i = i.max(g.ep41);
        while i < 41 {
            for &o in &self.ep[i as usize] {
                b[o as usize] = 1;
            }
            i += 1;
        }
        for i in g.xp72.min(72)..72 {
            for &o in &self.xp[i as usize] {
                b[o as usize] = 1;
            }
        }
        let n = self.cd_count.get(g.cd as usize).copied().unwrap_or(0) as usize;
        for &o in self.cd.iter().take(n) {
            b[o as usize] = 1;
        }
        b
    }
}

/// Buffer offset → pixel in the 80×16 HUD picture.
pub fn off_xy(o: u32) -> (u32, u32) {
    let (base, x0, tw) = if o < 0x200 { (0, 0, 4) } else if o < 0x300 { (0x200, 32, 2) } else { (0x300, 48, 4) };
    let r = o - base;
    let t = r / 64;
    let p = r % 64;
    (x0 + (t % tw) * 8 + p % 8, (t / tw) * 8 + p / 8)
}

/// 0x500-byte buffer → 80×16 linear picture.
pub fn to_picture(buf: &[u8]) -> Surface {
    let mut s = Surface::new(80, 16);
    for (o, &c) in buf.iter().enumerate() {
        let (x, y) = off_xy(o as u32);
        s.set(x as i32, y as i32, c);
    }
    s
}

pub fn json(h: &Hud) -> Value {
    let pts3 = |v: &Vec<[u32; 3]>| v.iter().map(|s| s.iter().map(|&o| json!(off_xy(o))).collect::<Vec<_>>()).collect::<Vec<_>>();
    json!({
        "size": [80, 16],
        "position": [8, 8],
        "objs": [
            {"x": 0, "w": 32, "h": 16, "tile_offset": 0, "content": "selected skill icon (icons/skills/skill_NN.png; _alt when EP is short)"},
            {"x": 32, "w": 16, "h": 16, "tile_offset": 16, "content": "frame left (HP / EP bar start)"},
            {"x": 48, "w": 32, "h": 16, "tile_offset": 24, "content": "frame right"},
        ],
        "obj_tiles": "8bpp OBJ tiles 880.. (1D), priority 1",
        "images": {
            "frame": "hud/frame.png (80x16, skill icon area empty; shows full HP and EP bars)",
            "ss": "hud/gauge_ss.png (full-HUD picture whose bar pixels are the Super Saiyan gauge colours)",
            "ss3": "hud/gauge_ss3.png (same for the SS3 gauge)",
        },
        "empty_colour": 1,
        "steps": {
            "hp": {"count": 41, "pixels": pts3(&h.hp), "rule": "steps >= HP*41/maxHP are set to colour 1", "rom": hx(HP_STEPS)},
            "ep": {"count": 41, "pixels": pts3(&h.ep),
                   "rule": "steps < ss3: pixel from gauge_ss3; steps < ss: from gauge_ss; steps >= max(ep, ss): colour 1 (ep = EP*41/maxEP, ss = SSgauge*41/maxEP, ss3 = SS3gauge*41/maxEP; ss = max(ss, ss3); ep = max(ep, ss))",
                   "rom": hx(EP_STEPS)},
            "xp": {"count": 72, "pixels": h.xp.iter().map(|s| s.iter().map(|&o| json!(off_xy(o))).collect::<Vec<_>>()).collect::<Vec<_>>(),
                   "rule": "steps >= 0x0801363E(player, 72) set to colour 1", "rom": hx(XP_STEPS)},
            "cooldown": {"counts": h.cd_count, "pixels": h.cd.iter().map(|&o| json!(off_xy(o))).collect::<Vec<_>>(),
                   "rule": "the first counts[SS cooldown >> 15] pixels set to colour 1 (the icon's corner triangle)",
                   "rom": {"counts": hx(CD_COUNT), "pixels": hx(CD_PIXELS)}},
        },
        "low_hp_sound": {"condition": "HP*5 <= maxHP", "every_frames": 60, "sound": "0x08319488"},
        "rom": {"frame": hx(FRAME), "gauge_ss": hx(IMG_SS), "gauge_ss3": hx(IMG_SS3), "draw": "0x0800B99C"},
    })
}
