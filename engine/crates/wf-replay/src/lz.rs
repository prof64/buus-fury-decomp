//! A small LZ77 byte compressor (no dependencies) for snapshots: `varint(len)` then tokens
//! `varint(literal count) literals [varint(match len - 4) varint(offset)]`. Offset 1 matches make long runs (the
//! enemy collision layer of a snapshot is mostly runs) nearly free; typical 66 KB battle snapshots shrink ~10x.

use crate::{err, put_varint, Reader, Result};

const HASH_BITS: u32 = 14;
const MIN: usize = 4;
const MAX_OFF: usize = 1 << 20;

fn h4(b: &[u8], i: usize) -> usize {
    let v = u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    (v.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
}

pub fn compress(src: &[u8]) -> Vec<u8> {
    let n = src.len();
    let mut out = Vec::with_capacity(n / 4 + 16);
    put_varint(&mut out, n as u64);
    let mut table = vec![0u32; 1 << HASH_BITS];
    let (mut anchor, mut i) = (0usize, 0usize);
    while i + MIN <= n {
        let h = h4(src, i);
        let cand = table[h] as usize;
        table[h] = (i + 1) as u32;
        if cand > 0 {
            let c = cand - 1;
            if i - c <= MAX_OFF && src[c..c + MIN] == src[i..i + MIN] {
                let mut m = MIN;
                while i + m < n && src[c + m] == src[i + m] {
                    m += 1;
                }
                put_varint(&mut out, (i - anchor) as u64);
                out.extend_from_slice(&src[anchor..i]);
                put_varint(&mut out, (m - MIN) as u64);
                put_varint(&mut out, (i - c) as u64);
                // index a few positions inside the match so the next ones find it
                let end = i + m;
                let mut k = i + 1;
                while k + MIN <= n && k < end {
                    table[h4(src, k)] = (k + 1) as u32;
                    k += if m > 64 { 16 } else { 1 };
                }
                i = end;
                anchor = i;
                continue;
            }
        }
        i += 1;
    }
    put_varint(&mut out, (n - anchor) as u64);
    out.extend_from_slice(&src[anchor..]);
    out
}

pub fn decompress(b: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(b);
    let n = r.varint()? as usize;
    if n > 1 << 30 {
        return err("lz: size too large");
    }
    let mut out = Vec::with_capacity(n);
    loop {
        let lit = r.varint()? as usize;
        out.extend_from_slice(r.bytes(lit)?);
        if out.len() >= n {
            break;
        }
        let m = r.varint()? as usize + MIN;
        let off = r.varint()? as usize;
        if off == 0 || off > out.len() || out.len() + m > n {
            return err("lz: bad match");
        }
        let start = out.len() - off;
        for k in 0..m {
            let v = out[start + k];
            out.push(v);
        }
    }
    if out.len() != n {
        return err("lz: size mismatch");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let mut v: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 13) as u8).collect();
        v.extend(std::iter::repeat_n(0u8, 70_000));
        v.extend((0..3000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
        for s in [&v[..], &v[..3], &[][..], &v[100..200]] {
            let c = compress(s);
            assert_eq!(decompress(&c).unwrap(), s);
        }
        assert!(compress(&v).len() < 6000, "{}", compress(&v).len());
    }
}
