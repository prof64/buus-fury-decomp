//! Instruction set of the Webfoot bytecode VM (interpreter `0x08004038`, opcode table `0x080554C0`).
//!
//! Every handler was read from the ROM (see `re/buu/SCRIPT.md` §3). Values are 32-bit two's complement;
//! all arithmetic wraps. Branch operands are 16-bit **big-endian, unsigned**, relative to the address of
//! the operand itself: `jmp`/`jz` can only jump forward, `loop` only backward.

use serde::Serialize;
use std::collections::BTreeMap;

/// One decoded instruction. Branch targets are code offsets (relative to the start of the script code).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "op", content = "arg", rename_all = "snake_case")]
pub enum Op {
    /// 00 n8: push n8 (zero-extended)
    PushU8(u8),
    /// 01 varint: push a zig-zag varint
    Push(i32),
    /// 1d n8 varint×n: push n varints in order
    PushN(Vec<i32>),
    /// 02 n8: call native n
    Native(u8),
    /// 03 n8: pop v; setter[n](v)
    Set(u8),
    /// 04 logical and (both non-zero → 1)
    And,
    /// 05 logical or
    Or,
    /// 06 negate top
    Neg,
    Add,
    Sub,
    Mul,
    /// 0a: second / top, truncating toward zero (RVCT `__rt_sdiv`)
    Div,
    /// 1e: remainder of second / top (sign of the dividend)
    Mod,
    Eq,
    Ne,
    /// second > top (signed); likewise ge / lt / le
    Gt,
    Ge,
    Lt,
    Le,
    /// 11: end of script (null handler: the interpreter loop returns)
    End,
    /// 12 u16: jump forward
    Jmp(usize),
    /// 13 u16: pop; jump forward if zero
    Jz(usize),
    /// 14: drop top
    Pop,
    /// 15 n: `var[n] += 1` (statement)
    Inc(u8),
    /// 16 n: `++var[n]` (pushes the new value)
    PreInc(u8),
    /// 17 n: `var[n]++` (pushes the old value)
    PostInc(u8),
    /// 18 n: `var[n] -= 1` (statement)
    Dec(u8),
    /// 19 n: `--var[n]` (pushes the new value)
    PreDec(u8),
    /// 1a n: `var[n]--` (pushes the old value)
    PostDec(u8),
    /// 1b: pop the repeat count onto the loop stack
    LoopPush,
    /// 1c u16: decrement loop counter; non-zero → jump back to the target, else pop loop stack
    Loop(usize),
}

#[derive(Clone, Debug, Serialize)]
pub struct Insn {
    pub off: usize,
    pub len: usize,
    pub op: Op,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    OutOfBounds(usize),
    BadOpcode { off: usize, byte: u8 },
    BadTarget { off: usize, target: usize },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::OutOfBounds(o) => write!(f, "code runs past the end at +{o:#x}"),
            DecodeError::BadOpcode { off, byte } => write!(f, "invalid opcode {byte:#04x} at +{off:#x}"),
            DecodeError::BadTarget { off, target } => write!(f, "branch at +{off:#x} to +{target:#x} outside the code"),
        }
    }
}

/// Zig-zag varint exactly as handler 0x08003C8A: 7-bit groups MSB first, bit 7 = continue, accumulated in a
/// 32-bit register (wraps), then odd → -(v >> 1), even → v >> 1 (arithmetic shifts).
pub fn varint(b: &[u8], mut i: usize) -> Option<(i32, usize)> {
    let mut v: u32 = 0;
    loop {
        let c = *b.get(i)?;
        i += 1;
        v = (v << 7).wrapping_add((c & 0x7f) as u32);
        if c & 0x80 == 0 {
            break;
        }
    }
    let s = (v as i32) >> 1;
    Some((if v & 1 == 1 { s.wrapping_neg() } else { s }, i))
}

/// Encode a value as the shortest zig-zag varint the game's decoder reads back (used by tests / assemblers).
pub fn encode_varint(v: i32) -> Vec<u8> {
    let z: u32 = if v < 0 { ((v.wrapping_neg() as u32) << 1) | 1 } else { (v as u32) << 1 };
    let mut groups = vec![(z & 0x7f) as u8];
    let mut r = z >> 7;
    while r != 0 {
        groups.push((r & 0x7f) as u8 | 0x80);
        r >>= 7;
    }
    groups.reverse();
    groups
}

fn be16(b: &[u8], i: usize) -> Option<usize> {
    Some(((*b.get(i)? as usize) << 8) | *b.get(i + 1)? as usize)
}

