//! The world map (area 0-50) in mode 1: affine ground (BG2) per scanline, the sky panorama (BG1), the haze /
//! mini-map BG0 (a normal map layer), billboard props, the flyer, its halo and shadow, the mini-map marker and the
//! bottom label — reimplemented from the game's integer code (re/buu/notes.md "Field: world map", docs/maps.md):
//!
//! * camera (map object `0x030019A4`): +0 x, +4 y (22.10 of world px), +8 height (22.10), +0xC heading, +0x10 pitch;
//! * `0x0802ED98` (every frame, before the objects): the 2x2 heading rotation (+0x44), the 3x4 view matrix (+0x5C,
//!   quaternions heading x 90° x pitch in 1 << 15, converted to a 1024-scale matrix), the far distance (+0x14) and
//!   the ground bounding box (+0x18..+0x24) used to cull billboards;
//! * `0x080086D8`: the per-scanline BG2 table (lines 56..159; the HBlank IRQ `0x080080BE` writes line v + 1's
//!   PA / PC / X / Y at the HBlank of line v);
//! * `0x0802F1C6`: point projection (sx = 120 + 120 x / z, sy = 80 - 80 y / z, BIOS Div);
//! * billboards `0x0800CBA8` / `0x0800CC6C`, the flyer `0x08010098..` / draw `0x0800EF94`, marker `0x08027A74`,
//!   label `0x0800C144`.
//!
//! The arithmetic is 32-bit wrapping where the game uses `muls`, 64-bit where it uses `smull`/`smlal`.

use wf_content::mode7::Mode7Data;
use wf_view::Mode7View;

pub const HORIZON_LINE: usize = 56;

/// `(t[i] * (1024 - f) + t[i + 1] * f) >> 16` of a 22.10 angle, wrapping like `muls`.
#[inline]
pub fn interp(t: &[i32], a: i32) -> i32 {
    let i = a >> 10;
    let f = a & 1023;
    let t0 = t[(i & 255) as usize];
    let t1 = t[((i + 1) & 255) as usize];
    t0.wrapping_mul(1024 - f).wrapping_add(t1.wrapping_mul(f)) >> 16
}

/// cos / sin of a 22.10 angle at quaternion precision (1 << 15), the half angle: `(a << 5) / 2`.
fn half15(sin: &[i32], a: i32) -> (i32, i32) {
    let a15 = a.wrapping_shl(5) / 2;
    let i = a15 >> 15;
    let f = a15 & 0x7FFF;
    let s = |k: i32| sin[(k & 255) as usize];
    let c = (0x8000 - f).wrapping_mul(s(i + 64)).wrapping_add(s(i + 65).wrapping_mul(f)) >> 16;
    let sn = (0x8000 - f).wrapping_mul(s(i)).wrapping_add(f.wrapping_mul(s(i + 1))) >> 16;
    (c, sn)
}

/// Quaternion product (`0x08052F08`), 1 << 15 scale, rounded.
fn qmul(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let m = |x: i32, y: i32| x.wrapping_mul(y);
    let w = m(a[0], b[0]).wrapping_sub(m(a[1], b[1])).wrapping_sub(m(a[2], b[2])).wrapping_sub(m(a[3], b[3])).wrapping_add(0x4000) >> 15;
    let x = m(a[0], b[1]).wrapping_add(m(a[1], b[0])).wrapping_add(m(a[2], b[3])).wrapping_sub(m(a[3], b[2])).wrapping_add(0x4000) >> 15;
    let y = m(a[0], b[2]).wrapping_add(m(a[2], b[0])).wrapping_add(m(a[3], b[1])).wrapping_sub(m(a[1], b[3])).wrapping_add(0x4000) >> 15;
    let z = m(a[0], b[3]).wrapping_add(m(a[3], b[0])).wrapping_add(m(a[1], b[2])).wrapping_sub(m(a[2], b[1])).wrapping_add(0x4000) >> 15;
    [w, x, y, z]
}

/// The camera state the game derives every frame (`0x0802ED98`).
#[derive(Clone, Debug, PartialEq)]
pub struct Camera3d {
    pub x: i32,
    pub y: i32,
    pub height: i32,
    pub heading: i32,
    pub pitch: i32,
    /// +0x44..+0x50: [cos, -sin, sin, cos] of the heading, 1024 scale
    pub rot: [i32; 4],
    /// +0x5C: rows (x, y, z inputs) x 3 outputs, then the translation
    pub mat: [i32; 12],
    /// +0x14
    pub far: i32,
    /// +0x18..+0x24 (px, x1 / y1 exclusive)
    pub bbox: [i32; 4],
}

