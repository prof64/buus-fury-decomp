//! Collision grids: header +0x34 (walk: player; copied to the enemy grid at load) and +0x38 (projectiles),
//! webfoot-LZ, 0x2000 bytes = 256 rows x 32 bytes, bit `x & 7` of byte `row * 32 + (x >> 3)`, 1 = passable,
//! one bit per 8x8 px tile (tools/battle/COLLISION.md).

use serde_json::{json, Value};
use wf_rom::hx;

use crate::rom_maps::Header;
use crate::Ctx;

pub const GRID: usize = 256;

pub struct Grids {
    /// row-major 256x256, 1 = passable
    pub walk: Vec<u8>,
    pub shot: Vec<u8>,
}

pub fn unpack(d: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; GRID * GRID];
    for (i, o) in out.iter_mut().enumerate() {
        let (y, x) = (i / GRID, i % GRID);
        let b = d.get(y * 32 + (x >> 3)).copied().unwrap_or(0);
        *o = (b >> (x & 7)) & 1;
    }
    out
}

pub fn grids(ctx: &mut Ctx, h: &Header) -> Grids {
    let walk = unpack(ctx.blob(h.walk));
    let shot = unpack(ctx.blob(h.shot));
    Grids { walk, shot }
}

/// Run lengths over the bits (row-major), first run = blocked (0), LEB128 varints, base64
/// (the encoding of data/battle/buu_collision.json).
pub fn rle(bits: &[u8]) -> String {
    let mut out = Vec::new();
    let mut cur = 0u8;
    let mut run: u64 = 0;
    let push = |mut n: u64, out: &mut Vec<u8>| loop {
        let b = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            break;
        }
    };
    for &b in bits {
        if b == cur {
            run += 1;
        } else {
            push(run, &mut out);
            cur ^= 1;
            run = 1;
        }
    }
    push(run, &mut out);
    b64(&out)
}

pub fn unrle(s: &str, n: usize) -> Vec<u8> {
    let b = unb64(s);
    let mut out = vec![0u8; n];
    let (mut i, mut p, mut cur) = (0usize, 0usize, 0u8);
    while i < b.len() {
        let (mut v, mut sh) = (0usize, 0);
        loop {
            let c = b[i];
            i += 1;
            v |= ((c & 0x7F) as usize) << sh;
            sh += 7;
            if c & 0x80 == 0 {
                break;
            }
        }
        if cur == 1 {
            for o in out.iter_mut().skip(p).take(v) {
                *o = 1;
            }
        }
        p += v;
        cur ^= 1;
    }
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64(d: &[u8]) -> String {
    let mut s = String::new();
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(B64[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn unb64(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut n) = (0u32, 0);
    for ch in s.bytes() {
        let v = match B64.iter().position(|&c| c == ch) {
            Some(v) => v as u32,
            None => continue,
        };
        acc = acc << 6 | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((acc >> n) as u8);
        }
    }
    out
}

/// Bounding box (tiles, x1/y1 exclusive) of the passable walk tiles.
pub fn walk_bounds(g: &Grids) -> Option<[usize; 4]> {
    let mut b: Option<[usize; 4]> = None;
    for (i, &v) in g.walk.iter().enumerate() {
        if v == 1 {
            let (y, x) = (i / GRID, i % GRID);
            b = Some(match b {
                None => [x, y, x + 1, y + 1],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
            });
        }
    }
    b
}

pub fn to_json(g: &Grids, h: &Header) -> Value {
    json!({
        "tile_px": 8,
        "width": GRID,
        "height": GRID,
        "encoding": "per grid: run lengths over width*height tiles (row-major), first run = blocked, LEB128 varints, base64",
        "walk": rle(&g.walk),
        "shot": rle(&g.shot),
        "walk_bounds": walk_bounds(g),
        "origin": [0, 0],
        "notes": "1 = passable. walk: player and enemies (the game copies it to a private enemy grid where every enemy \
                  marks its footprint); shot: projectiles explode on blocked tiles. Bare terrain: NPCs/enemies/objects \
                  mark their own tiles at runtime. Grid tile (0,0) is world pixel (0,0) = composite pixel (0,0).",
        "rom": {"header": hx(h.addr), "walk": hx(h.walk), "shot": hx(h.shot)},
    })
}
