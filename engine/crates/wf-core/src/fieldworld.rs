//! The world map (area 0-50, the only map with the "3D" header flag +0x3C): the flying player (ctor `0x080104F2`, vt
//! `0x0804E1B4`, flight state vt `0x0804D038`), its 3D camera (`0x08034124`, map object `0x030019A4`), the 64-frame
//! arrival fade with control, altitude triggers and the bottom label (message triggers 0x17). Landing / take-off
//! are scripts (capsule event s515 → `warp_fade(0, 50, ..)`; landing spots' altitude triggers → fade + warp).
//! Spec: re/buu/notes.md "Field: world map", tools/field/wm_model.py; ground truth data/field/worldmap_*.json.

use crate::tables::SIN256;
use crate::world::World;
use serde::{Deserialize, Serialize};

/// the world map size (map object +0x94 / +0x98): positions wrap
pub const WM_W: i32 = 3584;
pub const WM_H: i32 = 2560;
/// arrival fade-in length on the 3D map (brightness step 4), control running
pub const FADE_IN_3D: i32 = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Flight {
    /// altitude (obj+0x1A8, 22.10): arrival 60 px, A descends to 5, B climbs to 120
    pub z: i32,
    /// heading (22.10, 256 << 10 = a full turn, 0 = north, + = clockwise)
    pub h: i32,
    /// flight state +4 reversing, +8 speed, +0xC turn rate
    pub rev: bool,
    pub spd: i32,
    pub turn: i32,
    pub tick: i32,
    pub frame: i32,
    /// shown raw anim (16 straight, 17 banking)
    pub raw: i32,
    /// the 3D camera: x, y (22.10 of world px << 2 ...), height, heading, pitch; `zc` = z of the last pitch update
    pub cam: [i32; 5],
    pub zc: Option<i32>,
}

impl Flight {
    pub fn arrive() -> Flight {
        Flight { z: 60 << 10, raw: 16, ..Default::default() }
    }
}

fn tdiv(a: i64, b: i64) -> i64 {
    a / b
}

/// the interpolated sine (1024 = 1.0) of a 22.10 angle
pub fn s_i(a: i32) -> i64 {
    let i = ((a >> 10) & 255) as usize;
    let f = (a & 1023) as i64;
    (SIN256[i] as i64 * (1024 - f) + SIN256[(i + 1) & 255] as i64 * f) >> 16
}
pub fn c_i(a: i32) -> i64 {
    s_i(a.wrapping_add(64 << 10))
}
/// the camera pitch of an altitude
pub fn pitch(z: i32) -> i32 {
    let z = z as i64;
    ((z * 218 + tdiv(z * z, 3511) + 0x10F3333) >> 10) as i32
}

pub fn flying(w: &World) -> bool {
    w.field.as_ref().is_some_and(|f| f.flight.is_some())
}

