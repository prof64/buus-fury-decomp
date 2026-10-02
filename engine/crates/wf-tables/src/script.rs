//! Webfoot bytecode scripts (interpreter 0x08004038): disassembler, a small VM for loot/item scripts, and the
//! Mersenne Twister used for the Monte-Carlo loot expectations (bit-identical to Python's `random.Random(seed)`
//! so the simulated drop rates match the research data exactly).
//!
//! Opcodes: 00 n8 push | 01 varint push | 02 n8 native | 03 n8 setter | 04 and 05 or 06 neg 07 add 08 sub
//! 09 mul 0a div 1e mod | 0b eq 0c ne 0d gt 0e ge 0f lt 10 le | 11 end | 12 s16 jump | 13 s16 jz | 14 pop |
//! 15..1a n8 var ops | 1b loop push | 1c u16 loop | 1d n8 varint*n push n.
//! varint: 7-bit groups MSB first (bit 7 = continue), zig-zag (odd => -(v>>1)).

use crate::util::Ctx;

pub const SCRIPTS: u32 = 0x0879_B704;

fn nat_name(n: u8) -> &'static str {
    match n {
        0xd5 => "is_equipped(item)",
        0xd8 => "run_script(idx,p0,p1)",
        0xdf => "eq_hp_regen+=n*0x445>>8",
        0xe0 => "eq_thorns+=n*0xa4>>6",
        0xe1 => "eq_END+END2+=n",
        0xe2 => "eq_POW+=n",
        0xe3 => "eq_SPD+=n",
        0xe4 => "eq_STR+=n",
        0xe5 => "eq_block_dmg/=2",
        0xe6 => "eq_energy_cost/=2",
        0xe7 => "eq_eblock_cost/=2",
        0xe8 => "eq_ss_drain*=7/8",
        0xf5 => "eq_byte0+=n",
        _ => "",
    }
}

pub fn varint(b: &[u8], mut i: usize) -> Option<(i64, usize)> {
    let mut v: i64 = 0;
    loop {
        let c = *b.get(i)?;
        i += 1;
        v = (v << 7) | (c & 0x7f) as i64;
        if c & 0x80 == 0 {
            break;
        }
    }
    Some((if v & 1 == 1 { -(v >> 1) } else { v >> 1 }, i))
}

/// Disassemble (`re/buu/scriptdis.py:dis_script` text format) starting at `addr` over `maxlen` ROM bytes.
pub fn dis(c: &Ctx, addr: u32, maxlen: usize) -> Vec<(u32, String)> {
    dis_bytes(c.bytes(addr, maxlen), addr, true)
}

pub fn dis_bytes(b: &[u8], addr: u32, stop_at_end: bool) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let a = addr.wrapping_add(i as u32);
        let op = b[i];
        i += 1;
        let r: Option<String> = (|| {
            Some(match op {
                0 => {
                    let v = *b.get(i)?;
                    i += 1;
                    format!("push {v}")
                }
                1 => {
                    let (v, j) = varint(b, i)?;
                    i = j;
                    format!("push {v}")
                }
                2 => {
                    let n = *b.get(i)?;
                    i += 1;
                    format!("native 0x{n:02x} {}", nat_name(n))
                }
                3 => {
                    let n = *b.get(i)?;
                    i += 1;
                    format!("set 0x{n:02x}")
                }
                0x11 => "end".into(),
                0x12 | 0x13 | 0x1c => {
                    let off = ((*b.get(i)? as i64) << 8) | *b.get(i + 1)? as i64;
                    let tgt = if op == 0x1c {
                        a as i64 + 1 - off
                    } else {
                        let off = if off & 0x8000 != 0 { off - 0x10000 } else { off };
                        a as i64 + 1 + off
                    };
                    i += 2;
                    let nm = match op {
                        0x12 => "jmp",
                        0x13 => "jz",
                        _ => "loop",
                    };
                    format!("{nm} {tgt:08x}")
                }
                0x15..=0x1a => {
                    let n = *b.get(i)?;
                    i += 1;
                    format!("varop{op:02x} 0x{n:02x}")
                }
                0x1d => {
                    let n = *b.get(i)?;
                    i += 1;
                    let mut vals = Vec::new();
                    for _ in 0..n {
                        let (v, j) = varint(b, i)?;
                        i = j;
                        vals.push(v.to_string());
                    }
                    format!("pushn [{}]", vals.join(", "))
                }
                4 => "and".into(),
                5 => "or".into(),
                6 => "neg".into(),
                7 => "add".into(),
                8 => "sub".into(),
                9 => "mul".into(),
                0xa => "div".into(),
                0x1e => "mod".into(),
                0xb => "eq".into(),
                0xc => "ne".into(),
                0xd => "gt".into(),
                0xe => "ge".into(),
                0xf => "lt".into(),
                0x10 => "le".into(),
                0x14 => "pop".into(),
                0x1b => "loop_push".into(),
                _ => format!("?? 0x{op:02x}"),
            })
        })();
        let Some(s) = r else { break };
        let unknown = s.starts_with("??");
        let end = s == "end";
        out.push((a, s));
        if unknown || (end && stop_at_end) {
            break;
        }
    }
    out
}

