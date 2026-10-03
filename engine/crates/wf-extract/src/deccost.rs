//! CPU cycles the game's IWRAM decompressor (`0x03000040`, ARM) spends on a compressed stream.
//!
//! The status page decompresses the character's portrait in every redraw, and how long the redraw takes decides
//! how many video frames the menu lags (wf-core `sndsim.rs`), so each portrait carries this number
//! (`portraits/index.json` `decode_cycles`). An instruction-path walk with ARM7TDMI costs: code and output in
//! IWRAM (1 cycle fetches, byte load 3, byte store 2), stream words from ROM (load 8), register-specified shifts
//! +1, taken branches 3. Against the emulator: 99,211 vs 99,577 and 110,084 vs 110,510 user-mode cycles for two
//! portraits (the rest is the sound DMA's cycle stealing); `work/earth/timing/decmodel.py` is the prototype.

const LDR_ROM: u32 = 8;
const B: u32 = 3;
const LDRB: u32 = 3;
const STRB: u32 = 2;
/// the offset thresholds of the length adjustment (literal pool `0x03000328` / `0x0300032C`)
const K1: u32 = 0x34FF;
const K2: u32 = 0x27F;

struct St<'a> {
    rom: &'a [u8],
    src: usize,
    r6: u32,
    sl: i32,
    c: u32,
    ok: bool,
}

impl St<'_> {
    fn word(&mut self) -> u32 {
        let v = self.rom.get(self.src..self.src + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        self.src += 4;
        match v {
            Some(v) => v,
            None => {
                self.ok = false;
                0
            }
        }
    }
    fn bit(&mut self) -> u32 {
        self.sl -= 1;
        if self.sl < 0 {
            self.r6 = self.word();
            self.sl = 0x1F;
            self.c += 1 + LDR_ROM + 1 + 1;
        } else {
            self.c += 4;
        }
        let b = self.r6 >> 31;
        self.r6 <<= 1;
        b
    }
    fn bits(&mut self, mut r8: u32) -> u32 {
        self.c += 1;
        if self.sl == 0 {
            self.r6 = self.word();
            self.sl = 0x20;
            self.c += LDR_ROM + 1;
        } else {
            self.c += 2;
        }
        let fp = r8.min(self.sl as u32);
        self.c += 10;
        let mut r7 = if fp == 0 { 0 } else { self.r6 >> (32 - fp) };
        self.r6 = if fp >= 32 { 0 } else { self.r6 << fp };
        self.sl -= fp as i32;
        r8 -= fp;
        if r8 == 0 {
            self.c += B;
            return r7;
        }
        self.c += 1;
        self.r6 = self.word();
        self.sl = 32 - r8 as i32;
        r7 = (self.r6 >> self.sl) + (r7 << r8);
        self.r6 <<= r8;
        self.c += LDR_ROM + 1 + 2 + 2 + 2 + B;
        r7
    }
    fn gamma(&mut self) -> u32 {
        let mut sb = 1u32;
        self.c += 1;
        loop {
            sb = (sb << 1) | self.bit();
            self.c += 1;
            if self.bit() != 0 {
                self.c += B;
            } else {
                self.c += 1;
                return sb;
            }
        }
    }
}

fn copy(st: &mut St, out: &mut Vec<u8>, ip: u32, sb: u32, rle: bool) {
    let ip = ip as usize;
    if ip == 0 || ip > out.len() {
        st.ok = false;
        return;
    }
    if rle {
        st.c += LDRB;
        let v = out[out.len() - ip];
        for k in 0..sb {
            out.push(v);
            st.c += STRB + 1 + if k < sb - 1 { B } else { 1 };
        }
    } else {
        for k in 0..sb {
            out.push(out[out.len() - ip]);
            st.c += LDRB + STRB + 1 + if k < sb - 1 { B } else { 1 };
        }
    }
    st.c += B;
}