impl Camera3d {
    pub fn new(d: &Mode7Data, v: &Mode7View) -> Camera3d {
        let (x, y, height, heading, pitch) = (v.x, v.y, v.height, v.heading, v.pitch);
        let sin = &d.sin;
        let c = interp(sin, heading.wrapping_add(64 << 10));
        let s = interp(sin, heading);
        let rot = [c, -s, s, c];
        // quaternions: heading (about z), +90° about x, pitch (about x)
        let (ch, sh) = half15(sin, heading);
        let (cp, sp) = half15(sin, pitch);
        let q = qmul(qmul([ch, 0, 0, sh], [23170, 23170, 0, 0]), [cp, sp, 0, 0]);
        let (w, qx, qy, qz) = (q[0], q[1], q[2], q[3]);
        let p = |a: i32, b: i32| a.wrapping_mul(b.wrapping_shl(1)) >> 15;
        let (wx2, wy2, wz2) = (p(w, qx), p(w, qy), p(w, qz));
        let (xx2, xy2, xz2) = (p(qx, qx), p(qx, qy), p(qx, qz));
        let (yy2, yz2, zz2) = (p(qy, qy), p(qy, qz), p(qz, qz));
        let r = [
            (0x8000 - (yy2 + zz2)) >> 5,
            (xy2 - wz2) >> 5,
            (xz2 + wy2) >> 5,
            (xy2 + wz2) >> 5,
            (0x8000 - (zz2 + xx2)) >> 5,
            (yz2 - wx2) >> 5,
            (xz2 - wy2) >> 5,
            (yz2 + wx2) >> 5,
            (0x8000 - (xx2 + yy2)) >> 5,
        ];
        // identity (1024) with translation (-x, -y, -height), times R (0x03000560)
        let mut mat = [0i32; 12];
        mat[..9].copy_from_slice(&r);
        let t = [-x, -y, -height];
        for j in 0..3 {
            let acc = t[0] as i64 * r[j] as i64 + t[1] as i64 * r[3 + j] as i64 + t[2] as i64 * r[6 + j] as i64;
            mat[9 + j] = (acc >> 10) as i32;
        }
        mat[0] = mat[0].wrapping_shl(1);
        mat[3] = mat[3].wrapping_shl(1);
        mat[9] = mat[9].wrapping_shl(1);
        mat[6] = 0;
        mat[7] = 1024;
        mat[8] = 0;
        // far distance: the csc of (pitch - the line-56 angle) x height
        let far = height.wrapping_mul(interp(&d.csc, pitch.wrapping_add(-12160))) >> 10;
        // ground bounding box around the near point
        let a2 = pitch.wrapping_add(0x7EF9);
        let csc2 = interp(&d.csc, a2);
        let cos2 = interp(sin, a2.wrapping_add(64 << 10));
        let near = (csc2.wrapping_mul(height.wrapping_mul(cos2) >> 10) >> 10) - 1024;
        let cx = (x - (rot[1].wrapping_mul(near) >> 10)) >> 10;
        let cy = (y - (rot[3].wrapping_mul(near) >> 10)) >> 10;
        let mut bb = [cx, cy, cx, cy];
        let mut acc = |px: i32, py: i32| {
            if bb[0] > px {
                bb[0] = px;
            }
            if bb[1] > py {
                bb[1] = py;
            }
            if bb[2] < px {
                bb[2] = px;
            }
            if bb[3] < py {
                bb[3] = py;
            }
        };
        acc(cx - (rot[1].wrapping_mul(224) >> 10), cy - (rot[3].wrapping_mul(224) >> 10));
        acc(cx - (rot[0].wrapping_mul(112) >> 10), cy - (rot[2].wrapping_mul(112) >> 10));
        acc(cx + (rot[0].wrapping_mul(112) >> 10), cy + (rot[2].wrapping_mul(112) >> 10));
        let k = heading >> 10;
        let s8 = |i: i32| sin[(i & 255) as usize];
        acc(cx + (s8(k + 236).wrapping_mul(224) >> 16), cy - (s8(k + 236 + 64).wrapping_mul(224) >> 16));
        acc(cx + (s8(k + 20).wrapping_mul(224) >> 16), cy - (s8(k + 20 + 64).wrapping_mul(224) >> 16));
        bb[2] += 1;
        bb[3] += 1;
        Camera3d { x, y, height, heading, pitch, rot, mat, far, bbox: bb }
    }

    /// `0x08049180`: camera-space point (x, y, z inputs in 22.10).
    pub fn transform(&self, p: [i32; 3]) -> [i32; 3] {
        let m = &self.mat;
        let mut out = [0; 3];
        for (j, o) in out.iter_mut().enumerate() {
            let acc = p[0] as i64 * m[j] as i64 + p[1] as i64 * m[3 + j] as i64 + p[2] as i64 * m[6 + j] as i64;
            *o = m[9 + j].wrapping_add((acc >> 10) as i32);
        }
        out
    }

