//! The object tile allocator (OBJ VRAM) of the field's world (allocator struct = the world `0x03001C4C`: free list
//! head +0, spare-entry list head +1, 138 block entries of 10 bytes at +0x694, the start -> entry index table at
//! +0xC00). 512 units of 64 bytes; `alloc` (`0x0801124C`) is best fit with an exact match taken at once, a larger
//! block is split and the allocation takes its top end; `free` (`0x0801142A`) merges with free neighbours in address
//! order. An area load resets it (`fresh`). Every sprite-showing object holds one block (actors: their sprite def's
//! tile bytes, def +2); a text box takes 0x1000 / 0x1000 / (portrait 0x1000) / 0x800. When an allocation does not
//! fit, `0x08011732` suspends objects (see `field::vram_purge`). VERIFIED: tools/superbuu/vramsim.py replays every
//! alloc / free of the Super Buu route (190,593 frames) against the emulator's allocator bytes.

use serde::{Deserialize, Serialize};

pub const ENTRIES: usize = 138;
pub const UNITS: u16 = 512;
const FF: u8 = 0xFF;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    /// 0 free, 1 used, 0xFF spare
    pub used: u8,
    pub start: u16,
    pub size: u16,
    /// address-order neighbours (higher / lower); spare entries: the spare list (next / previous)
    pub hi: u8,
    pub lo: u8,
    /// the free list (next / previous)
    pub fnext: u8,
    pub fprev: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjVram {
    pub free_head: u8,
    pub spare_head: u8,
    pub e: Vec<Block>,
    /// start unit -> entry (`+0xC00`, never cleared by a free)
    pub idx: Vec<u8>,
}

impl Default for ObjVram {
    fn default() -> Self {
        Self::fresh()
    }
}

impl ObjVram {
    /// the state an area load leaves: entry 0 = the whole free block, 1.. spare in order
    pub fn fresh() -> ObjVram {
        let mut e = vec![Block::default(); ENTRIES];
        e[0] = Block { used: 0, start: 0, size: UNITS, hi: FF, lo: FF, fnext: FF, fprev: FF };
        for (i, b) in e.iter_mut().enumerate().skip(1) {
            *b = Block {
                used: FF,
                start: 0xFFFF,
                size: 0xFFFF,
                hi: if i + 1 < ENTRIES { (i + 1) as u8 } else { FF },
                lo: if i > 1 { (i - 1) as u8 } else { FF },
                fnext: FF,
                fprev: FF,
            };
        }
        ObjVram { free_head: 0, spare_head: 1, e, idx: vec![0; UNITS as usize] }
    }

    /// `0x080116FC`: the largest free block in bytes (0 when the object list is full, 0x80 objects)
    pub fn room(&self, objects: usize) -> i32 {
        if objects == 0x80 {
            return 0;
        }
        let mut m = 0;
        let mut i = self.free_head;
        while i != FF {
            m = m.max(self.e[i as usize].size as i32);
            i = self.e[i as usize].fnext;
        }
        m << 6
    }

    fn unlink(&mut self, i: u8) {
        let (fnext, fprev) = (self.e[i as usize].fnext, self.e[i as usize].fprev);
        if fprev == FF {
            self.free_head = fnext;
        } else {
            self.e[fprev as usize].fnext = fnext;
        }
        if fnext != FF {
            self.e[fnext as usize].fprev = fprev;
        }
    }
    fn push_free(&mut self, i: u8) {
        let h = self.free_head;
        let b = &mut self.e[i as usize];
        b.used = 0;
        b.fprev = FF;
        b.fnext = h;
        if h != FF {
            self.e[h as usize].fprev = i;
        }
        self.free_head = i;
    }
    fn spare(&mut self, i: u8) {
        let h = self.spare_head;
        let b = &mut self.e[i as usize];
        b.used = FF;
        b.lo = FF;
        b.hi = h;
        if h != FF {
            self.e[h as usize].lo = i;
        }
        self.spare_head = i;
    }
    /// `0x080113B0`: i absorbs its higher neighbour j (j's place in the free list), j becomes spare
    fn merge(&mut self, i: u8, j: u8) {
        let bj = self.e[j as usize];
        let bi = &mut self.e[i as usize];
        bi.size = bi.size.wrapping_add(bj.size);
        bi.hi = bj.hi;
        if bj.hi != FF {
            self.e[bj.hi as usize].lo = i;
        }
        self.e[i as usize].fprev = bj.fprev;
        if bj.fprev != FF {
            self.e[bj.fprev as usize].fnext = i;
        } else {
            self.free_head = i;
        }
        self.e[i as usize].fnext = bj.fnext;
        if bj.fnext != FF {
            self.e[bj.fnext as usize].fprev = i;
        }
        self.spare(j);
    }

