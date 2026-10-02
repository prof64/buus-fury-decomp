//! Buu's Fury world data straight from the ROM (no emulator): placed map objects (NPCs, enemy spawns, triggers,
//! exits, save points, breakables, item pickups), the area graph, and all dialogue text. See `engine/docs/world.md`.
//!
//! Output (paths relative to the content pack root):
//! * `maps/<map id>/objects.json` — per map: its props and, per area using the map, every placed object.
//! * `maps/<map id>/map.json` — gains a Tiled `objectgroup` layer (first area using the map) when wf-maps wrote it.
//! * `world/areas.json` — per area: key, name, maps, object counts, on-enter event / script, exits.
//! * `world/graph.json` — area graph: nodes = areas, edges = exit triggers.
//! * `text/dialog.json` — every text box / choice prompt of every event, with speaker; `text/banks.json` — the
//!   raw text banks (lines not reachable from an event included as `unreferenced` ranges).

pub mod bytecode;
pub mod events;
mod natives_gen;
pub mod objects;
pub mod text;
pub mod warps;

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use wf_rom::{content::Content, hx, Rom};

use events::{Events, Payload, TextSrc};

/// Area record address of area `i`.
fn area_addr(i: u32) -> u32 {
    wf_maps::rom_maps::AREA_TABLE + wf_maps::rom_maps::AREA_SIZE * i
}

pub struct World {
    pub banks: text::Banks,
    pub events: Events,
    pub areas: Vec<wf_maps::rom_maps::Area>,
    pub maps: Vec<wf_maps::MapInfo>,
}

impl World {
    pub fn load(rom: &Rom) -> World {
        let areas = wf_maps::rom_maps::areas(rom);
        let maps = wf_maps::maps(rom, &areas);
        let banks = text::Banks::load(rom);
        let addrs: Vec<u32> = areas.iter().map(|a| area_addr(a.index)).collect();
        let mut extra = objects::referenced_events(rom, &addrs);
        // area +0x30 on-enter code may be an event header? It is raw bytecode (run by 0x08004038); not an event.
        extra.sort();
        extra.dedup();
        let events = Events::load(rom, &banks, &extra);
        World { banks, events, areas, maps }
    }
}

/// Portrait names from an already extracted `portraits/index.json` (optional).
fn portrait_names(out: &Content) -> HashMap<u64, (String, String)> {
    let mut m = HashMap::new();
    if let Some(Value::Array(a)) = out.json("portraits/index.json") {
        for p in a {
            if let (Some(i), Some(id)) = (p["portrait_index"].as_u64(), p["id"].as_str()) {
                m.insert(i, (id.to_string(), p["name"].as_str().unwrap_or("").to_string()));
            }
        }
    }
    m
}

fn speaker_json(arg: u8, names: &HashMap<u64, (String, String)>) -> Value {
    let mut s = events::speaker(arg);
    if let Some(i) = s["portrait_index"].as_u64() {
        if let Some((id, name)) = names.get(&i) {
            s["portrait"] = json!(id);
            s["name"] = json!(name);
        }
    } else if s["kind"] == "player" {
        s["name"] = json!("(current player character)");
    }
    s
}

