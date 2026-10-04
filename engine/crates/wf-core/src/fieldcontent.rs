//! Static field content: the area table (`world/areas.json` + `areas.json`), every area's placed objects and every
//! map's header objects (`maps/<map>/objects.json`, crate wf-world, `engine/docs/world.md`), the maps' collision
//! (decoded on demand at area load: 429 maps x 128 KB would be 55 MB resident) and the supply table (`items/`).
//! Plain data behind `GameData::field`; nothing here is simulation state.

use crate::content::{int, rows_collision, unrle_grid, ContentSource, LoadError, MapCollision};
use crate::geom::{Dir, Rect};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// creation condition of a placed object (`0x08004784`)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Always,
    /// 1..0xFFFF: never created by the loader (only by `spawn_map_object`)
    Never(u32),
    FlagSet(i32),
    FlagClear(i32),
    /// 0x3xxxx: if the flag is set, clear it and create
    FlagSetConsume(i32),
    /// bytecode returning a bool (`inline_0xADDR`): decided by the script host
    Script(String),
}

/// `{fn, data}` callback pair: none / run bytecode (`inline_0xADDR`) / run event (`sNNN`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Callback {
    Script(String),
    Event(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Activation {
    Touch,
    TouchOnce,
    /// A pressed (optionally only while facing a direction)
    Button(Option<Dir>),
    /// touch at an altitude z (px above the ground) in (min, max)
    Altitude(i32, i32),
    /// an activation getter not decoded (fn address)
    Unknown(u32),
}

/// an area transition destination (exit def +0x11..+0x17)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Dest {
    pub world: i32,
    pub area: i32,
    /// map variant (1-based; 0 = the area's variant selector)
    pub variant: i32,
    pub x: i32,
    pub y: i32,
    /// arrival facing (None = keep)
    pub facing: Option<Dir>,
    /// edge exit: new = player - (rect x0, y0) + (x, y)
    pub relative: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    Script(String),
    Event(String),
    Exit { to: Dest, push: Dir },
    EdgeExit { to: Dest, push: Dir },
    SavePoint(String),
    Message { id: i32, value: i64 },
    Region,
    Unknown(u32),
}