/// Cycles of one call on the stream at ROM offset `src` (None: not a valid stream).
pub fn decode_cycles(rom: &[u8], src: usize) -> Option<u32> {
    let mut st = St { rom, src, r6: 0, sl: 0, c: 0, ok: true };
    let mut out: Vec<u8> = Vec::new();
    st.c += 10 + 1 + 1 + 1 + B;
    let (mut r5, mut ip, mut r4, mut r3) = (8u32, 1u32, 0u32, 1u32);
    loop {
        if !st.ok || out.len() > 0x20000 {
            return None;
        }
        if st.bit() != 0 {
            st.c += B + 1 + B;
            let r7 = st.bits(r3);
            out.push(r7.wrapping_add(r4) as u8);
            st.c += 1 + STRB;
            continue;
        }
        st.c += 1;
        if st.bit() != 0 {
            st.c += B;
            let mut sb = st.gamma();
            st.c += 1;
            if sb == 2 {
                st.c += 1;
                sb = st.gamma();
                st.c += 1;
                if ip != 1 {
                    st.c += B;
                    copy(&mut st, &mut out, ip, sb, false);
                } else {
                    st.c += 1 + B;
                    copy(&mut st, &mut out, ip, sb, true);
                }
                continue;
            }
            st.c += B;
            sb = sb.wrapping_sub(3);
            st.c += 1 + 1 + B;
            let r7 = st.bits(r5);
            ip = r7.wrapping_add(sb.wrapping_shl(r5));
            st.c += 2;
            sb = st.gamma();
            st.c += 1;
            if ip >= 0x10000 {
                st.c += 1 + B;
                copy(&mut st, &mut out, ip, sb + 3, false);
                continue;
            }
            st.c += 2 + 3 + 1;
            if ip >= K1 {
                st.c += 1 + B;
                copy(&mut st, &mut out, ip, sb + 2, false);
                continue;
            }
            st.c += 2 + 3 + 1;
            if ip >= K2 {
                st.c += 1 + B;
                copy(&mut st, &mut out, ip, sb + 1, false);
                continue;
            }
            st.c += 2 + 1;
            if ip > 0x7F {
                st.c += B;
                copy(&mut st, &mut out, ip, sb, false);
                continue;
            }
            st.c += 1 + 1 + 1;
            sb += 4;
            if ip == 1 {
                st.c += B;
                copy(&mut st, &mut out, ip, sb, true);
            } else {
                st.c += 1;
                copy(&mut st, &mut out, ip, sb, false);
            }
            continue;
        }
        st.c += 1;
        if st.bit() != 0 {
            st.c += 1 + 1 + B;
            let r7 = st.bits(4) as i32 - 1;
            st.c += 1;
            if r7 == 0 {
                st.c += B + STRB;
                out.push(0);
                continue;
            }
            st.c += 1;
            if r7 > 0 {
                if r7 as usize > out.len() {
                    return None;
                }
                st.c += B + LDRB + B + STRB;
                out.push(out[out.len() - r7 as usize]);
                continue;
            }
            st.c += 1;
            if st.bit() != 0 {
                st.c += 1;
                loop {
                    st.c += 1;
                    for k in 0..256 {
                        st.c += 1 + B;
                        let v = st.bits(8) as u8;
                        out.push(v);
                        st.c += STRB + 1 + if k < 255 { B } else { 1 };
                    }
                    if !st.ok {
                        return None;
                    }
                    if st.bit() != 0 {
                        st.c += B;
                    } else {
                        st.c += 1 + B;
                        break;
                    }
                }
                continue;
            }
            st.c += B + 1;
            r4 = 0;
            r3 = 7 + st.bit();
            st.c += 2;
            if r3 == 8 {
                st.c += B;
                continue;
            }
            st.c += 1 + 1 + B;
            r4 = st.bits(8);
            st.c += 1 + B;
            continue;
        }
        st.c += B + 1 + B;
        let r7 = st.bits(7);
        st.c += 1;
        if r7 != 0 {
            st.c += 1;
            ip = r7;
            st.c += 1 + 1 + B;
            let sb = st.bits(2) + 2;
            st.c += 2;
            if ip != 1 {
                st.c += B;
                copy(&mut st, &mut out, ip, sb, false);
            } else {
                st.c += 1 + B;
                copy(&mut st, &mut out, ip, sb, true);
            }
            continue;
        }
        st.c += B + 1 + B;
        let r7 = st.bits(2);
        st.c += 1;
        if r7 == 0 {
            st.c += B + 11 + B;
            break;
        }
        st.c += 1 + 1 + B;
        r5 = st.bits(r7 + 3);
        st.c += 1 + B;
    }
    st.ok.then_some(st.c)
}
