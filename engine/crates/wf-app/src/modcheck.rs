//! The mod checker (engine/docs/modding.md §7): loads `<content/base>` + mod folders exactly like the game, lists
//! what each mod adds / replaces / patches / derives, and checks everything the engine will read - every mod file
//! parses, patches / `$extends` / derives find what they point at, ids do not clash with the base game or another
//! mod, fighters -> characters / forms -> sprites + sheets, skills -> behaviour + HUD icons, areas -> maps (Tiled +
//! tileset + collision) -> talk / trigger / defeat scripts, every Rhai file compiles and calls only functions that
//! exist. Each problem names the file, what is wrong and a hint. Headless, no window, no audio.
//!
//! Front ends: `buus-fury --modcheck <mod dir>...` (the installed base content), `cargo run -p wf-app --example
//! modcheck -- <content/base> <mod dir>...`. Exit code 1 on errors.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use wf_content::{map, sprite, Packs};
use wf_core::fieldcontent::{Callback, ObjKind, TriggerKind};

/// One problem: where, what, and what to do about it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Issue {
    /// a mod-relative file (`<mod id>/<path>`) or the content record it concerns
    pub file: String,
    pub what: String,
    pub hint: String,
}

#[derive(Default)]
pub struct Report {
    /// what each mod touches and the summary lines
    pub lines: Vec<String>,
    /// sorted, without duplicates
    pub errors: Vec<Issue>,
}

impl Report {
    fn err(&mut self, file: impl Into<String>, what: impl Into<String>, hint: impl Into<String>) {
        let i = Issue { file: file.into(), what: what.into(), hint: hint.into() };
        if !self.errors.contains(&i) {
            self.errors.push(i);
        }
    }
    fn line(&mut self, s: impl Into<String>) {
        self.lines.push(s.into());
    }
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
    /// The whole report as text (the lines, then `ERROR file: what` + `  hint: ...` per problem, then OK / the
    /// count).
    pub fn text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        for e in &self.errors {
            s.push_str(&format!("ERROR {}: {}\n      hint: {}\n", e.file, e.what, e.hint));
        }
        if self.errors.is_empty() {
            s.push_str("OK\n");
        } else {
            s.push_str(&format!("{} problem(s)\n", self.errors.len()));
        }
        s
    }
}

/// id fields per content folder (a new file reusing an earlier pack's number would replace it at run time)
const ID_FIELDS: &[(&str, &str, &str)] = &[
    ("skills/", "skill", "24..255"),
    ("enemies/", "type", "150+"),
    ("forms/", "index", "54+"),
    ("characters/", "index", "9..63"),
];

/// Check `mods` over the base pack in `base` (a `content/base` directory).
pub fn check(base: &Path, mods: &[PathBuf]) -> Report {
    let mut r = Report::default();
    for m in mods {
        if m.is_dir() && !m.join("pack.json").exists() {
            r.err(
                format!("{}/pack.json", m.display()),
                "missing",
                r#"every mod needs a pack.json: {"id": "my-mod", "name": "My mod", "format_version": 1, "depends": ["base"]} (replays, save states and the online lobby name mods by id)"#,
            );
        }
    }
    let packs = match crate::mods::load(base, mods) {
        Ok(p) => p,
        Err(e) => {
            r.err("(load)", e.to_string(), "a mod folder that does not exist, or a pack.json that is not valid JSON");
            return r;
        }
    };
    for w in &packs.warnings {
        r.err("(load order)", w.clone(), "install the mod it depends on (same folder as this one) or remove it from `depends`");
    }
    r.line(format!("load order: {}", crate::mods::describe(&packs).join(" -> ")));
    files_pass(&packs, &mut r);
    data_pass(&packs, &mut r);
    r.errors.sort();
    r
}