/// the flyer's update (actor update `0x0800EF08` with the flight state: tick, then control on the held keys); true =
/// the player update is done
pub fn flyer_update(w: &mut World, i: usize) -> bool {
    if !flying(w) {
        return false;
    }
    let slot = w.pl_ref(i).slot as usize;
    if slot != 0 {
        return true;
    }
    let keys = w.input[0].held;
    let (mut x, mut y) = (w.pl_ref(i).a.x, w.pl_ref(i).a.y);
    let f = w.field.as_mut().unwrap().flight.as_mut().unwrap();
    f.tick = (f.tick + 1) & 0xFFFF;
    f.frame = (f.tick >> 3) & 3;
    use crate::input::*;
    if keys & A != 0 {
        if f.z > 5 << 10 {
            f.z -= 1 << 10;
        }
    } else if keys & B != 0 && f.z < 120 << 10 {
        f.z += 1 << 10;
    }
    let mut acc = if keys & RIGHT != 0 {
        0x80
    } else if keys & LEFT != 0 {
        -0x80
    } else {
        0
    };
    if keys & UP != 0 {
        f.spd += 0xC0;
    } else if keys & DOWN != 0 {
        f.spd -= 0xC0;
    }
    f.spd = (f.spd * 15) / 16;
    if f.spd > 0 {
        f.rev = false;
    } else if f.spd < 0 {
        f.rev = true;
    }
    if f.rev {
        acc = -acc;
    }
    let mut t = f.turn + acc;
    t -= t >> 3;
    if t > 0 {
        t -= 1;
    }
    f.turn = t;
    f.h = f.h.wrapping_add(t);
    x += ((SIN256[((f.h >> 10) & 255) as usize] as i64 * f.spd as i64) >> 18) as i32;
    y -= ((SIN256[(((f.h >> 10) + 64) & 255) as usize] as i64 * f.spd as i64) >> 18) as i32;
    if x < 0 {
        x += WM_W << 8;
    } else if (x >> 8) >= WM_W {
        x -= WM_W << 8;
    }
    if y < 0 {
        y += WM_H << 8;
    } else if (y >> 8) >= WM_H {
        y -= WM_H << 8;
    }
    let (raw, facing) = if f.turn > 0x80 {
        (17, if f.rev { 2 } else { 1 })
    } else if f.turn < -0x80 {
        (17, if f.rev { 0 } else { 3 })
    } else {
        (16, if f.rev { 0 } else { 1 })
    };
    f.raw = raw;
    let frame = f.frame;
    let p = w.pl(i);
    p.a.x = x;
    p.a.y = y;
    p.a.facing = facing;
    p.ap.raw = raw;
    p.ap.frame = frame;
    true
}

/// the end of the arrival fade: SetAnim(0) installs a fresh flight state (position, heading, z kept)
pub fn reset_state(w: &mut World) {
    if let Some(f) = w.field.as_mut().and_then(|f| f.flight.as_mut()) {
        f.rev = false;
        f.spd = 0;
        f.turn = 0;
        f.tick = 0;
        f.frame = 0;
    }
    if let Some(i) = w.player_idx(0) {
        let p = w.pl(i);
        p.a.facing = 1;
        p.ap.frame = 0;
    }
}

/// the 3D camera (`0x08034124`, after the objects, no easing): pitch when z changed, the eye 30 px behind the flyer;
/// the 2D camera rect follows the flyer (culling, observation)
pub fn camera(w: &mut World) {
    let Some(i) = w.player_idx(0) else { return };
    let (x, y) = (w.pl_ref(i).a.x, w.pl_ref(i).a.y);
    let Some(f) = w.field.as_mut().and_then(|f| f.flight.as_mut()) else { return };
    let s = if f.zc != Some(f.z) {
        f.zc = Some(f.z);
        let s = pitch(f.z);
        f.cam[4] = s;
        s
    } else {
        f.cam[4]
    };
    let cs = c_i(s);
    f.cam[0] = ((x as i64) << 2).wrapping_sub((s_i(f.h) * cs * 30) >> 10) as i32;
    f.cam[1] = ((y as i64) << 2).wrapping_add((c_i(f.h) * cs * 30) >> 10) as i32;
    f.cam[2] = (f.z as i64 + s_i(s) * 30) as i32;
    f.cam[3] = f.h;
    let c = &mut w.cameras[0];
    c.shown = c.rect;
    let (cx, cy) = ((x >> 8) - 112, (y >> 8) - 76);
    c.x = cx << 10;
    c.y = cy << 10;
    c.init = true;
    c.rect = crate::geom::Rect::new(cx, cy, cx + crate::world::GBA_W, cy + crate::world::GBA_H);
}

/// LoadArea step 2 (`0x08004A88`): the first map-screen entry of the area sets story flag 0x1C0 + bit
pub fn mark_visited(w: &mut World, world: i32, area: i32) {
    let Some(fd) = w.data.field.as_ref() else { return };
    if let Some(&(_, _, bit)) = fd.visited.iter().find(|v| v.0 == world && v.1 == area) {
        w.story.set_flag(0x1C0 + bit, true);
    }
}
