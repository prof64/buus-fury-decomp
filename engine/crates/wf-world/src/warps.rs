//! What a script / event does to the world, found from constant-argument native calls (names and argument meanings
//! from `engine/annotations/script_natives.json`, VERIFIED there):
//!
//! * `warp` 0x77 / `warp_fade` 0x76 `(world, area, variant, x, y)` — area transitions (the third argument goes to
//!   `0x08008D44` → `0x08008B30` → `0x080089A0` as the map variant; 0 = the area's own selector);
//! * `shop_add_item` 0xCD `(category 0 supply / 1 equipment, item, price, flag)`, `open_shop_buy` 0xCE,
//!   `open_shop_sell` 0xCF `(price_percent)` — shops;
//! * `give_item` 0x80 `(item)` — supply items handed out;
//!
//! followed transitively through the events / table scripts they start (`run_event` 0xB7, `run_script` 0xD8, goto
//! nodes; depth ≤ 4).

use serde_json::{json, Value};
use std::collections::BTreeSet;
use wf_rom::Rom;

use crate::bytecode::{self, NATIVE_RUN_EVENT, NATIVE_RUN_SCRIPT, NATIVE_WARP, NATIVE_WARP_FADE};
use crate::events::{Events, Payload};

pub const NATIVE_GIVE_ITEM: u8 = 0x80;
pub const NATIVE_SHOP_ADD: u8 = 0xCD;
pub const NATIVE_SHOP_BUY: u8 = 0xCE;
pub const NATIVE_SHOP_SELL: u8 = 0xCF;

const WANT: &[u8] = &[
    NATIVE_WARP,
    NATIVE_WARP_FADE,
    NATIVE_RUN_EVENT,
    NATIVE_RUN_SCRIPT,
    NATIVE_GIVE_ITEM,
    NATIVE_SHOP_ADD,
    NATIVE_SHOP_BUY,
    NATIVE_SHOP_SELL,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Src {
    Raw(u32),
    Event(u32),
}

#[derive(Default, Debug)]
pub struct Effects {
    pub warps: Vec<Value>,
    pub shop_items: Vec<Value>,
    pub shop_buy: bool,
    pub shop_sell: Option<Value>,
    pub gives: Vec<Value>,
}

pub struct Finder<'a> {
    rom: &'a Rom,
    ev: &'a Events,
}

impl<'a> Finder<'a> {
    pub fn new(rom: &'a Rom, ev: &'a Events) -> Finder<'a> {
        Finder { rom, ev }
    }

    fn calls(&self, s: Src) -> (Vec<bytecode::Call>, Vec<Src>) {
        let mut calls = Vec::new();
        let mut more = Vec::new();
        let mut code_units: Vec<Vec<u8>> = Vec::new();
        match s {
            Src::Raw(a) => code_units.push(bytecode::raw(self.rom, a)),
            Src::Event(h) => {
                if let Some(e) = self.ev.events.get(&h) {
                    for n in &e.nodes {
                        match n.payload {
                            Payload::Code { addr, compressed } => code_units.push(if compressed {
                                bytecode::inflate(self.rom, addr)
                            } else {
                                bytecode::raw(self.rom, addr)
                            }),
                            Payload::Goto(g) => more.push(Src::Event(g)),
                            _ => {}
                        }
                    }
                }
            }
        }
        for b in code_units {
            for c in bytecode::calls(&b, WANT) {
                match c.native {
                    NATIVE_RUN_EVENT | NATIVE_RUN_SCRIPT => {
                        if let Some(&h) = c.args[0].konst().and_then(|i| self.ev.table_order.get(i as usize)) {
                            more.push(Src::Event(h));
                        }
                    }
                    _ => calls.push(c),
                }
            }
        }
        (calls, more)
    }

    /// Everything reachable from `s`.
    pub fn effects(&self, s: Src) -> Effects {
        let mut fx = Effects::default();
        let mut seen = BTreeSet::new();
        let mut work = vec![(s, Vec::<String>::new())];
        while let Some((s, via)) = work.pop() {
            if !seen.insert(s) || via.len() > 4 {
                continue;
            }
            let (calls, more) = self.calls(s);
            for c in calls {
                let a: Vec<Value> = c.args.iter().map(|x| x.to_json()).collect();
                let k = |i: usize| c.args[i].konst();
                match c.native {
                    NATIVE_WARP | NATIVE_WARP_FADE => fx.warps.push(json!({
                        "to": match (k(0), k(1)) { (Some(w), Some(m)) => json!(format!("{w}-{m}")), _ => Value::Null },
                        "world": a[0], "area": a[1], "variant": a[2], "x": a[3], "y": a[4],
                        "fade": c.native == NATIVE_WARP_FADE,
                        "conditional": c.conditional || !via.is_empty(),
                        "via": via,
                    })),
                    NATIVE_SHOP_ADD => fx.shop_items.push(json!({
                        "category": match k(0) { Some(0) => json!("supply"), Some(_) => json!("equipment"), None => a[0].clone() },
                        "item": a[1], "price": a[2], "base_price": base_price(&c.args[2]), "flag": a[3],
                        "conditional": c.conditional || !via.is_empty(),
                    })),
                    NATIVE_SHOP_BUY => fx.shop_buy = true,
                    NATIVE_SHOP_SELL => fx.shop_sell = Some(a[0].clone()),
                    NATIVE_GIVE_ITEM => fx.gives.push(a[0].clone()),
                    _ => {}
                }
            }
            for m in more {
                let mut v = via.clone();
                if let Src::Event(h) = m {
                    v.push(self.ev.id(h));
                }
                work.push((m, v));
            }
        }
        fx
    }

    pub fn warps(&self, s: Src) -> Vec<Value> {
        self.effects(s).warps
    }

    /// Insert `warps`, `shop`, `gives_items` into an object.
    pub fn annotate(&self, m: &mut serde_json::Map<String, Value>, s: Src) {
        let fx = self.effects(s);
        if !fx.warps.is_empty() {
            m.insert("warps".into(), fx.warps.into());
        }
        if fx.shop_buy || fx.shop_sell.is_some() || !fx.shop_items.is_empty() {
            m.insert(
                "shop".into(),
                json!({"buy": fx.shop_buy, "sell_price_percent": fx.shop_sell, "items": fx.shop_items}),
            );
        }
        if !fx.gives.is_empty() {
            m.insert("gives_items".into(), fx.gives.into());
        }
    }
}

/// Shop prices are written `base * get_g0() / 100` (the shop's price percentage in G0): the constant factor.
fn base_price(s: &bytecode::Sym) -> Value {
    match s {
        bytecode::Sym::Const(v) => json!(v),
        bytecode::Sym::Expr(e) => {
            let t = e.trim_start_matches('(');
            let n: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            n.parse::<i64>().map(|v| json!(v)).unwrap_or(Value::Null)
        }
    }
}