/// per mod file: parses, its patch / derive / `$extends` target exists, colours, id clashes, mod-vs-mod overwrites
fn files_pass(packs: &Packs, r: &mut Report) {
    // new files (not in the base) each earlier mod added, and the ids per folder seen so far
    let mut added_by: BTreeMap<String, String> = BTreeMap::new();
    let mut ids: BTreeMap<(&str, i64), String> = BTreeMap::new();
    let id_of = |dir: &str, field: &str, b: &[u8]| -> Option<i64> {
        let _ = dir;
        serde_json::from_slice::<serde_json::Value>(b).ok()?.get(field)?.as_i64()
    };
    for f in packs.packs[0].src.list() {
        for &(dir, field, _) in ID_FIELDS {
            if f.starts_with(dir) && f.ends_with(".json") && !f.ends_with("index.json") {
                if let Some(id) = packs.packs[0].src.read(&f).and_then(|b| id_of(dir, field, &b)) {
                    ids.insert((dir, id), format!("base/{f}"));
                }
            }
        }
    }
    for (pi, p) in packs.packs.iter().enumerate().skip(1) {
        let id = p.info.id.clone();
        r.line(format!("\n[{id}]"));
        if p.info.format_version != 1 && p.src.exists("pack.json") {
            r.err(format!("{id}/pack.json"), format!("format_version is {}, must be 1", p.info.format_version), r#"add "format_version": 1"#);
        }
        let earlier = |path: &str| packs.packs[..pi].iter().any(|q| q.src.exists(path));
        for f in p.src.list() {
            if f == "pack.json" || f == "files.json" || f.ends_with(".md") || f.starts_with("tools/") {
                continue;
            }
            let file = format!("{id}/{f}");
            let bytes = p.src.read(&f).unwrap_or_default();
            let json = if f.ends_with(".json") {
                match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        r.err(&file, format!("not valid JSON: {e}"), "fix the syntax at that line / column (a trailing or double comma, a missing quote or brace)");
                        None
                    }
                }
            } else {
                None
            };
            let what = if let Some(s) = f.strip_suffix(".patch.json") {
                if !earlier(&format!("{s}.json")) {
                    r.err(&file, format!("patches {s}.json, which neither the base game nor an earlier mod has"), "check the file name (the base pack's file list: content/base/...), or ship the whole file as a new one (without .patch)");
                }
                "patch".to_string()
            } else if let Some(s) = f.strip_suffix(".derive.json") {
                let spec = json.clone().unwrap_or_default();
                let from = spec["from"].as_str().unwrap_or(s).to_string();
                if from != s && packs.read(&from).is_none() {
                    r.err(&file, format!("derives from {from}, which no pack has"), "`from` is a path in the merged content, e.g. icons/skills/skill_07.png");
                } else if from == s && !earlier(s) {
                    r.err(&file, format!("recolours {s} in place, but no earlier pack has it"), "give `from` an existing file to copy");
                }
                if let Some(o) = spec.get("recolor") {
                    match o.as_object() {
                        None => r.err(&file, "`recolor` is not an object", r##"{"recolor": {"#rrggbb": "#rrggbb", ...}}"##),
                        Some(o) => {
                            for (k, v) in o {
                                for c in [Some(k.as_str()), v.as_str()] {
                                    if c.and_then(wf_content::parse_rgb).is_none() {
                                        r.err(&file, format!("recolor: {} is not a colour", c.map(|c| format!("{c:?}")).unwrap_or_else(|| v.to_string())), r##"colours are "#rrggbb" (six hex digits), e.g. "#ffb529""##);
                                    }
                                }
                            }
                        }
                    }
                }
                format!("derive  (from {from})")
            } else if let Some(s) = f.strip_suffix(".delete") {
                format!("delete  {s}")
            } else if earlier(&f) {
                // a file only another mod added (not the base game): replaced, unless this mod says it builds on it
                if let Some(other) = added_by.get(&f) {
                    if !p.info.depends.contains(other) && !p.info.load_after.contains(other) {
                        r.err(&file, format!("replaces {f}, which mod {other} adds - two mods using the same new path / id"), format!("pick another number / name, or if this is meant to override {other}: add \"load_after\": [\"{other}\"] to pack.json"));
                    }
                }
                "replace".to_string()
            } else {
                added_by.insert(f.clone(), id.clone());
                "new".to_string()
            };
            // items / equipment: `sold_at` names shop events; a (non-story) supply must do something when used
            if (f.starts_with("items/") || f.starts_with("equipment/")) && f.ends_with(".json") && !f.ends_with("index.json") {
                let v = if f.ends_with(".patch.json") { packs.read(&f.replace(".patch.json", ".json")).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) } else { json.clone() };
                if let Some(v) = v {
                    for ev in v["sold_at"].as_array().into_iter().flatten() {
                        let name = ev.as_str().unwrap_or("");
                        let src = packs.read(&format!("scripts/events/{name}.rhai")).and_then(|b| String::from_utf8(b).ok());
                        match src {
                            None => r.err(&file, format!("sold_at {ev}: no event scripts/events/{name}.rhai"), "`sold_at` lists shop events by name, e.g. \"s028\" (the Hercule City Z-Mart)"),
                            Some(s) if !s.lines().take(3).any(|l| l.contains("Purpose: shop")) => r.err(&file, format!("sold_at {name}: that event is not a shop"), "use a shop event (its file's first line says `Purpose: shop`: s028, s030, s031, s033, s034, s036 ...)"),
                            Some(_) => {}
                        }
                    }
                    if f.starts_with("items/") && !v["story_item"].as_bool().unwrap_or(false) {
                        let script = v["use_script"].as_str().is_some_and(|s| !s.is_empty());
                        let ef = &v["effect"];
                        let effect = ef["restore_hp"].as_i64().unwrap_or(0) > 0 || ef["restore_ep"].as_i64().unwrap_or(0) > 0;
                        if !script && !effect {
                            r.err(&file, "a supply with neither `use_script` nor an `effect` - using it does nothing", r#"give it "effect": {"restore_hp": 40} (and / or "restore_ep"), or mark it "story_item": true"#);
                        }
                    }
                }
            }
            // `$extends` target
            if let Some(ext) = json.as_ref().and_then(|v| v.get("$extends")) {
                match ext.as_str() {
                    Some(t) if packs.read(t).is_some() => {}
                    Some(t) => r.err(&file, format!("$extends {t}, which no pack has"), "a path in the merged content, e.g. sprites/106_goku/sprite.json (the base pack's folders)"),
                    None => r.err(&file, "$extends is not a string", r#""$extends": "<path of an existing file>""#),
                }
            }
            // ids: a new file must not reuse a number the base game / an earlier mod has under another file
            if what == "new" {
                for &(dir, field, range) in ID_FIELDS {
                    if f.starts_with(dir) && f.ends_with(".json") && !f.ends_with(".patch.json") {
                        let v = packs.read(&f).and_then(|b| id_of(dir, field, &b));
                        if let Some(n) = v {
                            if let Some(prev) = ids.get(&(dir, n)) {
                                r.err(&file, format!("`{field}` {n} is already used by {prev}"), format!("new {} take numbers in {range} (docs/modding.md §3); to change {prev} instead, ship a .patch.json for it", dir.trim_end_matches('/')));
                            } else {
                                ids.insert((dir, n), file.clone());
                            }
                        }
                    }
                }
            }
            r.line(format!("  {what:8} {f}"));
        }
    }
}

/// the simulation's view of the merged content, then the scripts
fn data_pass(packs: &Packs, r: &mut Report) {
    let d = match crate::battle::game_data(packs) {
        Ok(d) => d,
        Err(e) => {
            r.err("(game data)", e.to_string(), "a file the engine needs is missing or does not have the fields it expects (see the file named)");
            return;
        }
    };
    r.line("");
    for f in &d.fighters {
        let Some(c) = d.chars.iter().find(|c| c.index == f.character) else {
            r.err(format!("fighter {}", f.id), format!("no character {}", f.character), "`character` is a characters/*.json `index`");
            continue;
        };
        for &fm in &f.forms {
            match d.form(fm) {
                None => r.err(format!("fighter {}", f.id), format!("no form {fm}"), "`forms` lists forms/*.json `index` values"),
                // (the game's plain form 45 has no sprite of its own: a link fighter's comes with its setup,
                // `FighterSetup::sprite`)
                Some(form) if form.gfx == 0 && f.id == "f45" => {}
                Some(form) => {
                    let spr = (0..=3).map(|k| form.gfx - 9 - k).find(|&s| d.sprite(s).is_some());
                    match spr.and_then(|s| d.sprite(s)) {
                        None => r.err(format!("form {fm}"), format!("no sprite for gfx {}", form.gfx), "a form's `gfx` is its sprite's `sprite_index` + 9"),
                        Some(s) => {
                            if let Err(e) = sprite::Sheet::load(packs, &format!("sprites/{}", s.id)) {
                                r.err(format!("form {fm}"), format!("sprite sheet: {e}"), "the sprite folder needs sprite.json + its sheet PNG (or `$extends` / a derive of a base one)");
                            }
                        }
                    }
                }
            }
        }
        let skills = f.skills.clone().unwrap_or_else(|| c.skills.clone());
        check_skills(packs, &d, &skills, r);
    }
    for c in &d.chars {
        check_skills(packs, &d, &c.skills, r);
    }
    for (id, pd) in &d.projectiles {
        let what = format!("projectiles/{id}.json");
        match sprite::Sheet::load(packs, &pd.sheet) {
            Err(e) => r.err(&what, format!("sheet {}: {e}", pd.sheet), "`sheet` is a sprite folder (sprite.json + PNG)"),
            Ok(sh) => {
                if let Some(a) = &pd.anim {
                    if sh.def.anim(a).is_none() {
                        r.err(&what, format!("{} has no animation {a}", pd.sheet), "use an animation name of that sprite.json, or `frames`");
                    }
                }
                let n = sh.def.frames.len();
                if pd.frames.iter().flatten().chain(&pd.end_frames).any(|&c| c as usize >= n) {
                    r.err(&what, format!("a cel beyond the sheet's {n} frames"), "cel numbers count from 0");
                }
            }
        }
        if pd.anim.is_none() && pd.frames.iter().all(|f| f.is_empty()) {
            r.err(&what, "neither `anim` nor `frames`", "give the projectile an animation name or a list of cels");
        }
        r.line(format!("projectile {id}: sheet {}", pd.sheet));
    }
    let mod_states: BTreeSet<u16> = packs.packs[1..]
        .iter()
        .flat_map(|p| p.src.list())
        .filter_map(|f| f.strip_prefix("ai/state_").and_then(|x| x.strip_suffix(".json")).and_then(|x| u16::from_str_radix(x, 16).ok()))
        .collect();
    for (st, t) in d.states.iter().chain(d.aux.iter()).filter(|(st, _)| mod_states.contains(st)) {
        let what = format!("ai/state_{st:x}.json");
        for n in &t.nodes {
            if n.k == wf_core::ai::K::Never {
                r.err(&what, format!("unknown node {}", n.name), "node kinds are the game's (docs/modding.md §4c) or ModCondition / ModAction");
            }
            if matches!(n.k, wf_core::ai::K::ModCondition | wf_core::ai::K::ModAction) {
                match n.call.split_once(':') {
                    Some((f, func)) if d.hooks.as_ref().is_some_and(|h| h.has(f, func)) => {}
                    _ => r.err(&what, format!("{} call {:?} is not a function of a hook script", n.name, n.call), "`call` is \"hooks/<file>.rhai:<function>\" and the function must exist"),
                }
            }
        }
    }
    for e in d.enemies.iter().filter(|e| e.ty >= 150) {
        let what = format!("enemy type {} ({})", e.ty, e.name);
        if !d.states.contains_key(&e.ai) {
            r.err(&what, format!("no AI state 0x{:x}", e.ai), "`ai_state` names an ai/state_<hex>.json");
        }
        match e.sprite.and_then(|s| d.sprite(s)) {
            None => r.err(&what, format!("no sprite for gfx {:?}", e.gfx), "`gfx_ids` are sprite_index + 9 of a sprite folder"),
            Some(s) => {
                if let Err(err) = sprite::Sheet::load(packs, &format!("sprites/{}", s.id)) {
                    r.err(&what, format!("sprite sheet: {err}"), "the sprite folder needs sprite.json + its sheet PNG");
                }
            }
        }
        r.line(format!("enemy type {} ({}): AI 0x{:x}, sprite {:?}", e.ty, e.name, e.ai, e.sprite));
    }
    for c in d.chars.iter().filter(|c| c.index >= 9) {
        let what = format!("character {} ({})", c.index, c.name);
        for &fm in c.forms.iter().chain(std::iter::once(&c.form)) {
            match d.form(fm) {
                None => r.err(&what, format!("no form {fm}"), "add forms/*.json with that `index`"),
                Some(f) if f.char_ != c.index => r.err(&what, format!("form {fm} belongs to character {}", f.char_), "a form's `char` must be this character's index"),
                Some(f) => {
                    if !(0..=3).any(|k| d.sprite(f.gfx - 9 - k).is_some()) {
                        r.err(&what, format!("form {fm}: no sprite for gfx {}", f.gfx), "a form's `gfx` is its sprite's `sprite_index` + 9");
                    }
                }
            }
        }
    }
    for a in &d.arenas {
        if !d.maps.contains_key(&a.map) {
            r.err(format!("arena {}", a.id), format!("map {} not loaded", a.map), "`map` is a W-A area key or a maps/<id> folder");
        }
    }
    // scripts: compile, then calls to functions that do not exist (in mod files only)
    let files: BTreeMap<String, String> = packs
        .list()
        .into_iter()
        .filter(|p| p.starts_with("scripts/") && (p.ends_with(".rhai") || p.ends_with(".text.json")))
        .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
        .collect();
    let lib = match wf_script_rhai::Runtime::new(&files, Box::new(wf_script::testhost::TestHost::new(0))) {
        Ok(rt) => Some(rt),
        Err(e) => {
            let (file, msg) = e.split_once(": ").map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or(("scripts".into(), e.clone()));
            r.err(owner_of(packs, &file), msg, "Rhai syntax: docs/scripting.md; an event file returns its node list from fn event()");
            None
        }
    };
    let api = wf_script_rhai::api_names();
    let all_defined: BTreeSet<String> = files.values().flat_map(|s| wf_script_rhai::defined_fns(s)).collect();
    for (path, src) in &files {
        if !path.ends_with(".rhai") || !packs.packs[1..].iter().any(|p| p.src.exists(path)) {
            continue;
        }
        for (line, name) in wf_script_rhai::unknown_calls(src, &all_defined, &api) {
            let near: Vec<&String> = api.iter().filter(|a| a.starts_with(&name[..name.len().min(3)])).take(4).collect();
            r.err(
                format!("{}:{line}", owner_of(packs, path)),
                format!("calls {name}(), which is not a script function"),
                if near.is_empty() { "the functions a story script can call: docs/scripting.md".to_string() } else { format!("a typo? similar: {} (every function: docs/scripting.md)", near.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) },
            );
        }
    }
    let (events, units): (BTreeSet<u16>, BTreeSet<String>) =
        lib.as_ref().map(|rt| (rt.library().graphs.keys().copied().collect(), rt.library().asts.keys().cloned().collect())).unwrap_or_default();
    let check_cb = |what: &str, cb: &Callback, r: &mut Report| match cb {
        Callback::Event(e) => {
            if !e.trim_start_matches('s').parse::<u16>().is_ok_and(|n| events.contains(&n)) && !e.starts_with("header_") {
                r.err(what, format!("event {e} not found"), format!("add scripts/events/{e}.rhai (an event file), or fix the name"));
            }
        }
        Callback::Script(s) => {
            if !units.contains(s) {
                r.err(what, format!("script {s} not found"), format!("add scripts/inline/{s}.rhai, or fix the name"));
            }
        }
    };
    if let Some(fd) = &d.field {
        let modded: BTreeSet<String> = packs.packs[1..]
            .iter()
            .flat_map(|p| p.src.list())
            .filter_map(|f| f.strip_prefix("world/areas/").map(|s| s.trim_end_matches(".json").split('+').next().unwrap_or("").to_string()))
            .collect();
        for a in fd.areas.values().filter(|a| modded.contains(&a.key)) {
            let what = format!("world/areas/{}.json", a.key);
            if a.maps.is_empty() {
                r.err(&what, "no maps", "`maps` lists map folders (maps/<id>/)");
            }
            for m in &a.maps {
                if !fd.maps.contains_key(m) {
                    r.err(&what, format!("map {m} has no maps/{m}/objects.json"), "a map folder holds map.json (Tiled), collision.json and objects.json (docs/maps.md)");
                }
                if let Err(e) = map::Map::load(packs, m) {
                    r.err(&what, format!("maps/{m}/map.json: {e}"), "export the map from Tiled as JSON (docs/maps.md)");
                }
                if let Err(e) = map::Collision::load(packs, m) {
                    r.err(&what, format!("maps/{m}/collision.json: {e}"), "collision.json: the walk grid (docs/maps.md)");
                }
            }
            for s in a.on_enter.iter().chain(&a.on_leave) {
                check_cb(&what, &Callback::Script(s.clone()), r);
            }
            for o in &a.objects {
                match &o.kind {
                    ObjKind::Npc { talk: Some(t), .. } => check_cb(&what, &Callback::Event(t.clone()), r),
                    ObjKind::Enemy { ty, on_defeat, .. } => {
                        if d.enemy(*ty).is_none() {
                            r.err(&what, format!("no enemy type {ty}"), "`enemy_type` is an enemies/*.json `type`");
                        }
                        if let Some(cb) = on_defeat {
                            check_cb(&what, cb, r);
                        }
                    }
                    ObjKind::Trigger { kind: TriggerKind::Script(s), .. } => check_cb(&what, &Callback::Script(s.clone()), r),
                    ObjKind::Trigger { kind: TriggerKind::Event(e), .. } => check_cb(&what, &Callback::Event(e.clone()), r),
                    ObjKind::Trigger { kind: TriggerKind::Exit { to, .. } | TriggerKind::EdgeExit { to, .. }, .. } => {
                        if fd.area(to.world, to.area).is_none() {
                            r.err(&what, format!("exit to {}-{}, which does not exist", to.world, to.area), "the target area needs world/areas/<W-A>.json");
                        }
                    }
                    ObjKind::Unknown => r.err(&what, "an object of unknown type", "object types: docs/world.md"),
                    _ => {}
                }
            }
            r.line(format!("area {} ({}): {} maps, {} objects", a.key, a.name.as_deref().unwrap_or(""), a.maps.len(), a.objects.len()));
        }
    }
    r.line(format!("fighters {}, skills {}, arenas {}, scripts {} ({} events)", d.fighters.len(), d.skills.len(), d.arenas.len(), files.len(), events.len()));
}

/// `<mod id>/<path>` of the last pack that has `path` (the file whose content runs)
fn owner_of(packs: &Packs, path: &str) -> String {
    packs.packs.iter().rev().find(|p| p.src.exists(path)).map(|p| format!("{}/{path}", p.info.id)).unwrap_or_else(|| path.to_string())
}

fn check_skills(packs: &Packs, d: &wf_core::content::GameData, skills: &[i32], r: &mut Report) {
    for &s in skills {
        let what = format!("skill {s}");
        if !d.skills.contains_key(&s) {
            r.err(&what, "no skills/*.json has this `skill` number", "add skills/<s>_<name>.json, or fix the number in the character's skill list");
        }
        let k = d.skill_kind(s);
        if k == wf_core::modkit::SCRIPTED {
            let sc = d.skill_engine(s).and_then(|e| e.script.clone()).unwrap_or_default();
            let h = d.hooks.as_ref();
            if !["on_start", "on_frame"].iter().any(|f| h.is_some_and(|h| h.has(&sc, f))) {
                r.err(&what, format!("script {sc} defines neither on_start nor on_frame"), "a skill script needs fn on_start / fn on_frame (docs/modding.md §4b)");
            }
        } else if k != s && !(1..=23).contains(&k) {
            r.err(&what, format!("engine.base {k} is not a built-in skill"), "`engine.base` is the built-in skill (1..23) whose behaviour runs it, e.g. 7 (Energy Blast)");
        }
        for alt in ["", "_alt"] {
            let p = format!("icons/skills/skill_{s:02}{alt}.png");
            if packs.read(&p).is_none() {
                r.err(&what, format!("missing HUD icon {p}"), format!("add {p} or derive it from a base icon: {p}.derive.json = {{\"from\": \"icons/skills/skill_07{alt}.png\"}}"));
            }
        }
    }
}
