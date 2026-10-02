//! Webfoot LZ (JCALG1-style) decompressor, as run by the game from IWRAM (`0x03000040`, copied from
//! ROM `0x087B79E4`). Format notes: `tools/extract/GRAPHICS.md` §1.1.
//!
//! Container: `u32 type, u32 decompressed_size, stream…`; type 0 = stored, 1/2 = compressed
//! (the type also seeds the literal bit width).

use crate::Error;

struct Bits<'a> {
    d: &'a [u8],
    p: usize,
    buf: u32,
    n: u32,
}

impl<'a> Bits<'a> {
    #[inline]
    fn bit(&mut self) -> Result<u32, Error> {
        if self.n == 0 {
            let w = self
                .d
                .get(self.p..self.p + 4)
                .ok_or(Error::Lz("stream runs past the end of the ROM"))?;
            self.buf = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
            self.p += 4;
            self.n = 32;
        }
        self.n -= 1;
        Ok((self.buf >> self.n) & 1)
    }
    #[inline]
    fn bits(&mut self, k: u32) -> Result<u32, Error> {
        let mut v = 0;
        for _ in 0..k {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
    #[inline]
    fn gamma(&mut self) -> Result<u32, Error> {
        let mut v: u32 = 1;
        loop {
            v = v.wrapping_shl(1) | self.bit()?;
            if self.bit()? == 0 {
                return Ok(v);
            }
        }
    }
}

#[inline]
fn copy_back(out: &mut Vec<u8>, dist: usize, len: usize) -> Result<(), Error> {
    if dist == 0 || dist > out.len() {
        return Err(Error::Lz("back-reference before the start of the output"));
    }
    for _ in 0..len {
        let b = out[out.len() - dist];
        out.push(b);
    }
    Ok(())
}

/// Decompress the blob at ROM *offset* `off`. Returns the data and the offset just past the stream.
pub fn decompress(data: &[u8], off: usize) -> Result<(Vec<u8>, usize), Error> {
    let hdr = data.get(off..off + 8).ok_or(Error::Lz("header out of range"))?;
    let typ = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let usize_ = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
    if typ == 0 {
        let s = data
            .get(off + 8..off + 8 + usize_)
            .ok_or(Error::Lz("stored blob out of range"))?;
        return Ok((s.to_vec(), off + 8 + usize_));
    }
    if typ != 1 && typ != 2 {
        return Err(Error::Lz("bad container type"));
    }
    if usize_ > 0x100_0000 {
        return Err(Error::Lz("implausible size"));
    }
    let mut b = Bits { d: data, p: off + 8, buf: 0, n: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(usize_);
    let mut litbits = typ;
    let mut litoff: u32 = 0;
    let mut idxbits: u32 = 8;
    let mut last: usize = 1;
    loop {
        if out.len() > 0x100_0000 {
            return Err(Error::Lz("runaway stream"));
        }
        if b.bit()? == 1 {
            out.push((b.bits(litbits)?.wrapping_add(litoff)) as u8);
            continue;
        }
        if b.bit()? == 1 {
            let hi = b.gamma()?;
            let ln;
            if hi == 2 {
                ln = b.gamma()? as usize;
            } else {
                let hi = hi.wrapping_sub(3) as usize;
                last = b.bits(idxbits)? as usize + (hi << idxbits);
                let mut l = b.gamma()? as usize;
                if last >= 0x10000 {
                    l += 3;
                } else if last >= 0x37FF {
                    l += 2;
                } else if last >= 0x27F {
                    l += 1;
                } else if last <= 0x7F {
                    l += 4;
                }
                ln = l;
            }
            copy_back(&mut out, last, ln)?;
            continue;
        }
        if b.bit()? == 0 {
            let v = b.bits(7)? as usize;
            if v != 0 {
                last = v;
                let ln = b.bits(2)? as usize + 2;
                copy_back(&mut out, last, ln)?;
                continue;
            }
            let v = b.bits(2)?;
            if v == 0 {
                break;
            }
            idxbits = b.bits(v + 3)?;
            continue;
        }
        let v = b.bits(4)? as i32 - 1;
        if v == 0 {
            out.push(0);
        } else if v > 0 {
            copy_back(&mut out, v as usize, 1)?;
        } else if b.bit()? == 1 {
            loop {
                for _ in 0..256 {
                    out.push(b.bits(8)? as u8);
                }
                if b.bit()? == 0 {
                    break;
                }
            }
        } else {
            litoff = 0;
            litbits = 7 + b.bit()?;
            if litbits != 8 {
                litoff = b.bits(8)?;
            }
        }
    }
    Ok((out, b.p))
}