/// `text/dialog.json` + `text/banks.json`.
pub fn text_files(w: &World, out: &mut Content) {
    let names = portrait_names(out);
    let mut lines = Vec::new();
    // covered[bank] = sorted (offset, len)
    let mut covered: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut seen_nodes = BTreeSet::new();
    // events in table order first, then the others by address
    let mut order: Vec<u32> = w.events.table_order.clone();
    order.extend(w.events.events.keys().filter(|a| !w.events.table.contains_key(a)));
    for h in order {
        let Some(ev) = w.events.events.get(&h) else { continue };
        if !seen_nodes.insert(h) {
            continue;
        }
        let eid = w.events.id(h);
        for n in &ev.nodes {
            let Payload::Text { src, units } = &n.payload else { continue };
            let kind = match n.kind {
                1 => "inline",
                2 => "printf",
                3 => "choice",
                5 => "bank",
                6 => "bank_printf",
                _ => "text",
            };
            let mut m = Map::new();
            m.insert("id".into(), format!("{eid}.{}", n.index).into());
            m.insert("event".into(), eid.clone().into());
            m.insert("node".into(), n.index.into());
            m.insert("next".into(), n.next.into());
            m.insert("kind".into(), kind.into());
            m.insert("speaker".into(), speaker_json(n.arg, &names));
            m.insert("box".into(), text::box_position(units).into());
            m.insert("text".into(), text::render(units).into());
            m.insert("plain".into(), text::plain(units).into());
            if n.kind == 3 {
                let opts: Vec<String> = text::plain(units)
                    .lines()
                    .filter_map(|l| l.strip_prefix('-').map(|s| s.to_string()))
                    .collect();
                m.insert("options".into(), opts.into());
            }
            match src {
                TextSrc::Bank(v) => {
                    let (b, o, l) = text::unpack_ref(*v);
                    covered.entry(b).or_default().insert((o, l));
                    m.insert("ref".into(), json!({"bank": b, "offset": o, "length": l, "packed": hx(*v)}));
                }
                TextSrc::Inline(a) | TextSrc::Pointer(a) => {
                    m.insert("rom".into(), json!(hx(*a)));
                }
            }
            if matches!(n.kind, 2 | 6) {
                m.insert("args_script".into(), json!({"addr": hx(n.addr + 8), "note": "bytecode computing the printf arguments"}));
            }
            m.insert("node_rom".into(), hx(n.addr).into());
            lines.push(Value::Object(m));
        }
    }
    // unreferenced bank ranges
    let mut unref = Vec::new();
    let mut bank_json = Vec::new();
    let mut total = 0usize;
    let mut cov_total = 0usize;
    for (b, (addr, units)) in w.banks.banks.iter().enumerate() {
        let mut mark = vec![false; units.len()];
        if let Some(s) = covered.get(&(b as u32)) {
            for &(o, l) in s {
                for i in o as usize..((o + l) as usize).min(units.len()) {
                    mark[i] = true;
                }
            }
        }
        total += units.len();
        cov_total += mark.iter().filter(|&&x| x).count();
        let mut i = 0;
        while i < units.len() {
            if mark[i] {
                i += 1;
                continue;
            }
            let s = i;
            while i < units.len() && !mark[i] {
                i += 1;
            }
            unref.push(json!({
                "id": format!("bank{b}_{s}"),
                "ref": {"bank": b, "offset": s, "length": i - s},
                "text": text::render(&units[s..i]),
            }));
        }
        bank_json.push(json!({
            "bank": b,
            "rom": hx(*addr),
            "length": units.len(),
            "referenced": mark.iter().filter(|&&x| x).count(),
            "text": text::render(units),
        }));
    }
    out.put_json(
        "text/dialog.json",
        &json!({
            "format": {
                "text": "control codes: \\n new line, {page} new box, {color N} colour (7 highlight, 2 normal), {nbsp} non-breaking space, {xxxx} other UTF-16 code unit, {{ literal brace; %s / %d printf slots (kinds printf / bank_printf)",
                "box": "first character ! / @ / # puts the box at the top / middle / bottom (y 40 / 80 / 120) and is not drawn; default = bottom",
                "speaker": "node arg: 0/7 none (narration), 1..6 the current player character's portrait slot, >= 8 portrait arg-8 (portraits/index.json)",
                "kinds": "inline (node kind 1), printf (2), choice (3: lines starting with '-' are the options), bank (5), bank_printf (6)",
                "id": "<event id>.<node index>; event ids match the script catalog (sNNN = event table 0x0879B704 index)",
                "plain": "text without control codes / box prefix",
            },
            "counts": {"lines": lines.len(), "bank_units": total, "bank_units_referenced": cov_total, "unreferenced_ranges": unref.len()},
            "lines": lines,
            "unreferenced": unref,
        }),
    );
    out.put_json(
        "text/banks.json",
        &json!({
            "format": "text bank table 0x087B5B64: webfoot-LZ UTF-16LE; packed ref = bank | offset << 6 | length << 21 (UTF-16 units); text rendered like text/dialog.json",
            "banks": bank_json,
        }),
    );
}