/// Decode the single instruction at `off`.
pub fn decode_at(b: &[u8], off: usize) -> Result<Insn, DecodeError> {
    let oob = DecodeError::OutOfBounds(off);
    let op = *b.get(off).ok_or(oob.clone())?;
    let mut i = off + 1;
    let byte_arg = |i: &mut usize| -> Result<u8, DecodeError> {
        let v = *b.get(*i).ok_or(DecodeError::OutOfBounds(*i))?;
        *i += 1;
        Ok(v)
    };
    let o = match op {
        0x00 => Op::PushU8(byte_arg(&mut i)?),
        0x01 => {
            let (v, j) = varint(b, i).ok_or(oob.clone())?;
            i = j;
            Op::Push(v)
        }
        0x02 => Op::Native(byte_arg(&mut i)?),
        0x03 => Op::Set(byte_arg(&mut i)?),
        0x04 => Op::And,
        0x05 => Op::Or,
        0x06 => Op::Neg,
        0x07 => Op::Add,
        0x08 => Op::Sub,
        0x09 => Op::Mul,
        0x0a => Op::Div,
        0x0b => Op::Eq,
        0x0c => Op::Ne,
        0x0d => Op::Gt,
        0x0e => Op::Ge,
        0x0f => Op::Lt,
        0x10 => Op::Le,
        0x11 => Op::End,
        0x12 | 0x13 | 0x1c => {
            let d = be16(b, i).ok_or(oob.clone())?;
            let t = if op == 0x1c {
                i.checked_sub(d).ok_or(DecodeError::BadTarget { off, target: usize::MAX })?
            } else {
                i + d
            };
            i += 2;
            match op {
                0x12 => Op::Jmp(t),
                0x13 => Op::Jz(t),
                _ => Op::Loop(t),
            }
        }
        0x14 => Op::Pop,
        0x15 => Op::Inc(byte_arg(&mut i)?),
        0x16 => Op::PreInc(byte_arg(&mut i)?),
        0x17 => Op::PostInc(byte_arg(&mut i)?),
        0x18 => Op::Dec(byte_arg(&mut i)?),
        0x19 => Op::PreDec(byte_arg(&mut i)?),
        0x1a => Op::PostDec(byte_arg(&mut i)?),
        0x1b => Op::LoopPush,
        0x1d => {
            let n = byte_arg(&mut i)?;
            let mut v = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (x, j) = varint(b, i).ok_or(oob.clone())?;
                i = j;
                v.push(x);
            }
            Op::PushN(v)
        }
        0x1e => Op::Mod,
        _ => return Err(DecodeError::BadOpcode { off, byte: op }),
    };
    Ok(Insn { off, len: i - off, op: o })
}

/// Successor offsets of an instruction (fall-through first).
pub fn successors(ins: &Insn) -> Vec<usize> {
    let next = ins.off + ins.len;
    match ins.op {
        Op::End => vec![],
        Op::Jmp(t) => vec![t],
        Op::Jz(t) | Op::Loop(t) => vec![next, t],
        _ => vec![next],
    }
}

/// Decode every instruction reachable from offset 0 (the interpreter's only entry point).
pub fn decode_reachable(b: &[u8]) -> Result<BTreeMap<usize, Insn>, DecodeError> {
    let mut out = BTreeMap::new();
    let mut work = vec![0usize];
    while let Some(o) = work.pop() {
        if out.contains_key(&o) {
            continue;
        }
        let ins = decode_at(b, o)?;
        for s in successors(&ins) {
            if s >= b.len() {
                return Err(DecodeError::BadTarget { off: o, target: s });
            }
            work.push(s);
        }
        out.insert(o, ins);
    }
    Ok(out)
}

/// Raw opcode byte → mnemonic.
pub fn mnemonic(op: &Op) -> &'static str {
    match op {
        Op::PushU8(_) | Op::Push(_) => "push",
        Op::PushN(_) => "pushn",
        Op::Native(_) => "native",
        Op::Set(_) => "set",
        Op::And => "and",
        Op::Or => "or",
        Op::Neg => "neg",
        Op::Add => "add",
        Op::Sub => "sub",
        Op::Mul => "mul",
        Op::Div => "div",
        Op::Mod => "mod",
        Op::Eq => "eq",
        Op::Ne => "ne",
        Op::Gt => "gt",
        Op::Ge => "ge",
        Op::Lt => "lt",
        Op::Le => "le",
        Op::End => "end",
        Op::Jmp(_) => "jmp",
        Op::Jz(_) => "jz",
        Op::Pop => "pop",
        Op::Inc(_) => "inc",
        Op::PreInc(_) => "preinc",
        Op::PostInc(_) => "postinc",
        Op::Dec(_) => "dec",
        Op::PreDec(_) => "predec",
        Op::PostDec(_) => "postdec",
        Op::LoopPush => "loop_push",
        Op::Loop(_) => "loop",
    }
}

/// One line of assembly text; `name` resolves native / variable ids to names.
pub fn format_insn(ins: &Insn, name: &dyn Fn(u8) -> String) -> String {
    let m = mnemonic(&ins.op);
    match &ins.op {
        Op::PushU8(v) => format!("{m} {v}"),
        Op::Push(v) => format!("{m} {v}"),
        Op::PushN(v) => format!("{m} [{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")),
        Op::Native(n) => format!("{m} 0x{n:02x}  ; {}", name(*n)),
        Op::Set(n) | Op::Inc(n) | Op::PreInc(n) | Op::PostInc(n) | Op::Dec(n) | Op::PreDec(n) | Op::PostDec(n) => {
            format!("{m} 0x{n:02x}  ; {}", name(*n))
        }
        Op::Jmp(t) | Op::Jz(t) | Op::Loop(t) => format!("{m} L{t:04x}"),
        _ => m.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for v in [0, 1, -1, 63, -64, 64, 300, -300, 0x3fff_ffff, -0x3fff_ffff, 149, 1000000] {
            // the decoder accumulates in 32 bits: zig-zag values beyond ±2^30 cannot be expressed
        let e = encode_varint(v);
            assert_eq!(varint(&e, 0), Some((v, e.len())), "{v}");
        }
        // 'push 281' from script #0: 01 84 32
        assert_eq!(varint(&[0x84, 0x32], 0), Some((281, 2)));
    }

    #[test]
    fn branches_are_unsigned_and_relative_to_operand() {
        // jz +3 from offset 5: operand at 6, target 9
        let code = [0x00, 1, 0x00, 2, 0x07, 0x13, 0x00, 0x03, 0x14, 0x11];
        let i = decode_at(&code, 5).unwrap();
        assert_eq!(i.op, Op::Jz(9));
        assert_eq!(i.len, 3);
        // loop back 4 from operand at 3 → target -1 is an error; back 2 → 1
        let code = [0x11, 0x11, 0x1c, 0x00, 0x02, 0x11];
        assert_eq!(decode_at(&code, 2).unwrap().op, Op::Loop(1));
    }
}
