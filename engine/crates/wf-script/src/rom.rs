//! Scripts in the ROM: the event table and inline bytecode.
//!
//! **Events.** The table `0x0879B704` holds 522 pointers to *event headers*. A header is an array of u32
//! entries indexed by node number: a value `< 0x10000` is a terminal (the event finishes; the value is
//! stored to the caller's state word), anything else points to a node `{u16 next, u8 kind, u8 arg, payload}`.
//! Node kinds (handler table `0x080728AC`):
//!
//! | kind | payload |
//! |---|---|
//! | 0 | bytecode, raw at +3 |
//! | 1 | text box, inline UTF-16 at +4 (unused in Buu) |
//! | 2 | text box, UTF-16 at `[+4]`, printf args from bytecode at +8 (unused in Buu) |
//! | 3 | choice prompt, inline UTF-16 at +4 |
//! | 4 | bytecode, headerless Webfoot-LZ stream at +4 (inflated into a 0x800 buffer) |
//! | 5 | text box, packed text-bank reference at +4 |
//! | 6 | kind 5 + printf args from bytecode at +8 |
//! | 7 | goto: continue with the event header at +4, node 0 |
//!
//! After a node finishes the event continues at `next`, unless a bytecode node ends with exactly one value
//! on its stack: that value is the next node index.
//!
//! **Bytecode units** ([`Script`]): event bytecode nodes, the printf-argument bytecode of kind 2/6 nodes, and
//! inline raw bytecode referenced directly by engine records (map triggers, conditions, items, …).
//! `run_script(idx)` runs node 0 of event `idx` (which must be bytecode).

use crate::bytecode::{decode_reachable, DecodeError, Insn};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use wf_rom::Rom;

pub const EVENT_TABLE: u32 = 0x0879_B704;
pub const EVENT_COUNT: u16 = 522;
pub const TEXT_BANKS: u32 = 0x087B_5B64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ScriptId {
    /// Bytecode of node `node` of event `event` (kind 0/4).
    Node { event: u16, node: u16 },
    /// Printf-argument bytecode of a kind 2/6 text node.
    Args { event: u16, node: u16 },
    /// Raw code at this bus address, referenced directly by an engine record.
    Inline(u32),
}