/// Write `maps/*/objects.json`, Tiled object layers, `world/*.json` and the text files.
pub fn extract(rom: &Rom, out: &mut Content) -> wf_rom::Result<()> {
    let w = World::load(rom);
    text_files(&w, out);
    let strings = wf_rom::text::StringTable::load(rom);
    let map_by_hdr: HashMap<u32, &wf_maps::MapInfo> = w.maps.iter().map(|m| (m.header.addr, m)).collect();
    let finder = warps::Finder::new(rom, &w.events);
    let map_trig: HashMap<u32, Vec<Value>> =
        w.maps.iter().map(|m| (m.header.addr, objects::map_triggers(rom, &w.events, &finder, m.header.addr))).collect();

    // per area objects
    let mut area_objs: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    let mut areas_json = Vec::new();
    let mut edges = Vec::new();
    let mut per_map: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for a in &w.areas {
        let addr = area_addr(a.index);
        let objs = objects::area_objects(rom, &w.events, &finder, addr);
        for o in &objs {
            *counts.entry(o["type"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        }
        let map_ids: Vec<String> =
            a.headers.iter().map(|h| map_by_hdr.get(h).map(|m| m.id.clone()).unwrap_or_default()).collect();
        let name = if a.name_id != 0 { strings.get(a.name_id as usize).map(String::from) } else { None };
        let on_enter = rom.u32(addr + 0x30);
        let enter_warps = if rom.isptr(on_enter) { finder.warps(warps::Src::Raw(on_enter)) } else { Vec::new() };
        for w in enter_warps.iter().filter(|w| !w["to"].is_null()) {
            edges.push(json!({"from": a.key(), "to": w["to"], "kind": "on_enter_script", "dest": [w["x"], w["y"]],
                              "variant": w["variant"], "conditional": true, "via": w["via"]}));
        }
        for (vi, h) in a.headers.iter().enumerate() {
            for (i, o) in map_trig.get(h).into_iter().flatten().enumerate() {
                *counts.entry(format!("map_{}", o["type"].as_str().unwrap_or("?"))).or_default() += 1;
                if o["type"] == "exit" || o["type"] == "edge_exit" {
                    edges.push(json!({
                        "from": a.key(), "from_variant": vi + 1, "to": o["to"]["key"], "kind": format!("map_{}", o["type"].as_str().unwrap()), "map_trigger_index": i, "relative": o["to"]["relative"],
                        "rect": o["rect"], "push": o["push"], "dest": [o["to"]["x"], o["to"]["y"]],
                        "variant": o["to"]["variant"], "condition": o["condition"],
                    }));
                }
            }
        }
        for (i, o) in objs.iter().enumerate() {
            for w in o["warps"].as_array().into_iter().flatten().filter(|w| !w["to"].is_null()) {
                edges.push(json!({
                    "from": a.key(), "to": w["to"], "kind": format!("{}_warp", o["type"].as_str().unwrap_or("?")),
                    "object_index": i, "rect": o.get("rect"), "pos": o.get("pos"), "activation": o.get("activation"),
                    "dest": [w["x"], w["y"]], "variant": w["variant"], "fade": w["fade"],
                    "conditional": w["conditional"], "via": w["via"], "condition": o["condition"],
                }));
            }
            if o["type"] == "exit" || o["type"] == "edge_exit" {
                edges.push(json!({
                    "from": a.key(), "to": o["to"]["key"], "kind": o["type"], "object_index": i, "relative": o["to"]["relative"],
                    "rect": o["rect"], "push": o["push"], "dest": [o["to"]["x"], o["to"]["y"]],
                    "variant": o["to"]["variant"], "condition": o["condition"],
                }));
            }
        }
        let on_leave = rom.u32(addr + 0x34);
        let variant_sel = rom.u32(addr + 0x28);
        let area_entry = json!({
            "key": a.key(),
            "world": a.world,
            "area": a.area,
            "name": name,
            "maps": map_ids,
            "variant_selector": if rom.isptr(variant_sel) { json!({"script": events::inline_id(variant_sel)}) } else { json!(variant_sel) },
            "on_enter": if rom.isptr(on_enter) { json!({"script": events::inline_id(on_enter), "warps": enter_warps}) } else { Value::Null },
            "on_leave": if rom.isptr(on_leave) { json!({"script": events::inline_id(on_leave)}) } else { Value::Null },
            "objects": objs.len(),
            "rom": hx(addr),
        });
        for (k, mid) in area_entry["maps"].as_array().unwrap().iter().enumerate() {
            let Some(mid) = mid.as_str() else { continue };
            if mid.is_empty() {
                continue;
            }
            per_map.entry(mid.to_string()).or_default().push(json!({
                "key": a.key(), "name": area_entry["name"], "variant": k + 1, "objects": objs,
            }));
        }
        areas_json.push(area_entry);
        area_objs.insert(a.index, objs);
    }
    for m in &w.maps {
        let props = objects::props(rom, m.header.addr);
        let areas = per_map.remove(&m.id).unwrap_or_default();
        if let Some(Value::Object(mut mj)) = out.json(&format!("maps/{}/map.json", m.id)) {
            add_tiled_objects(&mut mj, &props, map_trig.get(&m.header.addr).map(|v| v.as_slice()).unwrap_or(&[]), areas.first());
            out.put_json(format!("maps/{}/map.json", m.id), &Value::Object(mj));
        }
        out.put_json(
            format!("maps/{}/objects.json", m.id),
            &json!({
                "map": m.id,
                "format": "engine/docs/world.md",
                "start_pos": [rom.s16(m.header.addr + 0x30), rom.s16(m.header.addr + 0x32)],
                "map_triggers": map_trig.get(&m.header.addr).cloned().unwrap_or_default(),
                "props": props,
                "areas": areas,
            }),
        );
    }
    let nodes: Vec<Value> =
        areas_json.iter().map(|a| json!({"key": a["key"], "name": a["name"], "maps": a["maps"]})).collect();
    out.put_json(
        "world/graph.json",
        &json!({
            "format": "nodes = areas (key world-area); edges = exit triggers (from area, rect the player walks into while pushing `push`, to area + destination). Script-driven warps (instant transmission, cutscenes) are not edges here.",
            "nodes": nodes,
            "edges": edges,
        }),
    );
    out.put_json("world/areas.json", &json!({"counts": counts, "areas": areas_json}));
    Ok(())
}

/// Append a Tiled `objectgroup` with the props and the objects of one area.
fn add_tiled_objects(mj: &mut Map<String, Value>, props: &[Value], map_trig: &[Value], area: Option<&Value>) {
    let mut objs = Vec::new();
    let mut id = mj.get("nextobjectid").and_then(|v| v.as_u64()).unwrap_or(1);
    let mut push = |name: String, typ: &str, x: i64, y: i64, w: i64, h: i64, point: bool, props: Value| {
        let mut o = json!({"id": id, "name": name, "type": typ, "x": x, "y": y, "width": w, "height": h,
                           "rotation": 0, "visible": true,
                           "properties": [{"name": "data", "type": "string", "value": props.to_string()}]});
        if point {
            o["point"] = json!(true);
        }
        objs.push(o);
        id += 1;
    };
    for p in props {
        let r = &p["rect"];
        push(format!("prop {}", p["index"]), "prop", r["x"].as_i64().unwrap_or(0), r["y"].as_i64().unwrap_or(0),
             r["w"].as_i64().unwrap_or(0), r["h"].as_i64().unwrap_or(0), false, p.clone());
    }
    for (i, o) in map_trig.iter().enumerate() {
        let r = &o["rect"];
        let typ = o["type"].as_str().unwrap_or("?");
        push(format!("map {typ} {i}"), typ, r["x"].as_i64().unwrap_or(0), r["y"].as_i64().unwrap_or(0),
             r["w"].as_i64().unwrap_or(0), r["h"].as_i64().unwrap_or(0), false, o.clone());
    }
    let key = area.and_then(|a| a["key"].as_str()).unwrap_or("").to_string();
    if let Some(a) = area {
        for (i, o) in a["objects"].as_array().into_iter().flatten().enumerate() {
            let typ = o["type"].as_str().unwrap_or("?");
            if let Some(r) = o.get("rect") {
                push(format!("{typ} {i}"), typ, r["x"].as_i64().unwrap_or(0), r["y"].as_i64().unwrap_or(0),
                     r["w"].as_i64().unwrap_or(0), r["h"].as_i64().unwrap_or(0), false, o.clone());
            } else if let Some(p) = o.get("pos") {
                push(format!("{typ} {i}"), typ, p[0].as_i64().unwrap_or(0), p[1].as_i64().unwrap_or(0), 0, 0, true, o.clone());
            }
        }
    }
    mj.insert("nextobjectid".into(), json!(id));
    let group = json!({
        "type": "objectgroup", "name": format!("objects {key}"), "id": mj.get("nextlayerid").cloned().unwrap_or(json!(99)),
        "visible": true, "opacity": 1, "x": 0, "y": 0, "draworder": "index",
        "properties": [{"name": "area", "type": "string", "value": key}],
        "objects": objs,
    });
    if let Some(n) = mj.get("nextlayerid").and_then(|v| v.as_u64()) {
        mj.insert("nextlayerid".into(), json!(n + 1));
    }
    if let Some(Value::Array(layers)) = mj.get_mut("layers") {
        layers.push(group);
    }
}
