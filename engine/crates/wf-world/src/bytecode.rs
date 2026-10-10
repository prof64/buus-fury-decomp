//! Just enough of the script bytecode (interpreter `0x08004038`) to find constant-argument calls of the natives that
//! move the player between areas and that start other events. The full VM lives in `wf-script`.
//!
//! Opcodes: 00 n8 push | 01 varint push | 02 n8 native | 03 n8 setter | 04..10, 14, 1B, 1E no operand | 11 end |
//! 12 jmp / 13 jz (s16 big-endian, target = opcode + 1 + off) | 1C loop (u16, target = opcode + 1 − off) |
//! 15..1A n8 | 1D n8 + n varints (push list). Varint: 7-bit groups MSB first, zig-zag.

use std::collections::{BTreeMap, BTreeSet};
use wf_rom::Rom;

pub const NATIVE_WARP_FADE: u8 = 0x76;
pub const NATIVE_WARP: u8 = 0x77;
pub const NATIVE_RUN_EVENT: u8 = 0xB7;
pub const NATIVE_RUN_SCRIPT: u8 = 0xD8;

fn varint(b: &[u8], mut i: usize) -> Option<(i64, usize)> {
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

#[derive(Clone, Debug)]
enum Ins {
    Push(Vec<i64>),
    Native(u8),
    End,
    Jump(usize, bool), // target, conditional
    Bin(u8),
    Neg,
    Pop1,
    Var(u8, u8),
}

fn decode(b: &[u8], off: usize) -> Option<(Ins, usize)> {
    let op = *b.get(off)?;
    let i = off + 1;
    Some(match op {
        0 => (Ins::Push(vec![*b.get(i)? as i64]), 2),
        1 => {
            let (v, j) = varint(b, i)?;
            (Ins::Push(vec![v]), j - off)
        }
        0x1D => {
            let n = *b.get(i)?;
            let mut j = i + 1;
            let mut v = Vec::new();
            for _ in 0..n {
                let (x, k) = varint(b, j)?;
                v.push(x);
                j = k;
            }
            (Ins::Push(v), j - off)
        }
        2 => (Ins::Native(*b.get(i)?), 2),
        3 => {
            b.get(i)?;
            (Ins::Pop1, 2)
        }
        0x15..=0x1A => (Ins::Var(op, *b.get(i)?), 2),
        0x12 | 0x13 | 0x1C => {
            let o = ((*b.get(i)? as i64) << 8) | *b.get(i + 1)? as i64;
            let t = if op == 0x1C {
                off as i64 + 1 - o
            } else {
                off as i64 + 1 + if o & 0x8000 != 0 { o - 0x10000 } else { o }
            };
            if t < 0 {
                return None;
            }
            (Ins::Jump(t as usize, op != 0x12), 3)
        }
        0x11 => (Ins::End, 1),
        0x06 => (Ins::Neg, 1),
        0x04 | 0x05 | 0x07..=0x10 | 0x1E => (Ins::Bin(op), 1),
        0x14 | 0x1B => (Ins::Pop1, 1),
        _ => return None,
    })
}

/// Symbolic stack value: a constant or an expression over unknowns (natives, variables).
#[derive(Clone, Debug, PartialEq)]
pub enum Sym {
    Const(i64),
    Expr(String),
}

impl Sym {
    pub fn konst(&self) -> Option<i64> {
        match self {
            Sym::Const(v) => Some(*v),
            _ => None,
        }
    }
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Sym::Const(v) => (*v).into(),
            Sym::Expr(e) => e.clone().into(),
        }
    }
    fn text(&self) -> String {
        match self {
            Sym::Const(v) => v.to_string(),
            Sym::Expr(e) => e.clone(),
        }
    }
}

/// A call of one of the wanted natives with its (symbolic) arguments.
#[derive(Clone, Debug)]
pub struct Call {
    pub native: u8,
    pub args: Vec<Sym>,
    /// byte offset in the code unit
    pub off: usize,
    /// true when the call is not on every path (some conditional jump precedes it)
    pub conditional: bool,
}

/// Instructions reachable from offset 0 (following both arms of conditional jumps).
fn reachable(b: &[u8]) -> BTreeMap<usize, (Ins, usize)> {
    let mut seen = BTreeMap::new();
    let mut work = vec![0usize];
    while let Some(o) = work.pop() {
        if seen.contains_key(&o) || o >= b.len() {
            continue;
        }
        let Some((ins, len)) = decode(b, o) else { continue };
        match &ins {
            Ins::End => {}
            Ins::Jump(t, cond) => {
                work.push(*t);
                if *cond {
                    work.push(o + len);
                }
            }
            _ => work.push(o + len),
        }
        seen.insert(o, (ins, len));
    }
    seen
}

/// Length of the code unit at offset 0 (end of the furthest reachable instruction).
pub fn unit_len(b: &[u8]) -> usize {
    reachable(b).iter().map(|(o, (_, l))| o + l).max().unwrap_or(0)
}

