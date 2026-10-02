//! The game's enemy path follower (tools/battle/COLLISION.md §4), shared by ChasePlayer (0x0802A09F), MoveToPoint
//! (0x0802E105) and WanderToRandomPoint (0x0803BD65). Path object: +4 speed, +8 line budget (50), +0x0C/+0x10
//! waypoint (24.8), +0x14 valid, +0x18 re-plan counter, +0x1C/+0x20 goal (24.8).
//!   enter 0x0803BB00: counter = global stagger (then +1 & 7); plan
//!   update 0x0803BB48: counter 0 -> re-plan; counter = (counter + 7) & 7; step
//!   plan 0x0803B6EA: Bresenham line toward the goal (<= 49 px); blocked -> wall-follow from the last free point
//!   wall-follow 0x0803B4FC: two probes slide along the wall (256 steps each) until the original direction is free
//!   step 0x0803B934: face the waypoint, velocity = d * speed / max(|dx|, |dy|), full move / x only / y only
//! Emulator-verified: data/battle/buu_path_traces.json.

use crate::ai::PathRt;
use crate::enemy;
use crate::world::World;

fn ok(w: &mut World, i: usize, x: i32, y: i32) -> bool {
    enemy::fits(w, i, x >> 8, y >> 8)
}

/// 0x0803BB00
pub fn enter(w: &mut World, i: usize, p: &mut PathRt) {
    p.counter = w.g.path_stagger;
    w.g.path_stagger = (w.g.path_stagger + 1) & 7;
    replan(w, i, p);
}
fn replan(w: &mut World, i: usize, p: &mut PathRt) {
    enemy::unmark_self(w, i);
    let r = plan(w, i, p);
    enemy::mark_self(w, i);
    if !r {
        p.valid = 0;
    }
}
/// 0x0803BB48
pub fn update(w: &mut World, i: usize, p: &mut PathRt) {
    if p.counter == 0 {
        replan(w, i, p);
    }
    p.counter = (p.counter + 7) & 7;
    step(w, i, p);
}

/// 0x0803B6EA
pub fn plan(w: &mut World, i: usize, p: &mut PathRt) -> bool {
    let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let adx = (p.gx - ex).abs();
    let ady = (p.gy - ey).abs();
    let (mut err, err_straight, err_diag, major, mut sx, mut sy);
    let (mut gxs, mut gys) = (256, 256);
    if adx >= ady {
        let mn = ady >> 8;
        let mj = adx >> 8;
        major = mj + 1;
        err = 2 * mn - mj;
        err_straight = 2 * mn;
        err_diag = (mn - mj) * 2;
        sx = 256;
        sy = 0;
    } else {
        let mn = adx >> 8;
        let mj = ady >> 8;
        major = mj + 1;
        err = 2 * mn - mj;
        err_straight = 2 * mn;
        err_diag = (mn - mj) * 2;
        sx = 0;
        sy = 256;
    }
    if ex > p.gx {
        gxs = -256;
        sx = -sx;
    }
    if ey > p.gy {
        gys = -256;
        sy = -sy;
    }
    let mut steps = p.budget;
    if major < steps {
        steps = major - 1;
    }
    let (mut px, mut py) = (ex, ey);
    let mut n = steps - 1;
    while n > 0 {
        if err < 0 {
            err += err_straight;
            px += sx;
            py += sy;
            if !ok(w, i, px, py) {
                return wall_follow(w, i, p, px - sx, py - sy, sx, sy);
            }
        } else {
            err += err_diag;
            px += gxs;
            py += gys;
            if ok(w, i, px, py) {
                n -= 1;
                continue;
            }
            px -= gxs;
            py -= gys;
            let ax = if sx == gxs { 0 } else { gxs };
            let ay = if sx == gxs { gys } else { 0 };
            px += sx;
            py += sy;
            if !ok(w, i, px, py) {
                return wall_follow(w, i, p, px - sx, py - sy, sx, sy);
            }
            px += ax;
            py += ay;
            if !ok(w, i, px, py) {
                return wall_follow(w, i, p, px - gxs, py - gys, ax, ay);
            }
        }
        n -= 1;
    }
    p.wx = p.gx;
    p.wy = p.gy;
    p.valid = 1;
    true
}

/// 0x0803B4FC: probes from (x, y) blocked in direction (dx, dy)
#[allow(clippy::too_many_arguments)]
pub fn wall_follow(w: &mut World, i: usize, p: &mut PathRt, x: i32, y: i32, dx: i32, dy: i32) -> bool {
    let probe = |w: &mut World, side: (i32, i32)| -> (i32, i32, bool) {
        let (mut ax, mut ay) = (x, y);
        let mut s = side;
        let mut f = (dx, dy);
        for _ in 0..256 {
            ax += s.0;
            ay += s.1;
            if !ok(w, i, ax, ay) {
                ax -= s.0;
                ay -= s.1;
                let old = s;
                s = (-f.0, -f.1);
                f = old;
                continue;
            }
            if ok(w, i, ax + f.0, ay + f.1) {
                return (ax, ay, true);
            }
        }
        (ax, ay, false)
    };
    let a = probe(w, (-dy, dx));
    let b = probe(w, (dy, -dx));
    let win = if a.2 && !b.2 {
        a
    } else if b.2 && !a.2 {
        b
    } else {
        let d2 = |q: (i32, i32, bool)| {
            let qx = ((p.gx - q.0) >> 8) as i64;
            let qy = ((p.gy - q.1) >> 8) as i64;
            qx * qx + qy * qy
        };
        if d2(a) < d2(b) {
            a
        } else {
            b
        }
    };
    p.wx = win.0;
    p.wy = win.1;
    p.valid = 1;
    a.2 || b.2
}

/// 0x0803B934
pub fn step(w: &mut World, i: usize, p: &mut PathRt) {
    let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let dx = (p.wx >> 8) - (ex >> 8);
    let dy = (p.wy >> 8) - (ey >> 8);
    let (adx, ady) = (dx.abs(), dy.abs());
    let mut m;
    {
        let e = w.en(i);
        if adx > ady {
            m = adx;
            e.a.facing = if dx > 0 { 3 } else { 2 };
        } else {
            m = ady;
            e.a.facing = if dy > 0 { 0 } else { 1 };
        }
        if m == 0 {
            m = 1;
        }
        e.moving = true;
    }
    let tdiv = |a: i64, b: i64| if b == 0 { 0 } else { a / b };
    // JS: (dx * speed) << 8 as int32, divided by m << 8
    let num_x = ((dx as i64 * p.speed as i64) as i32).wrapping_shl(8) as i64;
    let num_y = ((dy as i64 * p.speed as i64) as i32).wrapping_shl(8) as i64;
    let vx = tdiv(num_x, (m << 8) as i64) as i32;
    let vy = tdiv(num_y, (m << 8) as i64) as i32;
    enemy::try_move(w, i, vx, vy, vx, vy);
}