impl ScriptId {
    pub fn slug(&self) -> String {
        match self {
            ScriptId::Node { event, node } => format!("s{event:03}_n{node}"),
            ScriptId::Args { event, node } => format!("s{event:03}_n{node}_args"),
            ScriptId::Inline(a) => format!("inline_0x{a:08X}"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Script {
    pub id: ScriptId,
    /// Bus address of `code[0]` for raw code; `None` when the code was decompressed.
    pub code_addr: Option<u32>,
    /// Address of the compressed stream, if compressed.
    pub stream_addr: Option<u32>,
    pub compressed: bool,
    /// Code bytes, trimmed to the end of the last reachable instruction.
    pub code: Vec<u8>,
    #[serde(skip)]
    pub insns: BTreeMap<usize, Insn>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum NodeBody {
    Code(ScriptId),
    /// Text box. `text` is decoded UTF-16 (control codes kept as `{xxxx}`), `ref_` the raw reference.
    Text { text: String, ref_: u32 },
    /// Text box whose `%d`/`%s` arguments are computed by bytecode.
    TextArgs { text: String, ref_: u32, args: ScriptId },
    /// Choice prompt; options are the lines starting with '-'.
    Choice { text: String },
    /// Continue at node 0 of another event (header address; `event` if it is in the table).
    Goto { header: u32, event: Option<u16> },
    Unknown { kind: u8 },
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub index: u16,
    pub addr: u32,
    pub next: u16,
    pub kind: u8,
    pub arg: u8,
    pub body: NodeBody,
}

#[derive(Clone, Debug, Serialize)]
pub enum Entry {
    Terminal(u16),
    Node(Node),
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub index: u16,
    pub header: u32,
    /// Entries by node index (only indices reachable statically, plus the dense range below the maximum).
    pub entries: BTreeMap<u16, Entry>,
    /// Some bytecode node computes its successor (targets not statically known).
    pub dynamic_branch: bool,
}

/// All events and bytecode units of the ROM.
#[derive(Clone, Debug, Default)]
pub struct ScriptSet {
    pub events: BTreeMap<u16, Event>,
    /// Header address → event index.
    pub headers: BTreeMap<u32, u16>,
    pub scripts: BTreeMap<ScriptId, Script>,
    pub errors: Vec<(ScriptId, String)>,
}

fn trim(code: &[u8]) -> Result<(Vec<u8>, BTreeMap<usize, Insn>), DecodeError> {
    let insns = decode_reachable(code)?;
    let end = insns.values().map(|i| i.off + i.len).max().unwrap_or(0);
    Ok((code[..end].to_vec(), insns))
}

/// Compressed script code: a bare Webfoot-LZ bit stream (no `type, size` container) inflated by the IWRAM
/// decoder (`0x08049120` → `0x03000040`). The stream starts with a literal-mode reset, so the container's
/// type word does not matter; we wrap it in a container header.
pub fn inflate(rom: &Rom, addr: u32) -> Result<Vec<u8>, String> {
    let start = Rom::off(addr);
    let end = (start + 0x4000).min(rom.len());
    let mut buf = Vec::with_capacity(8 + end - start);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&0x800u32.to_le_bytes());
    buf.extend_from_slice(&rom.data()[start..end]);
    wf_rom::lz::decompress(&buf, 0).map(|(d, _)| d).map_err(|e| e.to_string())
}

/// UTF-16 string at `a`, control characters kept as `{xxxx}`.
pub fn utf16_at(rom: &Rom, a: u32, max: usize) -> String {
    let mut v = Vec::new();
    for i in 0..max as u32 {
        let c = rom.u16(a + 2 * i);
        if c == 0 {
            break;
        }
        v.push(c);
    }
    decode_utf16(&v)
}

pub fn decode_utf16(v: &[u16]) -> String {
    let mut s = String::new();
    for &c in v {
        if c == 10 || (32..0xd800).contains(&c) && c != 0x7f {
            s.push(char::from_u32(c as u32).unwrap_or('?'));
        } else {
            s.push_str(&format!("{{{c:04x}}}"));
        }
    }
    s
}

/// Text banks (`0x087B5B64`): packed reference `v` → bank `v & 0x3F` (LZ container), offset
/// `((v >> 6) & 0x7FFF) * 2`, length `(v >> 21) * 2` bytes.
#[derive(Default, Clone, Debug)]
pub struct TextBanks {
    banks: BTreeMap<u32, Vec<u8>>,
}

impl TextBanks {
    pub fn get(&mut self, rom: &Rom, v: u32) -> String {
        let bank = v & 0x3f;
        let off = (((v >> 6) & 0x7fff) * 2) as usize;
        let len = ((v >> 21) * 2) as usize;
        let data = self.banks.entry(bank).or_insert_with(|| rom.lz(rom.u32(TEXT_BANKS + 4 * bank)).unwrap_or_default());
        let b = data.get(off..(off + len).min(data.len())).unwrap_or(&[]);
        let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        decode_utf16(&u)
    }
}

fn is_rom_ptr(v: u32) -> bool {
    (0x0800_0000..0x0880_0000).contains(&v)
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct InlineInfo {
    pub addr: String,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub r#where: Vec<String>,
}

/// The inline scripts found by the catalog (`engine/annotations/script_inline.json`).
pub fn inline_list() -> Vec<InlineInfo> {
    serde_json::from_str(include_str!("../../../annotations/script_inline.json")).unwrap_or_default()
}

pub fn inline_addrs() -> Vec<u32> {
    inline_list().iter().filter_map(|i| u32::from_str_radix(i.addr.trim_start_matches("0x"), 16).ok()).collect()
}

/// Enemy types (`0x08711274`, 150 × 28 bytes): `+0x18` is raw bytecode run when the enemy dies (the loot call,
/// typically `run_script(497, loot_level, forced_item)`; VERIFIED in the emulator trace `loot_drop.json`).
/// Returns (enemy type, code address).
pub fn enemy_drop_scripts(rom: &Rom) -> Vec<(u32, u32)> {
    (0..150u32)
        .filter_map(|t| {
            let c = rom.u32(0x0871_1274 + 28 * t + 0x18);
            is_rom_ptr(c).then_some((t, c))
        })
        .collect()
}

/// Load raw code at a bus address.
pub fn load_raw(rom: &Rom, id: ScriptId, addr: u32) -> Result<Script, String> {
    let start = Rom::off(addr);
    let end = (start + 0x8000).min(rom.len());
    let (code, insns) = trim(&rom.data()[start..end]).map_err(|e| e.to_string())?;
    Ok(Script { id, code_addr: Some(addr), stream_addr: None, compressed: false, code, insns })
}

pub fn load_compressed(rom: &Rom, id: ScriptId, addr: u32) -> Result<Script, String> {
    let d = inflate(rom, addr)?;
    let (code, insns) = trim(&d).map_err(|e| e.to_string())?;
    Ok(Script { id, code_addr: None, stream_addr: Some(addr), compressed: true, code, insns })
}

impl ScriptSet {
    /// Everything: all events, the catalogued inline scripts and the enemy drop scripts.
    pub fn load_all(rom: &Rom) -> ScriptSet {
        let mut s = ScriptSet::load(rom);
        let mut a = inline_addrs();
        a.extend(enemy_drop_scripts(rom).into_iter().map(|(_, c)| c));
        a.sort();
        a.dedup();
        s.add_inline(rom, &a);
        s
    }

    /// Load all 522 events (their nodes and bytecode) — inline scripts are added with [`ScriptSet::add_inline`].
    pub fn load(rom: &Rom) -> ScriptSet {
        let mut s = ScriptSet::default();
        for i in 0..EVENT_COUNT {
            s.headers.entry(rom.u32(EVENT_TABLE + 4 * i as u32)).or_insert(i);
        }
        let mut banks = TextBanks::default();
        for i in 0..EVENT_COUNT {
            let header = rom.u32(EVENT_TABLE + 4 * i as u32);
            let ev = s.load_event(rom, &mut banks, i, header);
            s.events.insert(i, ev);
        }
        s
    }

    fn add_script(&mut self, r: Result<Script, String>, id: ScriptId) -> bool {
        match r {
            Ok(sc) => {
                self.scripts.insert(id, sc);
                true
            }
            Err(e) => {
                self.errors.push((id, e));
                false
            }
        }
    }

    fn load_event(&mut self, rom: &Rom, banks: &mut TextBanks, index: u16, header: u32) -> Event {
        let mut entries = BTreeMap::new();
        let mut work = vec![0u16];
        let mut seen = BTreeSet::new();
        let mut dynamic = false;
        let mut max_seen = 0u16;
        let visit = |k: u16, work: &mut Vec<u16>| {
            work.push(k);
        };
        loop {
            while let Some(k) = work.pop() {
                if !seen.insert(k) || k > 2000 {
                    continue;
                }
                max_seen = max_seen.max(k);
                let v = rom.u32(header + 4 * k as u32);
                if v < 0x10000 {
                    entries.insert(k, Entry::Terminal(v as u16));
                    continue;
                }
                if !is_rom_ptr(v) {
                    continue;
                }
                let next = rom.u16(v);
                let kind = rom.u8(v + 2);
                let arg = rom.u8(v + 3);
                let mut succ = vec![next];
                let body = match kind {
                    0 | 4 => {
                        let id = ScriptId::Node { event: index, node: k };
                        let r = if kind == 0 { load_raw(rom, id, v + 3) } else { load_compressed(rom, id, v + 4) };
                        if self.add_script(r, id) {
                            let sc = &self.scripts[&id];
                            let (rets, dynr) = node_returns(sc);
                            dynamic |= dynr;
                            succ.extend(rets.into_iter().filter(|&r| (0..2000).contains(&r)).map(|r| r as u16));
                        }
                        NodeBody::Code(id)
                    }
                    1 | 3 => {
                        let text = utf16_at(rom, v + 4, 2000);
                        if kind == 1 {
                            NodeBody::Text { text, ref_: v + 4 }
                        } else {
                            NodeBody::Choice { text }
                        }
                    }
                    2 | 6 => {
                        let r = rom.u32(v + 4);
                        let text = if kind == 2 { utf16_at(rom, r, 2000) } else { banks.get(rom, r) };
                        let id = ScriptId::Args { event: index, node: k };
                        self.add_script(load_raw(rom, id, v + 8), id);
                        NodeBody::TextArgs { text, ref_: r, args: id }
                    }
                    5 => {
                        let r = rom.u32(v + 4);
                        NodeBody::Text { text: banks.get(rom, r), ref_: r }
                    }
                    7 => {
                        let h = rom.u32(v + 4);
                        NodeBody::Goto { header: h, event: self.headers.get(&h).copied() }
                    }
                    _ => NodeBody::Unknown { kind },
                };
                entries.insert(k, Entry::Node(Node { index: k, addr: v, next, kind, arg, body }));
                for s in succ {
                    visit(s, &mut work);
                }
            }
            // dynamic successors: also load the dense range below the highest index seen
            let missing: Vec<u16> = (0..max_seen).filter(|k| !seen.contains(k)).collect();
            if !dynamic || missing.is_empty() {
                break;
            }
            work.extend(missing);
        }
        Event { index, header, entries, dynamic_branch: dynamic }
    }

    /// Register inline scripts (code addresses from the catalog).
    pub fn add_inline(&mut self, rom: &Rom, addrs: &[u32]) {
        for &a in addrs {
            let id = ScriptId::Inline(a);
            let r = load_raw(rom, id, a);
            self.add_script(r, id);
        }
    }

    pub fn get(&self, id: ScriptId) -> Option<&Script> {
        self.scripts.get(&id)
    }

    /// The unit `run_script(idx)` executes: node 0 of event `idx` if it is bytecode.
    pub fn run_script_target(&self, idx: i32) -> Option<ScriptId> {
        let ev = self.events.get(&u16::try_from(idx).ok()?)?;
        match ev.entries.get(&0)? {
            Entry::Node(Node { body: NodeBody::Code(id), .. }) => Some(*id),
            _ => None,
        }
    }

    /// Build a set from raw code (tests, mods, assembled scripts).
    pub fn insert_code(&mut self, id: ScriptId, code: Vec<u8>) -> Result<(), String> {
        let (code, insns) = trim(&code).map_err(|e| e.to_string())?;
        self.scripts.insert(id, Script { id, code_addr: None, stream_addr: None, compressed: false, code, insns });
        Ok(())
    }
}

/// Constant values a bytecode node may leave as its single result (next-node candidates), and whether some
/// `end` leaves a computed single value.
pub fn node_returns(sc: &Script) -> (Vec<i32>, bool) {
    use crate::bytecode::Op;
    // walk each path tracking the stack abstractly: constants or unknown
    let mut rets = BTreeSet::new();
    let mut dynamic = false;
    let mut seen = BTreeSet::new();
    let mut work: Vec<(usize, Vec<Option<i32>>)> = vec![(0, vec![])];
    while let Some((o, mut st)) = work.pop() {
        if !seen.insert((o, st.clone())) || seen.len() > 20000 {
            continue;
        }
        let Some(ins) = sc.insns.get(&o) else { continue };
        let next = o + ins.len;
        let (pop, push) = crate::decompile::stack_effect(&ins.op);
        match &ins.op {
            Op::End => {
                if st.len() == 1 {
                    match st[0] {
                        Some(v) => {
                            rets.insert(v);
                        }
                        None => dynamic = true,
                    }
                }
                continue;
            }
            Op::PushU8(v) => st.push(Some(*v as i32)),
            Op::Push(v) => st.push(Some(*v)),
            Op::PushN(vs) => st.extend(vs.iter().map(|&v| Some(v))),
            _ => {
                for _ in 0..pop.min(st.len()) {
                    st.pop();
                }
                for _ in 0..push {
                    st.push(None);
                }
            }
        }
        match &ins.op {
            Op::Jmp(t) => work.push((*t, st)),
            Op::Jz(t) | Op::Loop(t) => {
                work.push((*t, st.clone()));
                work.push((next, st));
            }
            _ => work.push((next, st)),
        }
    }
    (rets.into_iter().collect(), dynamic)
}