/// one behaviour step (`{create fn, params}`, run in a loop by `0x0800A95A`); raw words kept
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub op: StepOp,
    /// create function address
    pub func: u32,
    /// the step record's words after the function pointer
    pub words: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StepOp {
    Stand,
    /// frames (s16), facing (u8, engine dir)
    Wait { frames: i32, face: u8 },
    /// variant 0 = `0x0800A1AF`, 1 = `0x0800A1D5`
    WalkTo { x: i32, y: i32, param: i32, variant: u8 },
    /// variant 0..3 = `0x0800A75B` / `A7DF` / `A82D` / `A8E9`
    Wander { period: i32, variant: u8 },
    /// variant 0..2 = `0x0800A7A1` / `A811` / `A85F`
    WanderRect { period: i32, rect: Rect, variant: u8 },
    CutsceneMove { x: i32, y: i32, param: u32 },
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjKind {
    Trigger { rect: Rect, act: Activation, kind: TriggerKind },
    Enemy { ty: i32, x: i32, y: i32, sprite: i32, on_defeat: Option<Callback>, behaviour: Vec<Step> },
    Npc { x: i32, y: i32, sprite: i32, talk: Option<String>, behaviour: Vec<Step> },
    Breakable { prop: i32, param_a: i32, hits: i32, on_break: Option<Callback> },
    /// item >= 0 supply, < 0 equipment (as the def stores it)
    Item { item: i32, x: i32, y: i32, on_pickup: Option<Callback> },
    /// a level sign: `label` = (frame of `effects/field_labels`, prop index it hangs on)
    MapSprite { x: i32, y: i32, sprite: i32, params: [i32; 3], label: Option<(i32, i32)> },
    Unknown,
}

/// where a placed object comes from: an area list (0 triggers, 1 actors, 2 breakables, 3 items, 4 map sprites) or the
/// map header's exits
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum ObjRef {
    Header(u16),
    List(u8, u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjDef {
    pub at: ObjRef,
    pub cond: Cond,
    pub kind: ObjKind,
    /// ROM def address (identity for traces)
    pub rom: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropDef {
    pub index: i32,
    pub rect: Rect,
    pub oam: u32,
    pub tile: u32,
    /// frame of `effects/props_<map>` (field_gfx), None = not exported
    pub frame: Option<i32>,
    /// shown at load (record +0x17; also the solid flag)
    pub visible: bool,
    /// OBJ priority
    pub priority: u8,
    /// the breakable hit / solid box: the graphic's opaque bounds, lower half (`hit_box`)
    pub hit_box: Option<[i32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Song {
    None,
    Id(i32),
    Script(String),
}

#[derive(Clone, Debug)]
pub struct AreaDef {
    pub world: i32,
    pub area: i32,
    pub key: String,
    pub name: Option<String>,
    /// map ids per variant (variant 1 first)
    pub maps: Vec<String>,
    pub variant_selector: i64,
    pub on_enter: Option<String>,
    pub on_leave: Option<String>,
    pub song: Song,
    /// area record +8: combat allowed (0 in towns: A only talks)
    pub combat: bool,
    /// the area's placed objects in list order (lists 0..4)
    pub objects: Vec<ObjDef>,
    pub rom: u32,
    /// the area-name banner: name string id (area table +0xC, 0 = none) and its text (maps `areas.json`)
    pub name_id: i64,
    pub banner: String,
}

#[derive(Clone, Debug)]
pub struct MapDef {
    pub id: String,
    pub start_pos: (i32, i32),
    /// the map header's exits (created unconditionally with the map)
    pub exits: Vec<ObjDef>,
    pub props: Vec<PropDef>,
    walk: String,
    shot: String,
    cw: usize,
    ch: usize,
    pub world_w: i32,
    pub world_h: i32,
    /// camera limits (map header +0x9C / +0x9E)
    pub camera_max: (i32, i32),
    /// the 3D world map (map header +0x3C: mode-7 ground layer kind 0x08007B63): flying player, 3D camera
    pub three_d: bool,
    /// the props object's object tiles (vram.rs; 0 = no props object)
    pub prop_tile_bytes: i32,
    /// CPU cycles of the map loader's decompression (`collision.json` reload; None = unknown): sndsim.rs
    pub reload: Option<Reload>,
    /// the map's BLDCNT (map header; `map.json` property `bldcnt`): its first-target bits decide whether a message
    /// window fades (scene.rs `window_fades`)
    pub bldcnt: u16,
}

/// see wf-maps `reload_json`
#[derive(Clone, Debug, Default)]
pub struct Reload {
    pub fixed: i64,
    /// the walk / shot grids' decompression (an area load only)
    pub grids: i64,
    /// tiles of the tile list (each copied to VRAM)
    pub tiles: i64,
    pub layers: Vec<ReloadLayer>,
}

#[derive(Clone, Debug, Default)]
pub struct ReloadLayer {
    pub cols: i32,
    pub rows: i32,
    pub rate: (i64, i64),
    pub offset: (i64, i64),
    pub ew: Vec<i64>,
    pub iw: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct Supply {
    pub index: i32,
    pub name: String,
    pub story: bool,
    pub price: i32,
    /// use script `inline_0xADDR`
    pub use_script: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FieldData {
    pub areas: BTreeMap<(i32, i32), AreaDef>,
    pub maps: BTreeMap<String, MapDef>,
    pub supplies: Vec<Supply>,
    /// dropped items' collision sub-box inside the 16x16 icon (`icons/index.json` sub_box): supplies, equipment
    pub supply_box: BTreeMap<i32, [i32; 4]>,
    pub equip_box: BTreeMap<i32, [i32; 4]>,
    /// the map-screen locations (`world/visited.json`): (world, area, bit) in table order (first match wins)
    pub visited: Vec<(i32, i32, i32)>,
    /// bytecode of the spawn-condition scripts (`scripts/bytecode.json`), evaluated by `story::eval_condition`
    pub cond_code: BTreeMap<String, Vec<u8>>,
    /// `show_title_text` strings' widths in px (`world/title_widths.json`)
    pub title_widths: Vec<i32>,
}

fn dir_of(s: &str) -> Option<Dir> {
    match s {
        "down" => Some(0),
        "up" => Some(1),
        "left" => Some(2),
        "right" => Some(3),
        _ => None,
    }
}

fn hexs(v: &Value) -> u32 {
    match v {
        Value::String(s) => u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0),
        _ => int(v) as u32,
    }
}

fn cond(v: &Value) -> Cond {
    if v.is_null() || v.get("always").is_some() {
        return Cond::Always;
    }
    if let Some(s) = v.get("script").and_then(|s| s.as_str()) {
        return Cond::Script(s.to_string());
    }
    if let Some(f) = v.get("flag_set") {
        return Cond::FlagSet(int(f) as i32);
    }
    if let Some(f) = v.get("flag_clear") {
        return Cond::FlagClear(int(f) as i32);
    }
    if let Some(f) = v.get("flag_set_consume") {
        return Cond::FlagSetConsume(int(f) as i32);
    }
    Cond::Never(v.get("value").map(|x| int(x) as u32).unwrap_or(1))
}

fn callback(v: &Value) -> Option<Callback> {
    if let Some(s) = v.get("script").and_then(|s| s.as_str()) {
        return Some(Callback::Script(s.to_string()));
    }
    v.get("event").and_then(|s| s.as_str()).map(|s| Callback::Event(s.to_string()))
}

fn rect(v: &Value) -> Rect {
    let (x, y) = (int(&v["x"]) as i32, int(&v["y"]) as i32);
    Rect::new(x, y, x + int(&v["w"]) as i32, y + int(&v["h"]) as i32)
}

fn activation(v: &Value) -> Activation {
    match v["mode"].as_str().unwrap_or("") {
        "touch" => Activation::Touch,
        "touch_once" => Activation::TouchOnce,
        "button" => Activation::Button(v["facing"].as_str().and_then(dir_of)),
        "touch_altitude" => Activation::Altitude(int(&v["z_min"]) as i32, int(&v["z_max"]) as i32),
        _ => Activation::Unknown(hexs(&v["fn"])),
    }
}

fn dest(v: &Value) -> Dest {
    Dest {
        world: int(&v["world"]) as i32,
        area: int(&v["area"]) as i32,
        variant: int(&v["variant"]) as i32,
        x: int(&v["x"]) as i32,
        y: int(&v["y"]) as i32,
        facing: v["facing"].as_str().and_then(dir_of),
        relative: v["relative"].as_bool().unwrap_or(false),
    }
}

fn steps(v: &Value) -> Vec<Step> {
    let mut out = Vec::new();
    for s in v.as_array().into_iter().flatten() {
        let rom = &s["rom"];
        let func = hexs(&rom["fn"]);
        let words: Vec<u32> = rom["words"].as_array().into_iter().flatten().map(hexs).collect();
        let w = |i: usize| words.get(i).copied().unwrap_or(0);
        let lo = |x: u32| (x & 0xFFFF) as i16 as i32;
        let hi = |x: u32| (x >> 16) as i16 as i32;
        let op = match func {
            0x0800_A26F => StepOp::Stand,
            0x0800_A223 => StepOp::Wait { frames: lo(w(0)), face: ((w(0) >> 16) & 0xFF) as u8 },
            0x0800_A1AF | 0x0800_A1D5 => StepOp::WalkTo { x: lo(w(0)), y: hi(w(0)), param: w(1) as i32, variant: (func == 0x0800_A1D5) as u8 },
            0x0800_A75B | 0x0800_A7DF | 0x0800_A82D | 0x0800_A8E9 => {
                StepOp::Wander { period: w(0) as i32, variant: [0x0800_A75B, 0x0800_A7DF, 0x0800_A82D, 0x0800_A8E9].iter().position(|&f| f == func).unwrap_or(0) as u8 }
            }
            0x0800_A7A1 | 0x0800_A811 | 0x0800_A85F => StepOp::WanderRect {
                period: w(0) as i32,
                rect: Rect::new((w(1) & 0xFFFF) as i32, (w(1) >> 16) as i32, (w(2) & 0xFFFF) as i32, (w(2) >> 16) as i32),
                variant: [0x0800_A7A1, 0x0800_A811, 0x0800_A85F].iter().position(|&f| f == func).unwrap_or(0) as u8,
            },
            0x0803_9F09 => StepOp::CutsceneMove { x: lo(w(0)), y: hi(w(0)), param: w(1) },
            _ => StepOp::Unknown,
        };
        out.push(Step { op, func, words });
    }
    out
}

fn xy(v: &Value) -> (i32, i32) {
    (int(&v[0]) as i32, int(&v[1]) as i32)
}

fn obj(v: &Value, at: ObjRef) -> ObjDef {
    let ty = v["type"].as_str().unwrap_or("");
    let kind = match ty {
        "script_trigger" | "event_trigger" | "exit" | "edge_exit" | "save_point" | "message_trigger" | "region" | "trigger_unknown" => {
            let kind = match ty {
                "script_trigger" => TriggerKind::Script(v["script"].as_str().unwrap_or("").to_string()),
                "event_trigger" => TriggerKind::Event(v["event"].as_str().unwrap_or("").to_string()),
                "exit" => TriggerKind::Exit { to: dest(&v["to"]), push: v["push"].as_str().and_then(dir_of).unwrap_or(0) },
                "edge_exit" => TriggerKind::EdgeExit { to: dest(&v["to"]), push: v["push"].as_str().and_then(dir_of).unwrap_or(0) },
                "save_point" => TriggerKind::SavePoint(v["event"].as_str().unwrap_or("s011").to_string()),
                "message_trigger" => TriggerKind::Message { id: int(&v["message"]["id"]) as i32, value: int(&v["message"]["value"]) },
                "region" => TriggerKind::Region,
                _ => TriggerKind::Unknown(hexs(&v["vtable_ctor"])),
            };
            ObjKind::Trigger { rect: rect(&v["rect"]), act: activation(&v["activation"]), kind }
        }
        "enemy" => {
            let (x, y) = xy(&v["pos"]);
            ObjKind::Enemy { ty: int(&v["enemy_type"]) as i32, x, y, sprite: int(&v["sprite_index"]) as i32, on_defeat: callback(&v["on_defeat"]), behaviour: steps(&v["behaviour"]) }
        }
        "npc" => {
            let (x, y) = xy(&v["pos"]);
            ObjKind::Npc { x, y, sprite: int(&v["sprite_index"]) as i32, talk: v["talk"]["event"].as_str().map(String::from), behaviour: steps(&v["behaviour"]) }
        }
        "breakable" => ObjKind::Breakable { prop: int(&v["prop"]) as i32, param_a: int(&v["param_a"]) as i32, hits: int(&v["hits"]) as i32, on_break: callback(&v["on_break"]) },
        "item" => {
            let (x, y) = xy(&v["pos"]);
            ObjKind::Item { item: int(&v["item"]) as i32, x, y, on_pickup: callback(&v["on_pickup"]) }
        }
        "map_sprite" => {
            let (x, y) = xy(&v["pos"]);
            let p = &v["params"];
            let label = v["label"].as_object().map(|l| (l.get("frame").map(int).unwrap_or(0) as i32, l.get("prop").map(int).unwrap_or(-1) as i32));
            ObjKind::MapSprite { x, y, sprite: int(&v["sprite"]) as i32, params: [int(&p[0]) as i32, int(&p[1]) as i32, int(&p[2]) as i32], label }
        }
        _ => ObjKind::Unknown,
    };
    ObjDef { at, cond: cond(&v["condition"]), kind, rom: hexs(&v["rom"]["def"]) }
}

fn list_ref(v: &Value) -> ObjRef {
    if let Some(h) = v.get("map_header") {
        return ObjRef::Header(int(h) as u16);
    }
    ObjRef::List(int(&v["list"]) as u8, int(&v["index"]) as u16)
}

/// the area list an object type lives in (wf-world: 0 triggers, 1 actors, 2 breakables, 3 items, 4 map sprites)
fn list_of_type(ty: &str) -> u8 {
    match ty {
        "enemy" | "npc" => 1,
        "breakable" => 2,
        "item" => 3,
        "map_sprite" => 4,
        _ => 0,
    }
}

/// parse objects appended to `have` (mod content may leave out `list`: the type's list, next free index)
fn objs_after(have: &[ObjDef], list: &[Value]) -> Vec<ObjDef> {
    let mut out: Vec<ObjDef> = Vec::new();
    for v in list {
        let at = if v.get("list").is_some_and(|l| l.is_object()) {
            list_ref(&v["list"])
        } else {
            let l = list_of_type(v["type"].as_str().unwrap_or(""));
            let n = have.iter().chain(out.iter()).filter(|o| matches!(o.at, ObjRef::List(k, _) if k == l)).count();
            ObjRef::List(l, n as u16)
        };
        out.push(obj(v, at));
    }
    out
}

fn read_json(src: &dyn ContentSource, path: &str) -> Option<Value> {
    src.read(path).and_then(|b| serde_json::from_slice(&b).ok())
}

/// a banner name id for a mod area's text (FNV-1a, above the game's string ids)
fn mod_name_id(text: &str) -> i64 {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    0x4000_0000 | (h & 0x3fff_ffff) as i64
}

impl FieldData {
    /// None when the pack has no world category (`world/areas.json`)
    pub fn load(src: &dyn ContentSource, all: &[String]) -> Result<Option<FieldData>, LoadError> {
        let Some(wa) = read_json(src, "world/areas.json") else { return Ok(None) };
        let mut fd = FieldData::default();
        fd.title_widths = read_json(src, "world/title_widths.json").and_then(|v| v.as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect())).unwrap_or_default();
        // songs from the maps category's areas.json
        let mut songs: BTreeMap<String, Song> = BTreeMap::new();
        let mut combat: BTreeMap<String, bool> = BTreeMap::new();
        let mut banners: BTreeMap<String, (i64, String)> = BTreeMap::new();
        if let Some(a) = read_json(src, "areas.json") {
            for x in a["areas"].as_array().into_iter().flatten() {
                banners.insert(x["key"].as_str().unwrap_or("").to_string(), (x["name_string_id"].as_i64().unwrap_or(0), x["name"].as_str().unwrap_or("").to_string()));
                let s = match &x["song"] {
                    Value::Null => Song::None,
                    Value::Object(o) => Song::Script(o.get("script").and_then(|s| s.as_str()).unwrap_or("").to_string()),
                    v => Song::Id(int(v) as i32),
                };
                songs.insert(x["key"].as_str().unwrap_or("").to_string(), s);
                combat.insert(x["key"].as_str().unwrap_or("").to_string(), x["rom"]["word08"].as_u64().is_none_or(|v| v & 0xFF != 0));
            }
        }
        for x in wa["areas"].as_array().into_iter().flatten() {
            let key = x["key"].as_str().unwrap_or("").to_string();
            let a = AreaDef {
                world: int(&x["world"]) as i32,
                area: int(&x["area"]) as i32,
                name: x["name"].as_str().map(String::from),
                maps: x["maps"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(String::from)).collect(),
                variant_selector: int(&x["variant_selector"]),
                on_enter: x["on_enter"]["script"].as_str().map(String::from),
                on_leave: x["on_leave"]["script"].as_str().map(String::from),
                song: songs.get(&key).cloned().unwrap_or(Song::None),
                combat: combat.get(&key).copied().unwrap_or(true),
                objects: Vec::new(),
                rom: hexs(&x["rom"]),
                name_id: banners.get(&key).map(|b| b.0).unwrap_or(0),
                banner: banners.get(&key).map(|b| b.1.clone()).unwrap_or_default(),
                key,
            };
            fd.areas.insert((a.world, a.area), a);
        }
        for p in all.iter().filter(|p| p.starts_with("maps/") && p.ends_with("/objects.json")) {
            let Some(o) = read_json(src, p) else { continue };
            let dir = &p[..p.len() - "/objects.json".len()];
            let id = o["map"].as_str().map(String::from).unwrap_or_else(|| dir["maps/".len()..].to_string());
            for a in o["areas"].as_array().into_iter().flatten() {
                let key = a["key"].as_str().unwrap_or("");
                let Some(ad) = fd.areas.values_mut().find(|x| x.key == key) else { continue };
                // the objects belong to the area (every map of a multi-variant area lists the same ones): first wins
                if ad.objects.is_empty() {
                    ad.objects = objs_after(&[], a["objects"].as_array().map(|v| v.as_slice()).unwrap_or(&[]));
                }
            }
            let exits = o["map_triggers"].as_array().into_iter().flatten().map(|v| obj(v, list_ref(&v["list"]))).collect();
            let prop_tile_bytes = o["prop_tile_bytes"].as_i64().unwrap_or(0) as i32;
            let props = o["props"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| PropDef {
                    index: int(&v["index"]) as i32,
                    rect: rect(&v["rect"]),
                    oam: hexs(&v["oam_attr0_1"]),
                    tile: hexs(&v["tile_word"]),
                    frame: v["gfx"]["frame"].as_i64().map(|f| f as i32),
                    visible: v["visible"].as_bool().unwrap_or(true),
                    priority: v["priority"].as_u64().unwrap_or(3) as u8,
                    hit_box: v["hit_box"].as_array().filter(|a| a.len() == 4).map(|a| [int(&a[0]) as i32, int(&a[1]) as i32, int(&a[2]) as i32, int(&a[3]) as i32]),
                })
                .collect();
            let (mut walk, mut shot, mut cw, mut ch) = (String::new(), String::new(), 1, 1);
            let mut reload = None;
            if let Some(c) = read_json(src, &format!("{dir}/collision.json")).map(rows_collision) {
                walk = c["walk"].as_str().unwrap_or("").to_string();
                shot = c["shot"].as_str().unwrap_or("").to_string();
                cw = int(&c["width"]).max(1) as usize;
                ch = int(&c["height"]).max(1) as usize;
                if c["reload"].is_object() {
                    let nums = |v: &serde_json::Value| -> Vec<i64> { v.as_array().into_iter().flatten().map(|x| x.as_i64().unwrap_or(0)).collect() };
                    let pair = |v: &serde_json::Value| (v[0].as_i64().unwrap_or(0), v[1].as_i64().unwrap_or(0));
                    reload = Some(Reload {
                        fixed: c["reload"]["fixed"].as_i64().unwrap_or(0),
                        grids: c["reload"]["grids"].as_i64().unwrap_or(0),
                        tiles: c["reload"]["tiles"].as_i64().unwrap_or(0),
                        layers: c["reload"]["layers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|l| ReloadLayer {
                                cols: l["cols"].as_i64().unwrap_or(0) as i32,
                                rows: l["rows"].as_i64().unwrap_or(0) as i32,
                                rate: pair(&l["rate"]),
                                offset: pair(&l["offset"]),
                                ew: nums(&l["ew"]),
                                iw: nums(&l["iw"]),
                            })
                            .collect(),
                    });
                }
            }
            let (mut ww, mut wh) = (0, 0);
            let mut cmax = (-1, -1);
            let mut three_d = false;
            let mut bldcnt = 0u16;
            if let Some(mj) = read_json(src, &format!("{dir}/map.json")) {
                for p in mj["properties"].as_array().into_iter().flatten() {
                    match p["name"].as_str() {
                        Some("world_w_px") => ww = int(&p["value"]) as i32,
                        Some("world_h_px") => wh = int(&p["value"]) as i32,
                        Some("camera_max_x") => cmax.0 = int(&p["value"]) as i32,
                        Some("camera_max_y") => cmax.1 = int(&p["value"]) as i32,
                        Some("bldcnt") => bldcnt = int(&p["value"]) as u16,
                        Some("unsupported_layers") => three_d = p["value"].as_str().is_some_and(|v| v.contains("0x08007B63")),
                        _ => {}
                    }
                }
                if ww == 0 {
                    ww = (int(&mj["width"]) * int(&mj["tilewidth"]).max(8)) as i32;
                    wh = (int(&mj["height"]) * int(&mj["tileheight"]).max(8)) as i32;
                }
            }
            // the 3D world map: its panorama layer (0x08007B63) listed as unsupported by older extracts, or the
            // mode-7 export the extractor writes for it now
            if !three_d && src.read(&format!("{dir}/mode7.json")).is_some() {
                three_d = true;
            }
            let sp = xy(&o["start_pos"]);
            if cmax.0 < 0 {
                cmax = ((ww - 240).max(0), (wh - 160).max(0));
            }
            fd.maps.insert(id.clone(), MapDef { id, start_pos: sp, exits, props, walk, shot, cw, ch, world_w: ww, world_h: wh, camera_max: cmax, three_d, prop_tile_bytes, reload, bldcnt });
        }
        // per-area files (mods): `world/areas/<W-A>.json` adds an area or changes one (fields given replace,
        // `objects` replaces the object list, `add_objects` appends to it)
        for p in all.iter().filter(|p| p.starts_with("world/areas/") && p.ends_with(".json")) {
            let Some(x) = read_json(src, p) else { return Err(LoadError(format!("{p}: not JSON"))) };
            let key = x["key"].as_str().map(String::from).unwrap_or_else(|| p["world/areas/".len()..p.len() - 5].to_string());
            let (w, ar) = key.split_once('-').and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?))).ok_or_else(|| LoadError(format!("{p}: area key must be \"world-area\"")))?;
            let fresh = !fd.areas.contains_key(&(w, ar));
            let a = fd.areas.entry((w, ar)).or_insert_with(|| AreaDef {
                world: w,
                area: ar,
                key: key.clone(),
                name: None,
                maps: Vec::new(),
                variant_selector: 0,
                on_enter: None,
                on_leave: None,
                song: Song::None,
                combat: true,
                objects: Vec::new(),
                rom: 0,
                name_id: 0,
                banner: String::new(),
            });
            let script = |v: &Value| v.as_str().map(String::from).or_else(|| v["script"].as_str().map(String::from));
            if let Some(n) = x["name"].as_str() {
                a.name = Some(n.to_string());
            }
            // the area-name banner: `"banner": "text"` (or a new area's `name`), `"banner": false` = none. Mod banners
            // get a name id from their text, so areas sharing a name show it once, like the game's string ids
            match x.get("banner") {
                Some(Value::String(t)) => {
                    a.banner = t.clone();
                    a.name_id = mod_name_id(t);
                }
                Some(Value::Bool(false)) => a.name_id = 0,
                _ => {
                    if let (true, Some(n)) = (fresh, x["name"].as_str()) {
                        a.banner = n.to_string();
                        a.name_id = mod_name_id(n);
                    }
                }
            }
            if let Some(m) = x["maps"].as_array() {
                a.maps = m.iter().filter_map(|m| m.as_str().map(String::from)).collect();
            }
            if x.get("on_enter").is_some() {
                a.on_enter = script(&x["on_enter"]);
            }
            if x.get("on_leave").is_some() {
                a.on_leave = script(&x["on_leave"]);
            }
            match x.get("song") {
                Some(Value::Null) => a.song = Song::None,
                Some(v @ Value::Number(_)) => a.song = Song::Id(int(v) as i32),
                _ => {}
            }
            if let Some(c) = x["combat"].as_bool() {
                a.combat = c;
            }
            if let Some(o) = x["objects"].as_array() {
                a.objects = objs_after(&[], o);
            }
            if let Some(o) = x["add_objects"].as_array() {
                let more = objs_after(&a.objects, o);
                a.objects.extend(more);
            }
        }
        for p in all.iter().filter(|p| p.starts_with("items/") && p.ends_with(".json") && !p.ends_with("index.json")) {
            let Some(it) = read_json(src, p) else { continue };
            if it.get("item").is_none() {
                continue;
            }
            fd.supplies.push(Supply {
                index: int(&it["item"]) as i32,
                name: it["name"].as_str().unwrap_or("").to_string(),
                story: it["story_item"].as_bool().unwrap_or(false),
                price: int(&it["price"]) as i32,
                use_script: it["use_script"].as_str().map(|s| format!("inline_0x{:08X}", hexs(&Value::String(s.to_string())))),
            });
        }
        fd.supplies.sort_by_key(|s| s.index);
        if let Some(Value::Array(icons)) = read_json(src, "icons/index.json") {
            for ic in icons {
                let Some(b) = ic["sub_box"].as_array().filter(|a| a.len() == 4) else { continue };
                let b = [int(&b[0]) as i32, int(&b[1]) as i32, int(&b[2]) as i32, int(&b[3]) as i32];
                let k = int(&ic["index"]) as i32;
                match ic["kind"].as_str() {
                    Some("supply") => {
                        fd.supply_box.insert(k, b);
                    }
                    Some("equipment") => {
                        fd.equip_box.insert(k, b);
                    }
                    _ => {}
                }
            }
        }
        if let Some(v) = read_json(src, "world/visited.json") {
            for r in v["records"].as_array().into_iter().flatten() {
                for a in r["areas"].as_array().into_iter().flatten() {
                    fd.visited.push((int(&a["world"]) as i32, int(&a["area"]) as i32, int(&a["bit"]) as i32));
                }
            }
        }
        let mut want: Vec<String> = Vec::new();
        for a in fd.areas.values() {
            for o in &a.objects {
                if let Cond::Script(id) = &o.cond {
                    want.push(id.clone());
                }
            }
            // an area song chosen by a selector script (Hercule City 3-1 .. 3-3: flag 26 and not 27 -> 29, else 6)
            if let Song::Script(s) = &a.song {
                want.push(if s.starts_with("inline_") { s.clone() } else { format!("inline_{s}") });
            }
        }
        if !want.is_empty() {
            if let Some(bc) = read_json(src, "scripts/bytecode.json") {
                for id in want {
                    if let Some(code) = bc["inline"][id.as_str()]["code"].as_str() {
                        let b: Vec<u8> = (0..code.len() / 2).filter_map(|k| u8::from_str_radix(&code[2 * k..2 * k + 2], 16).ok()).collect();
                        fd.cond_code.insert(id, b);
                    }
                }
            }
        }
        Ok(Some(fd))
    }

    pub fn area(&self, world: i32, area: i32) -> Option<&AreaDef> {
        self.areas.get(&(world, area))
    }
    /// the map of an area variant (1-based; out of range -> the first)
    pub fn map_of(&self, a: &AreaDef, variant: i32) -> Option<&MapDef> {
        let k = (variant.max(1) - 1) as usize;
        let id = a.maps.get(k).or_else(|| a.maps.first())?;
        self.maps.get(id)
    }
    /// decode a map's collision (walk + shot grids, world size)
    pub fn collision(&self, m: &MapDef) -> Arc<MapCollision> {
        Arc::new(MapCollision { walk: unrle_grid(&m.walk, m.cw, m.ch), shot: unrle_grid(&m.shot, m.cw, m.ch), world_w: m.world_w, world_h: m.world_h })
    }
}
