//! Everything an area places on its map, decoded from the area record (`0x0808E2E0 + 0x38·i`) and the map header.
//! The area loader (`0x08008B30` → `0x08008AA0`, `0x08008AFE`, `0x08008C9E`) creates these objects on **every**
//! entry into the area, each one only if its condition holds at that moment (condition evaluator `0x08004784`).
//!
//! | list | count | entry | created by |
//! |---|---|---|---|
//! | +0x10 | +3 | 8 B `{u32 condition, ptr def}`; def +0 = ctor | `0x08046AA2(def, ctor)` — triggers, exits, save points |
//! | +0x14 | +4 | ptr def; def +0 ctor, +4 condition | `0x08046AA4(def, i, ctor)` — enemies, NPCs |
//! | +0x18 | +5 | ptr def; def +0 condition | `0x0800C7E2` — breakable map props |
//! | +0x1C | +6 | ptr def; def +0 condition | `0x0800DD2C` — item pickups |
//! | +0x20 | +7 | 8 B inline | `0x0800C300` — bobbing map sprites |
//! | map header +0x24 / +0x28 | | 0x18 B | map props (OBJ sprites drawn over the map; breakables refer to them by index) |

use serde_json::{json, Map, Value};
use wf_rom::{hx, Rom};

use crate::events::{inline_id, Events};
use crate::warps::{Finder, Src};

pub const CTOR_SCRIPT_TRIGGER: u32 = 0x0800_7387;
pub const CTOR_EVENT_TRIGGER: u32 = 0x0800_742D;
pub const CTOR_EXIT: u32 = 0x0800_7675;
pub const CTOR_EXIT_B: u32 = 0x0800_7575;
pub const CTOR_EDGE_EXIT: u32 = 0x0800_72CD;
pub const CTOR_REGION: u32 = 0x0802_4A55;
pub const CTOR_SAVE_POINT: u32 = 0x0800_7499;
pub const CTOR_MESSAGE_TRIGGER: u32 = 0x0800_76C5;
pub const CTOR_ENEMY: u32 = 0x0800_FD55;
pub const CTOR_NPC: u32 = 0x0801_11A9;
pub const CTOR_TALKER: u32 = 0x0801_7DA3;
pub const SAVE_POINT_EVENT: u32 = 0x0877_CE00;

/// Spawn / creation condition word (`0x08004784`).
pub fn condition(rom: &Rom, c: u32) -> Value {
    let hi = c >> 16;
    let lo = c & 0xFFFF;
    if c == 0 {
        json!({"always": true})
    } else if rom.isptr(c) {
        json!({"script": inline_id(c)})
    } else {
        match hi {
            0 => json!({"never": true, "value": c, "note": "only created by scripts"}),
            1 => json!({"flag_set": lo}),
            2 => json!({"flag_clear": lo}),
            3 => json!({"flag_set_consume": lo, "note": "if set: clear it and create"}),
            _ => json!({"raw": hx(c)}),
        }
    }
}

/// `{fn, data}` callback pair (`0x0800421D` none, `0x0800421F` run bytecode, `0x0800424F` run event).
pub fn callback(rom: &Rom, ev: &Events, f: u32, d: u32) -> Value {
    match f {
        0x0800_421D => Value::Null,
        0x0800_421F => json!({"script": inline_id(d)}),
        0x0800_424F => json!({"event": ev.id(d)}),
        _ => json!({"fn": hx(f), "data": hx(d), "note": if rom.isptr(f) { "unknown callback" } else { "not a callback" }}),
    }
}

