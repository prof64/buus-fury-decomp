//! Geometry shared by every system: rects in integer pixels, directions 0 down, 1 up, 2 left, 3 right, positions
//! in 24.8 fixed point.

use serde::{Deserialize, Serialize};

pub type Dir = u8;
pub const DIR_VEC: [(i32, i32); 4] = [(0, 1), (0, -1), (-1, 0), (1, 0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    #[inline]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }
    pub fn at(r: [i32; 4], x: i32, y: i32) -> Rect {
        Rect::new(r[0] + x, r[1] + y, r[2] + x, r[3] + y)
    }
    pub fn inflate(&self, d: i32) -> Rect {
        Rect::new(self.x0 - d, self.y0 - d, self.x1 + d, self.y1 + d)
    }
    /// x0 + (w >> 1) style centre: (x0 + x1) >> 1
    pub fn center(&self) -> (i32, i32) {
        ((self.x0 + self.x1) >> 1, (self.y0 + self.y1) >> 1)
    }
}

/// the Webfoot strict overlap test
#[inline]
pub fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// dominant-axis facing (ties vertical)
#[inline]
pub fn facing_toward(dx: i32, dy: i32) -> Dir {
    if dx.abs() > dy.abs() {
        if dx > 0 {
            3
        } else {
            2
        }
    } else if dy > 0 {
        0
    } else {
        1
    }
}

/// box of size w x h centred on (x, y) (the reference `centered`)
pub fn centered(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(x - (w >> 1), y - (h >> 1), x - (w >> 1) + w, y - (h >> 1) + h)
}