    /// `0x0802F1C6`: (screen x, screen y, depth) when 0 < depth <= far.
    pub fn project(&self, p: [i32; 3]) -> Option<(i32, i32, i32)> {
        let c = self.transform(p);
        if c[2] <= 0 || c[2] > self.far {
            return None;
        }
        let sx = c[0].wrapping_mul(120).checked_div(c[2]).unwrap_or(0) + 120;
        let sy = c[1].wrapping_mul(-80).checked_div(c[2]).unwrap_or(0) + 80;
        Some((sx, sy, c[2]))
    }

    /// `0x080086D8`: BG2 parameters of a scanline (56..159): (pa, pc, x, y) — x / y in 1/256 world px.
    pub fn scanline(&self, d: &Mode7Data, line: usize) -> (i32, i32, i32, i32) {
        let a = line_angle(d, line).wrapping_add(self.pitch);
        let csc = interp(&d.csc, a);
        let dist = self.height.wrapping_mul(csc) >> 10;
        let cos_a = interp(&d.sin, a.wrapping_add(64 << 10));
        let dd = cos_a.wrapping_mul(dist) >> 10;
        let e = line_cos(d, line).wrapping_mul(dist) >> 10;
        let r7 = (self.x - (self.rot[1].wrapping_mul(dd) >> 10)) >> 2;
        let lr = (self.y - (self.rot[3].wrapping_mul(dd) >> 10)) >> 2;
        let pa = self.rot[0].wrapping_mul(e).wrapping_mul(10) >> 23;
        let pc = self.rot[2].wrapping_mul(e).wrapping_mul(10) >> 23;
        (pa, pc, r7.wrapping_sub(pa.wrapping_mul(120)), lr.wrapping_sub(pc.wrapping_mul(120)))
    }
}

/// per-line pitch offset; past the GBA screen (zoomed-out views) the angle keeps its last slope
fn line_angle(d: &Mode7Data, line: usize) -> i32 {
    let n = d.line_angle.len();
    if line < n {
        return d.line_angle[line];
    }
    let (a, b) = (d.line_angle[n - 2], d.line_angle[n - 1]);
    b + (b - a) * (line - n + 1) as i32
}
fn line_cos(d: &Mode7Data, line: usize) -> i32 {
    if line < d.line_cos.len() {
        return d.line_cos[line];
    }
    interp(&d.sin, line_angle(d, line).wrapping_add(64 << 10)) >> 6
}

/// Flyer position (x, y in 24.8, z in 22.10) from the camera it produced (`0x08034124` inverted): the eye is
/// 30 px behind the flyer at the camera pitch.
pub fn flyer_from_camera(d: &Mode7Data, v: &Mode7View) -> (i32, i32, i32) {
    let s64 = |a: i32| interp(&d.sin, a) as i64;
    let cs = s64(v.pitch.wrapping_add(64 << 10));
    let x4 = (v.x as i64).wrapping_add((s64(v.heading) * cs * 30) >> 10);
    let y4 = (v.y as i64).wrapping_sub((s64(v.heading.wrapping_add(64 << 10)) * cs * 30) >> 10);
    let z = (v.height as i64 - s64(v.pitch) * 30) as i32;
    ((x4 >> 2) as i32, (y4 >> 2) as i32, z)
}

/// GBA division (BIOS Div, truncating).
#[inline]
pub fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

/// A billboard prop's OAM: (x, y, scale, depth) or None when culled (`0x0800CBA8`).
pub fn billboard(cam: &Camera3d, p: &wf_content::mode7::Prop) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0, x1, y1) = (p.x, p.y, p.x + p.w, p.y + p.h);
    let w = x1 - x0;
    let hgt = y1 - y0;
    let cxp = (x0 + x1) >> 1;
    let mut cyp = y1;
    let mut yoff = hgt << 7;
    let z = match p.kind {
        0x28 => 0x37 << 11,
        0x3C => {
            if w > 16 {
                cyp = (y1 * 3 + y0) >> 2;
                yoff = yoff / 2;
            }
            0
        }
        _ => 15 << 12,
    };
    let b = &cam.bbox;
    if cxp < b[0] || cxp >= b[2] || cyp < b[1] || cyp >= b[3] {
        return None;
    }
    let (sx, sy, depth) = cam.project([cxp << 10, cyp << 10, z])?;
    let s = (depth.wrapping_mul(10) >> 12).max(0x80);
    let ox = sx - div(0, s) - w;
    if ox >= 240 || ox < -2 * w {
        return None;
    }
    let oy = sy - div(yoff, s) - hgt;
    if oy >= 160 || oy < -2 * hgt {
        return None;
    }
    Some((ox, oy, s, depth))
}