    /// `0x0801124C`: a block of `bytes` (>> 6 units); the handle (start unit * 2), or None
    pub fn alloc(&mut self, bytes: i32) -> Option<u16> {
        let need = (bytes >> 6) as u16;
        let (mut best, mut bsz) = (FF, u32::MAX);
        let mut i = self.free_head;
        while i != FF {
            let s = self.e[i as usize].size;
            if (s as u32) < bsz {
                if s == need {
                    self.e[i as usize].used = 1;
                    self.unlink(i);
                    let st = self.e[i as usize].start;
                    self.idx[st as usize & 511] = i;
                    return Some(st << 1);
                }
                if s > need {
                    best = i;
                    bsz = s as u32;
                }
            }
            i = self.e[i as usize].fnext;
        }
        if best == FF {
            return None;
        }
        if self.spare_head == FF {
            self.e[best as usize].used = 1;
            self.unlink(best);
            let st = self.e[best as usize].start;
            self.idx[st as usize & 511] = best;
            return Some(st << 1);
        }
        let s = self.spare_head;
        self.spare_head = self.e[s as usize].hi;
        if self.spare_head != FF {
            self.e[self.spare_head as usize].lo = FF;
        }
        let bh = self.e[best as usize].hi;
        self.e[s as usize].lo = best;
        self.e[s as usize].hi = bh;
        if bh != FF {
            self.e[bh as usize].lo = s;
        }
        self.e[best as usize].hi = s;
        self.e[best as usize].size -= need;
        let st = self.e[best as usize].start + self.e[best as usize].size;
        let b = &mut self.e[s as usize];
        b.used = 1;
        b.start = st;
        b.size = need;
        self.idx[st as usize & 511] = s;
        Some(st << 1)
    }

    /// `0x0801142A`: free the block of a handle
    pub fn free(&mut self, handle: u16) {
        let i = self.idx[(handle >> 1) as usize & 511];
        let b = self.e[i as usize];
        self.e[i as usize].used = 0;
        let (hi, lo) = (b.hi, b.lo);
        if hi == FF {
            if lo == FF {
                self.free_head = i;
                self.e[i as usize].fnext = FF;
                self.e[i as usize].fprev = FF;
                return;
            }
            if self.e[lo as usize].used == 0 {
                self.e[lo as usize].size = self.e[lo as usize].size.wrapping_add(b.size);
                self.e[lo as usize].hi = FF;
                self.spare(i);
            } else {
                self.push_free(i);
            }
            return;
        }
        let hu = self.e[hi as usize].used;
        if lo == FF {
            if hu == 0 {
                self.merge(i, hi);
            } else {
                self.push_free(i);
            }
            return;
        }
        let lu = self.e[lo as usize].used;
        if hu != 0 {
            if lu == 0 {
                self.e[lo as usize].size = self.e[lo as usize].size.wrapping_add(b.size);
                self.e[lo as usize].hi = b.hi;
                self.e[hi as usize].lo = b.lo;
                self.spare(i);
            } else {
                self.push_free(i);
            }
            return;
        }
        if lu != 0 {
            self.merge(i, hi);
            return;
        }
        self.unlink(hi);
        let h = self.e[hi as usize];
        self.e[lo as usize].size = b.size.wrapping_add(h.size).wrapping_add(self.e[lo as usize].size);
        self.e[lo as usize].hi = h.hi;
        if h.hi != FF {
            self.e[h.hi as usize].lo = b.lo;
        }
        self.spare(hi);
        self.spare(i);
    }

    /// the comparable state (a spare entry's start / size are stale)
    pub fn key(&self) -> Vec<(u8, u16, u16, u8, u8, u8, u8)> {
        let mut v = vec![(self.free_head, 0, 0, self.spare_head, 0, 0, 0)];
        for b in &self.e {
            v.push(if b.used == FF { (FF, 0, 0, b.hi, b.lo, 0, 0) } else { (b.used, b.start, b.size, b.hi, b.lo, b.fnext, b.fprev) });
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alloc_free_round_trip() {
        let mut v = ObjVram::fresh();
        let a = v.alloc(1024).unwrap();
        let b = v.alloc(512).unwrap();
        assert_eq!(a, (512 - 16) * 2);
        assert_eq!(b, (512 - 24) * 2);
        v.free(a);
        v.free(b);
        assert_eq!(v.room(1), 512 << 6);
        assert_eq!(v.free_head, 0);
    }
}
