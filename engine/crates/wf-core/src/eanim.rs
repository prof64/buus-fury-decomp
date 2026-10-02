//! Enemy animation timing as the game runs it (tools/battle/ENEMY_ANIM.md; reference eanim.ts). Like the player, an
//! enemy shows animation `raw` at frame obj+0x11 and its state object advances the frame once per tick, before the AI.
//! Enemy SetAnim(id) takes the raw from 0x08072670 and the state from the enemy table 0x080726B8; attack / special nodes
//! use their own state objects whose frame sequences were recorded tick by tick (`seq` rules).
//!
//! Rules live in one table in the game data (`GameData::anim_rules`); an actor refers to its rule by index, so the
//! state stays plain data.

use crate::player::AnimPos;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleKind {
    Idle,
    Loop,
    Once,
    OnceStop,
    Hold,
    Keep,
    Bored,
    Reverse,
    Seq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum End {
    Idle,
    Hold,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub at: i32,
    pub id: i32,
    pub vol: i32,
    pub rand: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EAnimRule {
    pub kind: RuleKind,
    pub shift: i32,
    pub end: Option<End>,
    pub swing: bool,
    pub fixed: Option<bool>,
    pub seq: Vec<(i32, i32, i32)>,
    pub sfx: Vec<Cue>,
}

impl EAnimRule {
    pub fn simple(kind: RuleKind, shift: i32, end: Option<End>, swing: bool) -> EAnimRule {
        EAnimRule { kind, shift, end, swing, fixed: None, seq: vec![], sfx: vec![] }
    }
    pub fn from_json(v: &Value) -> EAnimRule {
        let kind = match v["rule"].as_str().unwrap_or("") {
            "idle" => RuleKind::Idle,
            "loop" => RuleKind::Loop,
            "once" => RuleKind::Once,
            "once_stop" => RuleKind::OnceStop,
            "hold" => RuleKind::Hold,
            "bored" => RuleKind::Bored,
            "reverse" => RuleKind::Reverse,
            "seq" => RuleKind::Seq,
            _ => RuleKind::Keep,
        };
        let end = v["end"].as_str().map(|e| match e {
            "idle" => End::Idle,
            "hold" => End::Hold,
            _ => End::None,
        });
        let seq = v["seq"].as_array().map(|a| a.iter().map(|s| (s[0].as_i64().unwrap_or(0) as i32, s[1].as_i64().unwrap_or(0) as i32, s[2].as_i64().unwrap_or(0) as i32)).collect()).unwrap_or_default();
        let sfx = v["sfx"]
            .as_array()
            .map(|a| a.iter().map(|c| Cue { at: c["at"].as_i64().unwrap_or(0) as i32, id: c["id"].as_i64().unwrap_or(0) as i32, vol: c["vol"].as_i64().unwrap_or(64) as i32, rand: c["rand"].as_u64().unwrap_or(0) as u32 }).collect())
            .unwrap_or_default();
        EAnimRule { kind, shift: v["shift"].as_i64().unwrap_or(3) as i32, end, swing: v["swing"].as_bool().unwrap_or(false), fixed: v["fixed"].as_bool(), seq, sfx }
    }
}

/// enemy SetAnim id -> raw (0x08072670 + 1) and rule (0x080726B8)
pub fn setanim_table() -> Vec<(i32, EAnimRule)> {
    use RuleKind::*;
    let s = EAnimRule::simple;
    vec![
        (1, s(Idle, 3, None, false)),
        (2, s(Loop, 3, None, false)),
        (3, s(Loop, 3, None, false)),
        (4, s(Once, 3, Some(End::Idle), false)),
        (5, s(Once, 2, Some(End::Idle), true)),
        (6, s(Once, 2, Some(End::Idle), true)),
        (7, s(Once, 2, Some(End::Idle), true)),
        (8, s(Hold, 3, None, false)),
        (9, s(Bored, 3, None, false)),
        (10, s(Once, 3, Some(End::Idle), false)),
        (11, s(OnceStop, 3, Some(End::None), false)),
        (12, s(OnceStop, 3, Some(End::None), false)),
        (13, s(Once, 4, Some(End::Idle), false)),
        (14, s(Loop, 3, None, false)),
        (15, s(Loop, 3, None, false)),
        (16, s(Loop, 3, None, false)),
        (18, s(Once, 5, Some(End::Hold), false)),
        (19, s(Once, 4, Some(End::Idle), false)),
        (20, s(Once, 4, Some(End::Idle), false)),
        (21, s(Once, 4, Some(End::Idle), false)),
        (22, s(Once, 4, Some(End::Idle), false)),
        (23, s(Once, 4, Some(End::Idle), false)),
        (24, s(Once, 4, Some(End::Idle), false)),
    ]
}

/// the shared state objects (IWRAM singletons) by address
pub fn generic_states() -> Vec<(&'static str, EAnimRule)> {
    use RuleKind::*;
    let s = EAnimRule::simple;
    vec![
        ("0x03001B0C", s(Idle, 3, None, false)),
        ("0x03002B44", s(Idle, 3, None, false)),
        ("0x03001B58", s(Loop, 3, None, false)),
        ("0x03001B48", s(Loop, 3, None, false)),
        ("0x03001B10", s(Once, 3, Some(End::Idle), false)),
        ("0x03001B18", s(Once, 4, Some(End::Idle), false)),
        ("0x03001B50", s(Once, 2, Some(End::Idle), true)),
        ("0x03001B30", s(OnceStop, 3, Some(End::None), false)),
        ("0x03001B40", s(Once, 5, Some(End::Hold), false)),
        ("0x03001B04", s(Hold, 3, None, false)),
        ("0x03001B08", s(Bored, 3, None, false)),
        ("0x03002B9C", s(Reverse, 3, None, false)),
        ("0x03003100", s(Reverse, 3, None, false)),
    ]
}
pub const KEEP_VTABLES: [&str; 2] = ["0x0804985C", "0x0804A214"];

/// all rules of the game data: fixed indices for the builtins, then the per-type recorded ones
#[derive(Clone)]
pub struct AnimRules {
    pub rules: Vec<EAnimRule>,
    /// SetAnim table by raw
    pub setanim: BTreeMap<i32, u32>,
    /// generic states by object address
    pub generic: BTreeMap<String, u32>,
    /// per enemy type: key (raw "5" or controller "vt@obj") -> rule
    pub types: BTreeMap<i32, BTreeMap<String, u32>>,
    pub keep: u32,
    pub reverse: u32,
    pub fallback_loop: u32,
    pub fallback_once: u32,
    pub idle: u32,
}

impl AnimRules {
    pub fn load(sim_json: &str) -> AnimRules {
        let mut rules = Vec::new();
        let push = |r: EAnimRule, rules: &mut Vec<EAnimRule>| {
            rules.push(r);
            (rules.len() - 1) as u32
        };
        let mut setanim = BTreeMap::new();
        for (raw, r) in setanim_table() {
            let i = push(r, &mut rules);
            setanim.insert(raw, i);
        }
        let mut generic = BTreeMap::new();
        for (k, r) in generic_states() {
            let i = push(r, &mut rules);
            generic.insert(k.to_string(), i);
        }
        let keep = push(EAnimRule::simple(RuleKind::Keep, 3, None, false), &mut rules);
        let reverse = push(EAnimRule::simple(RuleKind::Reverse, 3, None, false), &mut rules);
        let fallback_loop = push(EAnimRule::simple(RuleKind::Loop, 3, Some(End::None), false), &mut rules);
        let fallback_once = push(EAnimRule::simple(RuleKind::OnceStop, 3, Some(End::None), false), &mut rules);
        let idle = setanim[&1];
        let mut types = BTreeMap::new();
        if let Ok(v) = serde_json::from_str::<Value>(sim_json) {
            if let Some(o) = v["types"].as_object() {
                for (tk, rs) in o {
                    let ty = i32::from_str_radix(tk.trim_start_matches("0x"), 16).unwrap_or(-1);
                    let mut m = BTreeMap::new();
                    for (k, r) in rs.as_object().into_iter().flatten() {
                        let i = push(EAnimRule::from_json(r), &mut rules);
                        m.insert(k.clone(), i);
                    }
                    types.insert(ty, m);
                }
            }
        }
        AnimRules { rules, setanim, generic, types, keep, reverse, fallback_loop, fallback_once, idle }
    }
    /// the rule of a SetState(controller) for a type (tests): shared object, else the type's recorded rule
    pub fn for_controller(&self, ctl: &str, ty: i32) -> Option<u32> {
        let (vt, st) = ctl.split_once('@').unwrap_or((ctl, ""));
        if KEEP_VTABLES.contains(&vt) {
            return Some(self.keep);
        }
        if let Some(&i) = self.generic.get(st) {
            return Some(i);
        }
        self.types.get(&ty).and_then(|m| m.get(ctl)).copied()
    }
}

/// the actor's animation fields (obj+0x50 raw, +0x11 frame, +0x12 tick) and its state rule
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnemyAnim {
    pub pos: AnimPos,
    pub rule: u32,
    /// a one-shot finished (end callback 'none' / 'hold', or the sequence ran out)
    pub done: bool,
    /// ticks since the state was entered
    pub t: i32,
}

pub enum Tick {
    Idle,
    None,
}

/// host callbacks of the model: frames of a raw (0 = missing), the game RNG, sounds
pub trait Host {
    fn nframes(&mut self, raw: i32) -> i32;
    fn rand(&mut self, n: u32) -> i32;
    fn sfx(&mut self, id: i32, vol: i32);
}

impl EnemyAnim {
    /// SetState with rule `r` showing `raw` (enter: tick = frame = 0; idle: tick = rand(255); punch: swing sound)
    pub fn enter(&mut self, rules: &[EAnimRule], raw: i32, rule: u32, h: &mut dyn Host) {
        self.pos.raw = raw;
        self.rule = rule;
        self.done = false;
        self.t = 0;
        let r = &rules[rule as usize];
        if r.kind == RuleKind::Keep {
            return;
        }
        self.pos.frame = 0;
        self.pos.tick = 0;
        if r.kind == RuleKind::Idle {
            self.pos.tick = h.rand(255);
        }
        if r.swing {
            let s = 50 + h.rand(2);
            h.sfx(s, 64);
        }
        if r.kind == RuleKind::Seq {
            if let Some(&(raw0, f0, _)) = r.seq.first() {
                self.pos.raw = raw0;
                self.pos.frame = f0;
            }
        }
        self.cues(rules, 0, h);
    }
    fn cues(&self, rules: &[EAnimRule], t: i32, h: &mut dyn Host) {
        for c in &rules[self.rule as usize].sfx {
            if c.at == t {
                let id = c.id + if c.rand > 0 { h.rand(c.rand) } else { 0 };
                h.sfx(id, c.vol);
            }
        }
    }
    /// one tick of the state (vtable +4); Idle = the end callback asks for SetAnim(0)
    pub fn tick(&mut self, rules: &[EAnimRule], h: &mut dyn Host) -> Tick {
        let r = &rules[self.rule as usize];
        let n = h.nframes(self.pos.raw);
        self.t += 1;
        let a = &mut self.pos;
        match r.kind {
            RuleKind::Idle => {
                a.tick = (a.tick + 1) & 0xffff;
                if n == 2 {
                    a.frame = if (a.tick & 0xff) > 0xf0 { 1 } else { 0 };
                } else {
                    a.frame = (a.tick >> 3) & 0xff;
                    if a.frame >= n {
                        a.frame = 0;
                        a.tick = 0;
                    }
                }
                Tick::None
            }
            RuleKind::Loop => {
                a.tick = (a.tick + 1) & 0xffff;
                let mut f = a.tick >> r.shift;
                if f >= n {
                    a.tick = 0;
                    f = 0;
                }
                a.frame = f;
                Tick::None
            }
            RuleKind::Bored => {
                a.tick = (a.tick + 1) & 0xffff;
                let f = a.tick >> 5;
                if f >= n {
                    a.tick = 0x40;
                    a.frame = 2;
                } else {
                    a.frame = f;
                }
                Tick::None
            }
            RuleKind::Reverse => {
                let mut t = a.tick - 1;
                if t < 0 {
                    t = n * 8 - 1;
                }
                a.tick = t & 0xffff;
                a.frame = (t >> 3) & 0xff;
                Tick::None
            }
            RuleKind::Hold | RuleKind::Keep => Tick::None,
            RuleKind::Once | RuleKind::OnceStop => {
                if self.done {
                    a.tick = (a.tick + 1) & 0xffff;
                    return Tick::None;
                }
                a.tick = (a.tick + 1) & 0xffff;
                let f = a.tick >> r.shift;
                if f >= n {
                    if r.end == Some(End::Idle) {
                        return Tick::Idle;
                    }
                    self.done = true;
                } else {
                    a.frame = f;
                }
                Tick::None
            }
            RuleKind::Seq => {
                let t = self.t;
                self.cues(rules, t, h);
                let r = &rules[self.rule as usize];
                let mut k = t;
                for &(raw, frame, cnt) in &r.seq {
                    if k < cnt {
                        self.pos.raw = raw;
                        self.pos.frame = frame;
                        return Tick::None;
                    }
                    k -= cnt;
                }
                self.done = true;
                if !r.seq.is_empty() && r.end == Some(End::Idle) && r.fixed != Some(false) {
                    Tick::Idle
                } else {
                    Tick::None
                }
            }
        }
    }
}
