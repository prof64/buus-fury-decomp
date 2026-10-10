//! Native (builtin) table `0x08055098`: 248 handlers, each popping its arguments from the VM stack and
//! optionally pushing one result. Signatures and names come from `engine/annotations/script_natives.json`
//! (research annotation, no ROM data), compiled in.

use serde::Deserialize;
use std::sync::OnceLock;

pub const NATIVE_COUNT: usize = 248;
pub const SETTER_COUNT: usize = 18;
/// `run_script(idx)`: nested run sharing the caller's P block.
pub const RUN_SCRIPT: u8 = 0xd6;
/// `run_script(idx, p0)`: writes the caller's P0 first.
pub const RUN_SCRIPT_P0: u8 = 0xd7;
/// `run_script(idx, p0, p1)`: writes the caller's P0, P1 first.
pub const RUN_SCRIPT_P0_P1: u8 = 0xd8;
/// `run_event(idx)` .. `run_event(idx, p0, p1, p2, p3)` (0xB5..0xB9): write P0..Pk of the caller's block, run
/// the whole event with it, block until it ends.
pub const RUN_EVENT: u8 = 0xb5;
pub const RUN_EVENT_P0_P3: u8 = 0xb9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sig {
    pub pops: u8,
    pub pushes: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NativeInfo {
    pub id: u8,
    pub name: String,
    pub pops: u8,
    pub pushes: u8,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub blocking: Option<serde_json::Value>,
    #[serde(default)]
    pub semantics: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

impl NativeInfo {
    /// Does this native (possibly) span frames?
    pub fn may_block(&self) -> bool {
        match &self.blocking {
            Some(serde_json::Value::Bool(b)) => *b,
            Some(serde_json::Value::String(s)) => !s.is_empty() && s != "false",
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SetterInfo {
    pub id: u8,
    pub name: String,
    #[serde(default)]
    pub semantics: Option<String>,
}

static TABLE: OnceLock<Vec<NativeInfo>> = OnceLock::new();
static SETTERS: OnceLock<Vec<SetterInfo>> = OnceLock::new();

pub fn table() -> &'static [NativeInfo] {
    TABLE.get_or_init(|| {
        let mut v: Vec<NativeInfo> = serde_json::from_str(include_str!("../../../annotations/script_natives.json"))
            .expect("script_natives.json");
        v.sort_by_key(|e| e.id);
        assert_eq!(v.len(), NATIVE_COUNT);
        v
    })
}

pub fn setters() -> &'static [SetterInfo] {
    SETTERS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../annotations/script_setters.json")).unwrap_or_default()
    })
}

pub fn info(n: u8) -> Option<&'static NativeInfo> {
    table().get(n as usize)
}

/// Stack effect of native `n` (out-of-table ids read garbage on the GBA; here they are 0/0).
pub fn sig(n: u8) -> Sig {
    match n {
        0..=7 => Sig { pops: 0, pushes: true },
        RUN_SCRIPT => Sig { pops: 1, pushes: false },
        RUN_SCRIPT_P0 => Sig { pops: 2, pushes: false },
        RUN_SCRIPT_P0_P1 => Sig { pops: 3, pushes: false },
        _ => info(n).map(|i| Sig { pops: i.pops, pushes: i.pushes != 0 }).unwrap_or(Sig { pops: 0, pushes: false }),
    }
}

pub fn name(n: u8) -> String {
    match n {
        0..=3 => format!("G{n}"),
        4..=7 => format!("P{}", n - 4),
        _ => info(n).map(|i| i.name.clone()).unwrap_or_else(|| format!("native_{n:02x}")),
    }
}

/// Natives with a result whose evaluation changes nothing (reads). `rand`/`chance` advance the RNG and
/// menus run modally, so they are not pure. Skipping a pure query (short-circuit) is unobservable.
pub fn is_pure_query(n: u8) -> bool {
    let s = sig(n);
    s.pushes
        && !matches!(n, 0x19 | 0x1a)
        && info(n).map(|i| i.kind.as_deref() == Some("query") && !i.may_block()).unwrap_or(n < 8)
}

/// Name of the variable behind getter native `n` / setter `n` (n < 18).
pub fn var_name(n: u8) -> String {
    match n {
        0..=3 => format!("G{n}"),
        4..=7 => format!("P{}", n - 4),
        _ => setters()
            .iter()
            .find(|s| s.id == n)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("var_{n:02x}")),
    }
}
