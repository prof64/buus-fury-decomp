//! Mod checker (engine/docs/modding.md §7): loads `<content/base>` + the given mod folders exactly like the game,
//! then reports what each mod adds / replaces / patches / derives and checks every cross reference the engine will
//! follow (fighter -> character / forms -> sprite + sheet, skills -> behaviour + HUD icons, areas -> maps (Tiled +
//! tileset + collision) -> talk / trigger / defeat scripts, all Rhai compiles). Headless, no window, no audio.
//!
//! `cargo run -p wf-app --example modcheck -- <content/base> <mod dir>...` (exit code 1 on errors)

use std::collections::BTreeSet;
use std::path::PathBuf;

use wf_app::wf_content::{map, sprite, FileSource, Packs};
use wf_core::fieldcontent::{Callback, ObjKind, TriggerKind};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: modcheck <content/base> <mod dir>...");
        std::process::exit(2);
    }
    let mods: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
    let packs = match wf_app::mods::load(&PathBuf::from(&args[0]), &mods) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let mut errors: Vec<String> = packs.warnings.iter().map(|w| format!("load order: {w}")).collect();
    println!("load order: {}", wf_app::mods::describe(&packs).join(" -> "));
    // what each mod touches
    for p in &packs.packs[1..] {
        println!("\n[{}]", p.info.id);
        if p.info.format_version != 1 {
            errors.push(format!("{}: pack.json format_version must be 1", p.info.id));
        }
        for f in p.src.list() {
            if f == "pack.json" || f == "files.json" || f.ends_with(".md") || f.starts_with("tools/") {
                continue;
            }
            let earlier = |path: &str| packs.packs.iter().take_while(|q| q.info.id != p.info.id).any(|q| q.src.exists(path));
            let what = if let Some(s) = f.strip_suffix(".patch.json") {
                if earlier(&format!("{s}.json")) {
                    "patch  ".to_string()
                } else {
                    errors.push(format!("{}: {f} patches {s}.json, which no earlier pack has", p.info.id));
                    "patch? ".to_string()
                }
            } else if let Some(s) = f.strip_suffix(".derive.json") {
                let spec: serde_json::Value = serde_json::from_slice(&p.src.read(&f).unwrap_or_default()).unwrap_or_default();
                let from = spec["from"].as_str().unwrap_or(s).to_string();
                if packs.read(s).is_none() {
                    errors.push(format!("{}: {f}: cannot derive from {from}", p.info.id));
                }
                format!("derive  (from {from})")
            } else if let Some(s) = f.strip_suffix(".delete") {
                format!("delete  {s}")
            } else if earlier(&f) {
                "replace".to_string()
            } else {
                "new    ".to_string()
            };
            println!("  {what:8} {f}");
        }
    }
    // the simulation's view of the merged content
    let d = match wf_app::battle::game_data(&packs) {
        Ok(d) => d,
        Err(e) => {
            errors.push(format!("game data: {e}"));
            return finish(errors);
        }
    };
    println!();
    for f in &d.fighters {
        let Some(c) = d.chars.iter().find(|c| c.index == f.character) else {
            errors.push(format!("fighter {}: no character {}", f.id, f.character));
            continue;
        };
        for &fm in &f.forms {
            match d.form(fm) {
                None => errors.push(format!("fighter {}: no form {fm}", f.id)),
                Some(form) => {
                    let spr = (0..=3).map(|k| form.gfx - 9 - k).find(|&s| d.sprite(s).is_some());
                    match spr.and_then(|s| d.sprite(s)) {
                        None => errors.push(format!("form {fm}: no sprite for gfx {}", form.gfx)),
                        Some(s) => {
                            if let Err(e) = sprite::Sheet::load(&packs, &format!("sprites/{}", s.id)) {
                                errors.push(format!("form {fm}: sprite sheet: {e}"));
                            }
                        }
                    }
                }
            }
        }
        let skills = f.skills.clone().unwrap_or_else(|| c.skills.clone());
        check_skills(&packs, &d, &format!("fighter {}", f.id), &skills, &mut errors);
    }
    for c in &d.chars {
        check_skills(&packs, &d, &format!("character {}", c.name), &c.skills, &mut errors);
    }
    for a in &d.arenas {
        if !d.maps.contains_key(&a.map) {
            errors.push(format!("arena {}: map {} not loaded", a.id, a.map));
        }
    }
    // scripts: everything compiles, every referenced event / inline script exists
    let files: std::collections::BTreeMap<String, String> = packs
        .list()
        .into_iter()
        .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
        .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
        .collect();
    let lib = match wf_script_rhai::Runtime::new(&files, Box::new(wf_script::testhost::TestHost::new(0))) {
        Ok(rt) => Some(rt),
        Err(e) => {
            errors.push(format!("scripts: {e}"));
            None
        }
    };
    let (events, units): (BTreeSet<u16>, BTreeSet<String>) =
        lib.as_ref().map(|rt| (rt.library().graphs.keys().copied().collect(), rt.library().asts.keys().cloned().collect())).unwrap_or_default();
    let check_cb =|what: &str, cb: &Callback, errors: &mut Vec<String>| match cb {
        Callback::Event(e) => {
            if !e.trim_start_matches('s').parse::<u16>().is_ok_and(|n| events.contains(&n)) && !e.starts_with("header_") {
                errors.push(format!("{what}: event {e} not found (scripts/events/{e}.rhai)"));
            }
        }
        Callback::Script(s) => {
            if !units.contains(s) {
                errors.push(format!("{what}: script {s} not found (scripts/inline/{s}.rhai)"));
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
            let what = format!("area {}", a.key);
            if a.maps.is_empty() {
                errors.push(format!("{what}: no maps"));
            }
            for m in &a.maps {
                if !fd.maps.contains_key(m) {
                    errors.push(format!("{what}: map {m} has no maps/{m}/objects.json"));
                }
                if let Err(e) = map::Map::load(&packs, m) {
                    errors.push(format!("{what}: maps/{m}/map.json: {e}"));
                }
                if let Err(e) = map::Collision::load(&packs, m) {
                    errors.push(format!("{what}: maps/{m}/collision.json: {e}"));
                }
            }
            for s in a.on_enter.iter().chain(&a.on_leave) {
                check_cb(&what, &Callback::Script(s.clone()), &mut errors);
            }
            for o in &a.objects {
                match &o.kind {
                    ObjKind::Npc { talk: Some(t), .. } => check_cb(&what, &Callback::Event(t.clone()), &mut errors),
                    ObjKind::Enemy { ty, on_defeat, .. } => {
                        if d.enemy(*ty).is_none() {
                            errors.push(format!("{what}: no enemy type {ty}"));
                        }
                        if let Some(cb) = on_defeat {
                            check_cb(&what, cb, &mut errors);
                        }
                    }
                    ObjKind::Trigger { kind: TriggerKind::Script(s), .. } => check_cb(&what, &Callback::Script(s.clone()), &mut errors),
                    ObjKind::Trigger { kind: TriggerKind::Event(e), .. } => check_cb(&what, &Callback::Event(e.clone()), &mut errors),
                    ObjKind::Trigger { kind: TriggerKind::Exit { to, .. } | TriggerKind::EdgeExit { to, .. }, .. } => {
                        if fd.area(to.world, to.area).is_none() {
                            errors.push(format!("{what}: exit to {}-{} which does not exist", to.world, to.area));
                        }
                    }
                    ObjKind::Unknown => errors.push(format!("{what}: an object of unknown type")),
                    _ => {}
                }
            }
            println!("{what} ({}): {} maps, {} objects, ok so far", a.name.as_deref().unwrap_or(""), a.maps.len(), a.objects.len());
        }
    }
    println!("fighters {}, skills {}, arenas {}, scripts {} ({} events)", d.fighters.len(), d.skills.len(), d.arenas.len(), files.len(), events.len());
    finish(errors)
}

fn check_skills(packs: &Packs, d: &wf_core::content::GameData, what: &str, skills: &[i32], errors: &mut Vec<String>) {
    for &s in skills {
        if !d.skills.contains_key(&s) {
            errors.push(format!("{what}: skill {s} has no skills/*.json"));
        }
        let k = d.skill_kind(s);
        if k != s && !(1..=23).contains(&k) {
            errors.push(format!("skill {s}: engine.base {k} is not a built-in skill (1..23)"));
        }
        for alt in ["", "_alt"] {
            let p = format!("icons/skills/skill_{s:02}{alt}.png");
            if packs.read(&p).is_none() {
                errors.push(format!("{what}: skill {s}: missing HUD icon {p}"));
            }
        }
    }
}

fn finish(errors: Vec<String>) {
    if errors.is_empty() {
        println!("OK");
    } else {
        for e in &errors {
            println!("ERROR {e}");
        }
        std::process::exit(1);
    }
}