fn native_info(n: u8) -> Option<(usize, usize, &'static str)> {
    crate::natives_gen::NATIVES.iter().find(|r| r.0 == n).map(|r| (r.1 as usize, r.2 as usize, r.3))
}

/// Symbolic execution of the reachable instructions in address order (the stack is forgotten at every branch
/// target), recording the calls of the natives in `want`.
pub fn calls(b: &[u8], want: &[u8]) -> Vec<Call> {
    let r = reachable(b);
    let mut out = Vec::new();
    let mut st: Vec<Sym> = Vec::new();
    let mut prev_end = usize::MAX;
    let mut any_branch = false;
    let targets: BTreeSet<usize> =
        r.values().filter_map(|(i, _)| if let Ins::Jump(t, _) = i { Some(*t) } else { None }).collect();
    let pop = |st: &mut Vec<Sym>| st.pop().unwrap_or(Sym::Expr("?".into()));
    for (&o, (ins, len)) in &r {
        if o != prev_end || targets.contains(&o) {
            st.clear();
        }
        prev_end = o + len;
        match ins {
            Ins::Push(v) => st.extend(v.iter().map(|&x| Sym::Const(x))),
            Ins::Native(n) => {
                let (pops, pushes, name) = native_info(*n).unwrap_or((0, 0, "?"));
                let mut args: Vec<Sym> = (0..pops).map(|_| pop(&mut st)).collect();
                args.reverse();
                if want.contains(n) {
                    out.push(Call { native: *n, args: args.clone(), off: o, conditional: any_branch });
                }
                if native_info(*n).is_none() {
                    st.clear();
                }
                for _ in 0..pushes {
                    let a: Vec<String> = args.iter().map(|x| x.text()).collect();
                    st.push(Sym::Expr(format!("{name}({})", a.join(", "))));
                }
            }
            Ins::Bin(op) => {
                let y = pop(&mut st);
                let x = pop(&mut st);
                st.push(match (x.konst(), y.konst()) {
                    (Some(a), Some(b)) => match bin(*op, a, b) {
                        Some(v) => Sym::Const(v),
                        None => Sym::Expr(format!("({} {} {})", a, opname(*op), b)),
                    },
                    _ => Sym::Expr(format!("({} {} {})", x.text(), opname(*op), y.text())),
                });
            }
            Ins::Neg => {
                let x = pop(&mut st);
                st.push(match x.konst() {
                    Some(a) => Sym::Const(-a),
                    None => Sym::Expr(format!("-{}", x.text())),
                });
            }
            Ins::Pop1 => {
                pop(&mut st);
            }
            Ins::Var(op, v) => {
                if matches!(op, 0x16 | 0x17 | 0x19 | 0x1A) {
                    st.push(Sym::Expr(format!("var{v}")));
                }
            }
            Ins::Jump(_, c) => {
                if *c {
                    pop(&mut st);
                }
                any_branch |= *c;
            }
            Ins::End => st.clear(),
        }
    }
    out
}

fn opname(op: u8) -> &'static str {
    match op {
        4 => "&&",
        5 => "||",
        7 => "+",
        8 => "-",
        9 => "*",
        0xA => "/",
        0x1E => "%",
        0xB => "==",
        0xC => "!=",
        0xD => ">",
        0xE => ">=",
        0xF => "<",
        0x10 => "<=",
        _ => "?",
    }
}

fn bin(op: u8, a: i64, b: i64) -> Option<i64> {
    Some(match op {
        4 => (a != 0 && b != 0) as i64,
        5 => (a != 0 || b != 0) as i64,
        7 => a.wrapping_add(b),
        8 => a.wrapping_sub(b),
        9 => a.wrapping_mul(b),
        0xA => a.checked_div(b)?,
        0x1E => a.checked_rem(b)?,
        0xB => (a == b) as i64,
        0xC => (a != b) as i64,
        0xD => (a > b) as i64,
        0xE => (a >= b) as i64,
        0xF => (a < b) as i64,
        0x10 => (a <= b) as i64,
        _ => return None,
    })
}

/// Raw bytecode starting at `a` (the unit is cut at its reachable end by the scanner).
pub fn raw(rom: &Rom, a: u32) -> Vec<u8> {
    if !rom.isptr(a) {
        return Vec::new();
    }
    rom.bytes(a, 0x1000).to_vec()
}

/// Compressed bytecode: headerless webfoot-LZ stream at `a` (decoded with a synthetic container header).
pub fn inflate(rom: &Rom, a: u32) -> Vec<u8> {
    if !rom.isptr(a) {
        return Vec::new();
    }
    let mut buf = vec![1, 0, 0, 0, 0, 8, 0, 0];
    buf.extend_from_slice(rom.bytes(a, 0x4000));
    wf_rom::lz::decompress(&buf, 0).map(|(d, _)| d).unwrap_or_default()
}
