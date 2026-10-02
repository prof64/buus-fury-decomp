//! Events (the game's dialogue / cutscene sequences): table `0x0879B704` (522 headers) plus any header reached by
//! a goto node or a map object. Only the structure needed for text and object links is decoded here; the bytecode
//! itself belongs to the script VM (`engine/crates/wf-script`, `engine/annotations/script_catalog.json`, whose ids
//! these match: `sNNN` = table index, `inline_0xADDR` = raw bytecode).
//!
//! Header = `u32[]`: an entry `< 0x10000` ends the event with that value, else it points to a node
//! `{u16 next, u8 kind, u8 arg, payload}`. Node kinds (handler table `0x080728AC`):
//! 0 bytecode at +3, 1 text inline at +4, 2 printf text (format pointer at +4, argument bytecode at +8),
//! 3 choice prompt inline at +4, 4 compressed bytecode (headerless webfoot-LZ at +4), 5 text-bank line (ref at +4),
//! 6 text-bank line + printf arguments (bytecode at +8), 7 goto event header at +4.
//! For text nodes `arg` is the speaker portrait (`0x08012012`): 0 / 7 none, 1..6 the current player character's
//! portrait set (table `0x086B82EC`, per form, +0xF + arg), ≥ 8 portrait `arg − 8` of `0x086F395C`.

use std::collections::{BTreeMap, HashMap};
use wf_rom::Rom;

pub const EVENT_TABLE: u32 = 0x0879_B704;

#[derive(Clone, Debug)]
pub enum Payload {
    Code { addr: u32, compressed: bool },
    /// Text: source + UTF-16 code units.
    Text { src: TextSrc, units: Vec<u16> },
    Goto(u32),
    Unknown,
}

#[derive(Clone, Debug)]
pub enum TextSrc {
    Inline(u32),
    Pointer(u32),
    Bank(u32),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub index: u32,
    pub addr: u32,
    pub next: u16,
    pub kind: u8,
    pub arg: u8,
    pub payload: Payload,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub addr: u32,
    pub nodes: Vec<Node>,
    /// (entry index, value)
    pub terminals: Vec<(u32, u32)>,
}

pub struct Events {
    /// header address → table index
    pub table: HashMap<u32, u32>,
    pub table_order: Vec<u32>,
    pub events: BTreeMap<u32, Event>,
}

/// Content id of an event header (matches `script_catalog.json`).
pub fn event_id(table: &HashMap<u32, u32>, addr: u32) -> String {
    match table.get(&addr) {
        Some(i) => format!("s{i:03}"),
        None => format!("event_0x{addr:08X}"),
    }
}

/// Content id of a raw bytecode unit.
pub fn inline_id(addr: u32) -> String {
    format!("inline_0x{addr:08X}")
}

fn walk(rom: &Rom, banks: &crate::text::Banks, h: u32) -> Event {
    let mut ev = Event { addr: h, nodes: Vec::new(), terminals: Vec::new() };
    // sequential scan: entries are terminals or node pointers; the header ends at the first other word
    // (headers follow their nodes, so the next word is the next event's first node: u16 next | kind | arg,
    // never a ROM pointer), but never before the highest index reachable through `next`.
    let mut need = 0u32;
    let mut k = 0u32;
    while k < 512 {
        let v = rom.u32(h + 4 * k);
        if v < 0x10000 {
            ev.terminals.push((k, v));
            k += 1;
            continue;
        }
        if !rom.isptr(v) || rom.u8(v + 2) > 7 {
            if k >= need {
                break;
            }
            k += 1;
            continue;
        }
        let (next, kind, arg) = (rom.u16(v), rom.u8(v + 2), rom.u8(v + 3));
        need = need.max(next as u32);
        let payload = match kind {
            0 => Payload::Code { addr: v + 3, compressed: false },
            4 => Payload::Code { addr: v + 4, compressed: true },
            1 | 3 => Payload::Text { src: TextSrc::Inline(v + 4), units: crate::text::inline_units(rom, v + 4, 4000) },
            2 => {
                let p = rom.u32(v + 4);
                if rom.isptr(p) {
                    Payload::Text { src: TextSrc::Pointer(p), units: crate::text::inline_units(rom, p, 4000) }
                } else {
                    Payload::Unknown
                }
            }
            5 | 6 => {
                let r = rom.u32(v + 4);
                Payload::Text { src: TextSrc::Bank(r), units: banks.get(r).to_vec() }
            }
            7 => Payload::Goto(rom.u32(v + 4)),
            _ => Payload::Unknown,
        };
        ev.nodes.push(Node { index: k, addr: v, next, kind, arg, payload });
        k += 1;
    }
    ev
}

impl Events {
    /// Walk the event table, then every goto target and every `extra` header (map objects), transitively.
    pub fn load(rom: &Rom, banks: &crate::text::Banks, extra: &[u32]) -> Events {
        let mut table = HashMap::new();
        let mut table_order = Vec::new();
        let mut i = 0u32;
        loop {
            let h = rom.u32(EVENT_TABLE + 4 * i);
            if !rom.isptr(h) {
                break;
            }
            table.entry(h).or_insert(i);
            table_order.push(h);
            i += 1;
        }
        let mut events = BTreeMap::new();
        let mut work: Vec<u32> = table_order.iter().chain(extra.iter()).copied().collect();
        while let Some(h) = work.pop() {
            if events.contains_key(&h) || !rom.isptr(h) {
                continue;
            }
            let ev = walk(rom, banks, h);
            for n in &ev.nodes {
                if let Payload::Goto(g) = n.payload {
                    work.push(g);
                }
            }
            events.insert(h, ev);
        }
        Events { table, table_order, events }
    }
    pub fn id(&self, addr: u32) -> String {
        event_id(&self.table, addr)
    }
}

/// Speaker of a text node from its `arg`.
pub fn speaker(arg: u8) -> serde_json::Value {
    match arg {
        0 | 7 => serde_json::json!({"kind": "none"}),
        1..=6 => serde_json::json!({"kind": "player", "player_portrait_slot": arg}),
        _ => serde_json::json!({"kind": "portrait", "portrait_index": arg as u32 - 8}),
    }
}