/// Code bytes of script `idx` of the script table: header → blob; blob byte 2 = 0 (raw code at +3) or
/// 4 (webfoot-LZ compressed at +4).
pub fn script_code(c: &Ctx, idx: u32) -> Vec<u8> {
    let hdr = c.u32(SCRIPTS + 4 * idx);
    let blob = c.u32(hdr);
    if c.u8(blob + 2) == 4 {
        c.rom.lz(blob + 4).unwrap_or_default()
    } else {
        c.bytes(blob + 3, 20000).to_vec()
    }
}

/// MT19937 seeded like CPython's `random.Random(int)`.
pub struct Mt {
    mt: [u32; 624],
    i: usize,
}

impl Mt {
    fn init_genrand(s: u32) -> Mt {
        let mut mt = [0u32; 624];
        mt[0] = s;
        for i in 1..624 {
            mt[i] = 1812433253u32.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        Mt { mt, i: 624 }
    }
    /// `random.Random(seed)` for a non-negative seed < 2^32.
    pub fn python(seed: u32) -> Mt {
        let key = [seed];
        let mut m = Mt::init_genrand(19650218);
        let n = 624usize;
        let (mut i, mut j) = (1usize, 0usize);
        let mut k = n.max(key.len());
        while k > 0 {
            m.mt[i] = (m.mt[i] ^ (m.mt[i - 1] ^ (m.mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= n {
                m.mt[0] = m.mt[n - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = n - 1;
        while k > 0 {
            m.mt[i] = (m.mt[i] ^ (m.mt[i - 1] ^ (m.mt[i - 1] >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= n {
                m.mt[0] = m.mt[n - 1];
                i = 1;
            }
            k -= 1;
        }
        m.mt[0] = 0x8000_0000;
        m
    }
    pub fn next_u32(&mut self) -> u32 {
        if self.i >= 624 {
            for k in 0..624 {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % 624] & 0x7fff_ffff);
                let mut v = self.mt[(k + 397) % 624] ^ (y >> 1);
                if y & 1 == 1 {
                    v ^= 0x9908_b0df;
                }
                self.mt[k] = v;
            }
            self.i = 0;
        }
        let mut y = self.mt[self.i];
        self.i += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
    /// `getrandbits(16)`
    pub fn bits16(&mut self) -> i64 {
        (self.next_u32() >> 16) as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Drop {
    Zenie(i64),
    Supply(i64),
    Equip(i64),
}

/// Python re-implementation of the VM (`re/buu/vm.py`): enough natives for loot / item scripts.
pub struct Vm<'a> {
    pub g: [i64; 4],
    pub p: [i64; 4],
    pub drops: Vec<Drop>,
    pub rng: Mt,
    code: std::collections::HashMap<u32, std::rc::Rc<Vec<u8>>>,
    c: &'a Ctx<'a>,
}

impl<'a> Vm<'a> {
    pub fn new(c: &'a Ctx<'a>, seed: u32) -> Vm<'a> {
        Vm { g: [0; 4], p: [0; 4], drops: Vec::new(), rng: Mt::python(seed), code: Default::default(), c }
    }

    fn native(&mut self, n: u8, st: &mut Vec<i64>) {
        match n {
            0..=3 => st.push(self.g[n as usize]),
            4..=7 => st.push(self.p[n as usize - 4]),
            0x19 => {
                let v = st.pop().unwrap();
                let r = self.rng.bits16();
                st.push((r * v) >> 16);
            }
            0x1a => {
                let v = st.pop().unwrap();
                let r = self.rng.bits16();
                st.push(if r < ((v * 0x28F5D) >> 8) { 1 } else { 0 });
            }
            0x84 => {
                let v = st.pop().unwrap();
                self.drops.push(Drop::Zenie([5, 25, 50, 100][v as usize]));
            }
            0x85 => {
                let v = st.pop().unwrap();
                self.drops.push(Drop::Supply(v));
            }
            0x86 => {
                let v = st.pop().unwrap();
                self.drops.push(Drop::Equip(v));
            }
            0x92 => {
                st.pop();
                st.push(0); // nothing equipped
            }
            0xd7 => {
                let p0 = st.pop().unwrap();
                let idx = st.pop().unwrap();
                self.p[0] = p0;
                self.run(idx as u32);
            }
            0xd8 => {
                let p1 = st.pop().unwrap();
                let p0 = st.pop().unwrap();
                let idx = st.pop().unwrap();
                self.p[0] = p0;
                self.p[1] = p1;
                self.run(idx as u32);
            }
            _ => panic!("vm: native {n:#x} not implemented"),
        }
    }

    fn setter(&mut self, n: u8, v: i64) {
        match n {
            0..=3 => self.g[n as usize] = v,
            4..=7 => self.p[n as usize - 4] = v,
            _ => panic!("vm: setter {n:#x}"),
        }
    }

    pub fn run(&mut self, idx: u32) {
        let b = match self.code.get(&idx) {
            Some(b) => std::rc::Rc::clone(b),
            None => {
                let b = std::rc::Rc::new(script_code(self.c, idx));
                self.code.insert(idx, std::rc::Rc::clone(&b));
                b
            }
        };
        let mut i = 0usize;
        let mut st: Vec<i64> = Vec::new();
        let mut lp: Vec<i64> = Vec::new();
        loop {
            let op = b[i];
            i += 1;
            match op {
                0 => {
                    st.push(b[i] as i64);
                    i += 1;
                }
                1 => {
                    let (v, j) = varint(&b, i).unwrap();
                    i = j;
                    st.push(v);
                }
                2 => {
                    let n = b[i];
                    i += 1;
                    self.native(n, &mut st);
                }
                3 => {
                    let n = b[i];
                    i += 1;
                    let v = st.pop().unwrap();
                    self.setter(n, v);
                }
                4 | 5 => {
                    let y = st.pop().unwrap();
                    let x = st.pop().unwrap();
                    st.push(if op == 4 { (x != 0 && y != 0) as i64 } else { (x != 0 || y != 0) as i64 });
                }
                6 => {
                    let x = st.pop().unwrap();
                    st.push(-x);
                }
                7..=0x10 | 0x1e => {
                    let y = st.pop().unwrap();
                    let x = st.pop().unwrap();
                    let r = match op {
                        7 => x + y,
                        8 => x - y,
                        9 => x * y,
                        0xa => x / y,
                        0x1e => x - (x / y) * y,
                        0xb => (x == y) as i64,
                        0xc => (x != y) as i64,
                        0xd => (x > y) as i64,
                        0xe => (x >= y) as i64,
                        0xf => (x < y) as i64,
                        _ => (x <= y) as i64,
                    };
                    st.push(r);
                }
                0x11 => return,
                0x12 => {
                    let mut off = ((b[i] as i64) << 8) | b[i + 1] as i64;
                    if off & 0x8000 != 0 {
                        off -= 0x10000;
                    }
                    i = (i as i64 + off) as usize;
                }
                0x13 => {
                    let mut off = ((b[i] as i64) << 8) | b[i + 1] as i64;
                    if off & 0x8000 != 0 {
                        off -= 0x10000;
                    }
                    if st.pop().unwrap() == 0 {
                        i = (i as i64 + off) as usize;
                    } else {
                        i += 2;
                    }
                }
                0x14 => {
                    st.pop();
                }
                0x1b => lp.push(st.pop().unwrap()),
                0x1c => {
                    let off = ((b[i] as usize) << 8) | b[i + 1] as usize;
                    *lp.last_mut().unwrap() -= 1;
                    if *lp.last().unwrap() != 0 {
                        i -= off;
                    } else {
                        lp.pop();
                        i += 2;
                    }
                }
                0x1d => {
                    let n = b[i];
                    i += 1;
                    for _ in 0..n {
                        let (v, j) = varint(&b, i).unwrap();
                        i = j;
                        st.push(v);
                    }
                }
                _ => panic!("vm: op {op:#x}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mt_matches_python() {
        // python: r = random.Random(1); [r.getrandbits(32) for _ in range(3)]
        let mut m = super::Mt::python(1);
        assert_eq!([m.next_u32(), m.next_u32(), m.next_u32()], [577090037, 2444712010, 3639700191]);
    }
}