/// Trigger activation mode: the def's +4 function returns a test object (`0x08009AEC`).
fn activation(f: u32) -> Value {
    match f {
        0x0800_77DF => json!({"mode": "touch_once", "note": "fires on overlap; the trigger is removed after it fires (until the area reloads)"}),
        0x0800_776D => json!({"mode": "touch"}),
        0x0800_770D => json!({"mode": "button", "button": "A"}),
        0x0800_774F => json!({"mode": "button", "button": "A", "facing": "up"}),
        0x0800_7753 => json!({"mode": "button", "button": "A", "facing": "down"}),
        0x0800_775F => json!({"mode": "button", "button": "A", "facing": "left"}),
        0x0800_7759 => json!({"mode": "button", "button": "A", "facing": "right"}),
        0x0803_4355 => json!({"mode": "touch_altitude", "z_min": 0, "z_max": 16}),
        0x0803_436F => json!({"mode": "touch_altitude", "z_min": 44, "z_max": 76}),
        _ => json!({"mode": "unknown", "fn": hx(f)}),
    }
}

const DIRS: [&str; 4] = ["down", "up", "left", "right"];

fn xy(rom: &Rom, a: u32) -> Value {
    json!([rom.s16(a), rom.s16(a + 2)])
}

fn rect(rom: &Rom, d: u32) -> Value {
    let (x0, y0, x1, y1) = (rom.s16(d + 8), rom.s16(d + 10), rom.s16(d + 12), rom.s16(d + 14));
    json!({"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0})
}

/// Movement / behaviour program of an actor: `u32 n` then `n` step pointers inline; the steps run in a loop
/// (`0x0800A95A`). Step record = `{create fn, params…}`.
fn steps(rom: &Rom, at: u32) -> Value {
    let n = rom.u32(at).min(32);
    let mut out = Vec::new();
    for k in 0..n {
        let s = rom.u32(at + 4 + 4 * k);
        if !rom.isptr(s) {
            out.push(json!({"op": "invalid", "ptr": hx(s)}));
            continue;
        }
        let f = rom.u32(s);
        let w = |i: u32| rom.u32(s + 4 * i);
        let (op, nw, extra): (&str, u32, Value) = match f {
            0x0800_A26F => ("stand", 1, json!({})),
            0x0800_A223 => ("wait", 2, json!({"frames": rom.s16(s + 4), "face": DIRS.get(rom.u8(s + 6) as usize).copied().unwrap_or("?")})),
            0x0800_A1AF => ("walk_to", 3, json!({"to": xy(rom, s + 4), "param": w(2)})),
            0x0800_A1D5 => ("walk_to_b", 3, json!({"to": xy(rom, s + 4), "param": w(2)})),
            0x0800_A75B => ("wander", 2, json!({"period": w(1)})),
            0x0800_A7DF => ("wander_b", 2, json!({"period": w(1)})),
            0x0800_A82D => ("wander_c", 2, json!({"period": w(1)})),
            0x0800_A8E9 => ("wander_d", 2, json!({"period": w(1)})),
            0x0800_A7A1 | 0x0800_A811 | 0x0800_A85F => (
                match f {
                    0x0800_A7A1 => "wander_rect",
                    0x0800_A811 => "wander_rect_b",
                    _ => "wander_rect_c",
                },
                4,
                json!({"period": w(1), "rect": {"x0": rom.u16(s + 8), "y0": rom.u16(s + 10), "x1": rom.u16(s + 12), "y1": rom.u16(s + 14)}}),
            ),
            0x0803_9F09 => ("cutscene_move", 3, json!({"to": xy(rom, s + 4), "param": hx(w(2))})),
            _ => ("unknown", 4, json!({})),
        };
        let mut o = Map::new();
        o.insert("op".into(), op.into());
        if let Value::Object(m) = extra {
            o.extend(m);
        }
        o.insert("rom".into(), json!({"step": hx(s), "fn": hx(f), "words": (1..nw).map(|i| hx(w(i))).collect::<Vec<_>>()}));
        out.push(Value::Object(o));
    }
    Value::Array(out)
}

/// A region trigger def (list 0 entry or a map-header exit): base `0x08009AEC` = +4 activation-mode getter,
/// +8 / +0xC rectangle corners (s16 x, y), +0x10.. payload by ctor.
pub fn trigger(rom: &Rom, ev: &Events, wf: &Finder, d: u32, cond: Option<u32>, list: Value) -> Value {
    let ctor = rom.u32(d);
    let mut o = Map::new();
    let typ = match ctor {
        CTOR_SCRIPT_TRIGGER => {
            o.insert("script".into(), inline_id(rom.u32(d + 0x10)).into());
            "script_trigger"
        }
        CTOR_EVENT_TRIGGER => {
            o.insert("event".into(), ev.id(rom.u32(d + 0x10)).into());
            "event_trigger"
        }
        CTOR_EXIT | CTOR_EXIT_B => {
            let b = rom.u8(d + 0x10);
            let face = b >> 4;
            o.insert(
                "to".into(),
                json!({
                    "world": rom.u8(d + 0x11), "area": rom.u8(d + 0x12),
                    "key": format!("{}-{}", rom.u8(d + 0x11), rom.u8(d + 0x12)),
                    "variant": rom.u8(d + 0x13), "x": rom.s16(d + 0x14), "y": rom.s16(d + 0x16),
                    "facing": if face == 0 { Value::Null } else { DIRS.get(face as usize - 1).copied().into() },
                }),
            );
            o.insert("push".into(), DIRS.get((b & 0xF) as usize).copied().unwrap_or("?").into());
            "exit"
        }
        CTOR_EDGE_EXIT => {
            // vtable 0x08049BC4, activate 0x080071F8: new position = player − (rect x0, y0) + (x, y): the player's
            // offset inside the strip is kept; facing = the pushed direction.
            let b = rom.u8(d + 0x10);
            o.insert(
                "to".into(),
                json!({
                    "world": rom.u8(d + 0x11), "area": rom.u8(d + 0x12),
                    "key": format!("{}-{}", rom.u8(d + 0x11), rom.u8(d + 0x12)),
                    "variant": rom.u8(d + 0x13), "x": rom.s16(d + 0x14), "y": rom.s16(d + 0x16),
                    "relative": true,
                    "facing": DIRS.get(b as usize).copied().unwrap_or("?"),
                }),
            );
            o.insert("push".into(), DIRS.get(b as usize).copied().unwrap_or("?").into());
            "edge_exit"
        }
        CTOR_SAVE_POINT => {
            o.insert("event".into(), ev.id(SAVE_POINT_EVENT).into());
            "save_point"
        }
        CTOR_MESSAGE_TRIGGER => {
            o.insert("message".into(), json!({"id": 0x17, "value": rom.u32(d + 0x10)}));
            "message_trigger"
        }
        CTOR_REGION => {
            o.insert("note".into(), "vtable 0x080499D4: enter / stay / leave state machine with a no-op activate; purpose not decoded".into());
            "region"
        }
        _ => {
            o.insert("vtable_ctor".into(), hx(ctor).into());
            o.insert("payload".into(), (0..4).map(|i| hx(rom.u32(d + 0x10 + 4 * i))).collect::<Vec<_>>().into());
            "trigger_unknown"
        }
    };
    let mut m = Map::new();
    m.insert("type".into(), typ.into());
    m.insert("list".into(), list);
    m.insert("condition".into(), match cond { Some(c) => condition(rom, c), None => json!({"always": true}) });
    m.insert("rect".into(), rect(rom, d));
    m.insert("activation".into(), activation(rom.u32(d + 4)));
    m.extend(o);
    let src = match ctor {
        CTOR_SCRIPT_TRIGGER => Some(Src::Raw(rom.u32(d + 0x10))),
        CTOR_EVENT_TRIGGER => Some(Src::Event(rom.u32(d + 0x10))),
        _ => None,
    };
    if let Some(src) = src {
        wf.annotate(&mut m, src);
    }
    m.insert("rom".into(), json!({"def": hx(d), "ctor": hx(ctor)}));
    Value::Object(m)
}

/// One area's placed objects (map props come from the map header, see [`props`]).
pub fn area_objects(rom: &Rom, ev: &Events, wf: &Finder, a: u32) -> Vec<Value> {
    let mut out = Vec::new();
    // list 0: triggers
    let (n0, p0) = (rom.u8(a + 3) as u32, rom.u32(a + 0x10));
    for k in 0..n0 {
        let cond = rom.u32(p0 + 8 * k);
        let d = rom.u32(p0 + 8 * k + 4);
        if rom.isptr(d) {
            out.push(trigger(rom, ev, wf, d, Some(cond), json!({"list": 0, "index": k})));
        }
    }
    // list 1: actors
    let (n1, p1) = (rom.u8(a + 4) as u32, rom.u32(a + 0x14));
    for k in 0..n1 {
        let d = rom.u32(p1 + 4 * k);
        if !rom.isptr(d) {
            continue;
        }
        let ctor = rom.u32(d);
        let mut m = Map::new();
        match ctor {
            CTOR_ENEMY => {
                m.insert("type".into(), "enemy".into());
                m.insert("enemy_type".into(), rom.u32(d + 8).into());
                m.insert("pos".into(), xy(rom, d + 0xC));
                m.insert("sprite_index".into(), rom.u32(d + 0x10).into());
                let cb = rom.u32(d + 0x14);
                m.insert(
                    "on_defeat".into(),
                    if rom.isptr(cb) { callback(rom, ev, rom.u32(cb), rom.u32(cb + 4)) } else { Value::Null },
                );
                m.insert("behaviour".into(), steps(rom, d + 0x18));
            }
            CTOR_NPC => {
                m.insert("type".into(), "npc".into());
                m.insert("pos".into(), xy(rom, d + 8));
                m.insert("sprite_index".into(), rom.u32(d + 0xC).into());
                m.insert("behaviour".into(), steps(rom, d + 0x10));
            }
            CTOR_TALKER => {
                m.insert("type".into(), "npc".into());
                m.insert("pos".into(), xy(rom, d + 8));
                m.insert("talk".into(), json!({"event": ev.id(rom.u32(d + 0xC)), "activation": "button A, facing the NPC"}));
                wf.annotate(&mut m, Src::Event(rom.u32(d + 0xC)));
                m.insert("sprite_index".into(), rom.u32(d + 0x10).into());
                m.insert("behaviour".into(), steps(rom, d + 0x14));
            }
            _ => {
                m.insert("type".into(), "actor_unknown".into());
                m.insert("raw".into(), (1..8).map(|i| hx(rom.u32(d + 4 * i))).collect::<Vec<_>>().into());
            }
        }
        m.insert("list".into(), json!({"list": 1, "index": k}));
        m.insert("condition".into(), condition(rom, rom.u32(d + 4)));
        m.insert("rom".into(), json!({"def": hx(d), "ctor": hx(ctor)}));
        out.push(Value::Object(m));
    }
    // list 2: breakable props
    let (n2, p2) = (rom.u8(a + 5) as u32, rom.u32(a + 0x18));
    for k in 0..n2 {
        let d = rom.u32(p2 + 4 * k);
        if !rom.isptr(d) {
            continue;
        }
        out.push(json!({
            "type": "breakable",
            "list": {"list": 2, "index": k},
            "condition": condition(rom, rom.u32(d)),
            "prop": rom.u8(d + 4),
            "param_a": rom.u8(d + 5),
            "hits": rom.u16(d + 6),
            "on_break": callback(rom, ev, rom.u32(d + 8), rom.u32(d + 12)),
            "note": "map prop `prop` that shakes when hit (0x0800C858) and breaks; on_break usually runs loot script s497",
            "rom": {"def": hx(d)},
        }));
    }
    // list 3: item pickups
    let (n3, p3) = (rom.u8(a + 6) as u32, rom.u32(a + 0x1C));
    for k in 0..n3 {
        let d = rom.u32(p3 + 4 * k);
        if !rom.isptr(d) {
            continue;
        }
        out.push(json!({
            "type": "item",
            "list": {"list": 3, "index": k},
            "condition": condition(rom, rom.u32(d)),
            "item": rom.u32(d + 4) as i32,
            "item_kind": if (rom.u32(d + 4) as i32) >= 0 { "supply" } else { "equipment" },
            "pos": [rom.u32(d + 8) as i32, rom.u32(d + 12) as i32],
            "on_pickup": callback(rom, ev, rom.u32(d + 0x10), rom.u32(d + 0x14)),
            "rom": {"def": hx(d)},
        }));
    }
    // list 4: bobbing map sprites
    let (n4, p4) = (rom.u8(a + 7) as u32, rom.u32(a + 0x20));
    for k in 0..n4 {
        let d = p4 + 8 * k;
        out.push(json!({
            "type": "map_sprite",
            "list": {"list": 4, "index": k},
            "pos": xy(rom, d),
            "sprite": rom.u8(d + 4),
            "params": [rom.u8(d + 5), rom.u8(d + 6), rom.u8(d + 7)],
            "note": "OBJ sprite resource bobbing on a sine table (0x087B6EEC); params not decoded",
            "rom": {"def": hx(d)},
        }));
    }
    out
}

/// Map props from the map header (+0x24 count, +0x28 → 0x18-byte records): rectangle, OAM attributes, tile.
pub fn props(rom: &Rom, hdr: u32) -> Vec<Value> {
    let n = rom.u32(hdr + 0x24).min(256);
    let p = rom.u32(hdr + 0x28);
    (0..n)
        .map(|k| {
            let r = p + 0x18 * k;
            let (x0, y0, x1, y1) = (rom.u32(r) as i32, rom.u32(r + 4) as i32, rom.u32(r + 8) as i32, rom.u32(r + 12) as i32);
            json!({
                "index": k,
                "rect": {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0},
                "oam_attr0_1": hx(rom.u32(r + 16)),
                "tile_word": hx(rom.u32(r + 20)),
                "rom": hx(r),
            })
        })
        .collect()
}

/// The map header's own triggers (+0x40 count, +0x44 → def pointers; created by the map loader `0x08009670` →
/// `0x08012664` on every load, unconditionally): the screen-edge and door exits.
pub fn map_triggers(rom: &Rom, ev: &Events, wf: &Finder, hdr: u32) -> Vec<Value> {
    let n = rom.u32(hdr + 0x40).min(64);
    let p = rom.u32(hdr + 0x44);
    if !rom.isptr(p) {
        return Vec::new();
    }
    (0..n)
        .filter_map(|k| {
            let d = rom.u32(p + 4 * k);
            rom.isptr(d).then(|| trigger(rom, ev, wf, d, None, json!({"map_header": k})))
        })
        .collect()
}

/// Every event / script referenced by area objects (for the event walker).
pub fn referenced_events(rom: &Rom, areas: &[u32]) -> Vec<u32> {
    let mut v = vec![SAVE_POINT_EVENT];
    for &a in areas {
        let (n0, p0) = (rom.u8(a + 3) as u32, rom.u32(a + 0x10));
        for k in 0..n0 {
            let d = rom.u32(p0 + 8 * k + 4);
            if rom.isptr(d) && rom.u32(d) == CTOR_EVENT_TRIGGER {
                v.push(rom.u32(d + 0x10));
            }
        }
        let (n1, p1) = (rom.u8(a + 4) as u32, rom.u32(a + 0x14));
        for k in 0..n1 {
            let d = rom.u32(p1 + 4 * k);
            if rom.isptr(d) && rom.u32(d) == CTOR_TALKER {
                v.push(rom.u32(d + 0xC));
            }
        }
    }
    v.retain(|&x| rom.isptr(x));
    v
}
