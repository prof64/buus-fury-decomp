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
        Flight {
            z: 60 << 10,
            raw: 16,
            ..Default::default()
        }
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
    // an event over the running map (a circle's question s480, a circle's inline script: Earth 3-1 / 4-1) holds the
    // flyer, its animation ticks; a warp's fade-in and the frame after it do not (s479 / s515: the event ends then)
    let frame = w.frame;
    // (also through an event's `warp_fade` away from the map: s026's fade-out, step 4)
    let script_fade =
        |m: &crate::field::Mode| matches!(m, crate::field::Mode::FadeOut { step: 4, .. });
    if w.field.as_ref().is_some_and(|f| {
        f.busy
            && (matches!(f.mode, crate::field::Mode::Normal) && f.fade_end + 1 != frame
                || script_fade(&f.mode))
    }) {
        // (its animation keeps ticking)
        let f = w.field.as_mut().unwrap().flight.as_mut().unwrap();
        f.tick = (f.tick + 1) & 0xFFFF;
        f.frame = (f.tick >> 3) & 3;
        let frame = f.frame;
        w.pl(i).ap.frame = frame;
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

/// the Dragon Radar's update (object vtable 0x0804B400, `0x0802BA18`; before the camera's): each target (in order -
/// a later one overwrites) seen from the camera position (>> 10) through the heading's rotation (camera +0x44, made
/// before the objects from the heading of the pass before) sets the arrow of its larger axis to its distance, the
/// arrows show < 100 / < 400 / further, the centre the nearest target's squared distance < 625 / < 62,500 / further
pub fn radar_update(w: &mut World) {
    let Some(cam) = w.field.as_ref().and_then(|f| f.flight.as_ref()).map(|f| f.cam) else { return };
    let Some(r) = w.scene.radar.as_mut() else { return };
    let (x, y) = (cam[0] >> 10, cam[1] >> 10);
    let (c, s) = (c_i(cam[3]) as i32, s_i(cam[3]) as i32);
    // (behind, ahead, left, right)
    let mut dist = [i32::MAX; 4];
    let mut near = i32::MAX;
    for &(tx, ty) in &r.targets {
        let (dx, dy) = (tx.wrapping_sub(x), ty.wrapping_sub(y));
        let d2 = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
        if d2 < near {
            near = d2;
        }
        let a = c.wrapping_mul(dx).wrapping_add(s.wrapping_mul(dy)) >> 10;
        let b = s.wrapping_neg().wrapping_mul(dx).wrapping_add(c.wrapping_mul(dy)) >> 10;
        if a.wrapping_abs() > b.wrapping_abs() {
            match a.signum() {
                1 => dist[3] = a,
                -1 => dist[2] = a.wrapping_neg(),
                _ => (dist[2], dist[3]) = (0, 0),
            }
        } else {
            match b.signum() {
                1 => dist[0] = b,
                -1 => dist[1] = b.wrapping_neg(),
                _ => (dist[0], dist[1]) = (0, 0),
            }
        }
    }
    for k in 0..4 {
        r.dir[k] = if dist[k] < 100 { 0 } else if dist[k] < 400 { 1 } else { 2 };
    }
    r.centre = if near < 625 { 0 } else if near < 62_500 { 1 } else { 2 };
}

/// the 3D camera (`0x08034124`, after the objects, no easing): pitch when z changed, the eye 30 px behind the flyer;
/// the 2D camera rect follows the flyer (culling, observation)
pub fn camera(w: &mut World) {
    let Some(i) = w.player_idx(0) else { return };
    let (x, y) = (w.pl_ref(i).a.x, w.pl_ref(i).a.y);
    let Some(f) = w.field.as_mut().and_then(|f| f.flight.as_mut()) else {
        return;
    };
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
    let cam = f.cam;
    face_objects(w, cam);
    let c = &mut w.cameras[0];
    c.shown = c.rect;
    c.shown_fine = (c.x, c.y);
    let (cx, cy) = ((x >> 8) - 112, (y >> 8) - 76);
    c.x = cx << 10;
    c.y = cy << 10;
    c.init = true;
    c.rect = crate::geom::Rect::new(cx, cy, cx + crate::world::GBA_W, cy + crate::world::GBA_H);
}

/// LoadArea step 2 (`0x08004A88`): the first map-screen entry of the area sets story flag 0x1C0 + bit
pub fn mark_visited(w: &mut World, world: i32, area: i32) {
    let Some(fd) = w.data.field.as_ref() else {
        return;
    };
    if let Some(&(_, _, bit)) = fd.visited.iter().find(|v| v.0 == world && v.1 == area) {
        w.story.set_flag(0x1C0 + bit, true);
    }
}

/// the 3D camera's ground rect (`0x0802ED98`, camera +0x18..+0x24, px): the bounding box of the view's footprint -
/// the point under the eye at the pitch's ground distance, 224 px ahead, +-112 px across and the +-20 heading rays
/// (VERIFIED against RAM, 3-1)
pub fn view_rect(cam: [i32; 5]) -> [i32; 4] {
    use crate::tables::CSC256;
    let [cx, cy, hgt, h, p] = cam;
    let a = p.wrapping_add(0x7ef9);
    let (i, fr) = (((a >> 10) & 255) as usize, a & 1023);
    let t2 = CSC256[i]
        .wrapping_mul(1024 - fr)
        .wrapping_add(CSC256[(i + 1) & 255].wrapping_mul(fr));
    let cv = SIN256[(i + 0x40) & 255]
        .wrapping_mul(1024 - fr)
        .wrapping_add(SIN256[(i + 0x41) & 255].wrapping_mul(fr))
        >> 16;
    let r0 = hgt.wrapping_mul(cv) >> 10;
    let d = (r0.wrapping_mul(t2 >> 16) >> 10) - 1024;
    let s = s_i(h) as i32;
    let c = c_i(h) as i32;
    let (m00, m01, m10, m11) = (c, -s, s, c);
    let x0 = cx.wrapping_sub(m01.wrapping_mul(d) >> 10) >> 10;
    let y0 = cy.wrapping_sub(m11.wrapping_mul(d) >> 10) >> 10;
    let mut pts = vec![
        (x0, y0),
        (x0 - ((m01 * 224) >> 10), y0 - ((m11 * 224) >> 10)),
        (x0 - ((m00 * 112) >> 10), y0 - ((m10 * 112) >> 10)),
        (x0 + ((m00 * 112) >> 10), y0 + ((m10 * 112) >> 10)),
    ];
    let hh = h >> 10;
    for dd in [-20, 20] {
        let q = ((hh + dd) & 255) as usize;
        pts.push((
            x0 + ((SIN256[q] * 224) >> 16),
            y0 - ((SIN256[(q + 64) & 255] * 224) >> 16),
        ));
    }
    let r = pts
        .iter()
        .fold([i32::MAX, i32::MAX, i32::MIN, i32::MIN], |r, &(x, y)| {
            [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]
        });
    [r[0], r[1], r[2] + 1, r[3] + 1]
}

/// the 3D map's object draw (`0x0801003A`): an enemy inside the view rect gets its facing from its heading relative
/// to the camera (`0x08010010`: table 0x08072718 by ((heading >> 10) - (camera heading >> 10)) & 0xFF >> 5); outside
/// it keeps the last one (3-1: an airship turns when it comes into view)
fn face_objects(w: &mut World, cam: [i32; 5]) {
    const FACE: [u8; 8] = [1, 3, 3, 0, 0, 2, 2, 1];
    let r = view_rect(cam);
    for i in 0..w.ents.len() {
        if w.ents[i].removed || !matches!(w.ents[i].body, crate::world::Body::Enemy(_)) {
            continue;
        }
        let (px, py) = (w.en_ref(i).a.x >> 8, w.en_ref(i).a.y >> 8);
        if px < r[0] || px >= r[2] || py < r[1] || py >= r[3] {
            continue;
        }
        let oh = crate::fieldpatrol::heading(w, i);
        let k = ((((oh >> 10) - (cam[3] >> 10)) & 0xff) >> 5) as usize;
        w.en(i).a.facing = FACE[k];
    }
}
