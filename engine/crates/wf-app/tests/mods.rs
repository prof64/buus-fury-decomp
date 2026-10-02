//! The example mods (`engine/mods-examples/*`, engine/docs/modding.md) built end to end, headless and muted: each
//! is loaded as a plain folder on top of the extracted base pack, the same way the shells load mods, and its effect
//! is checked in the running game (battle `Session` / story `StorySim`), including rendering without missing
//! content. Skipped without `roms/buu.gba`.

use std::path::PathBuf;
use std::sync::OnceLock;

use wf_app::story::StorySim;
use wf_app::wf_content::Packs;
use wf_core::battle::BattleConfig;
use wf_core::content::GameData;
use wf_core::input as btn;
use wf_core::session::Session;
use wf_core::world::{Body, Event, World};
use wf_view::{RenderView, Sim};

/// the full base pack, extracted once into the cargo tmp dir (`WF_FULL_CONTENT=<content/base>` uses an existing one)
fn base_dir() -> Option<PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if let Ok(d) = std::env::var("WF_FULL_CONTENT") {
            return Some(PathBuf::from(d));
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-mods-content");
        if dir.join(".complete").exists() {
            return Some(dir);
        }
        let rom = wf_rom::testutil::rom()?;
        let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
        for (p, b) in &c.files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        std::fs::write(dir.join(".complete"), b"").unwrap();
        Some(dir)
    })
    .clone()
}

fn mod_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods-examples").join(name)
}

fn packs(mods: &[&str]) -> Option<Packs> {
    let base = base_dir()?;
    let dirs: Vec<PathBuf> = mods.iter().map(|m| mod_dir(m)).collect();
    let p = wf_app::mods::load(&base, &dirs).expect("packs");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(p.packs.len(), mods.len() + 1);
    Some(p)
}

fn data(p: &Packs) -> std::sync::Arc<GameData> {
    wf_app::battle::game_data(p).expect("game data")
}

/// a battle in the forest with the enemy moved 56 px in front of the player (in the blasts' line)
fn battle(p: &Packs, fighter: &str, enemy: &str) -> Session {
    let cfg = wf_app::battle::config("forest", enemy, 1, fighter, None, None, 1, 7);
    let mut s = wf_app::battle::session(p, cfg).expect("session");
    let w = &mut s.world;
    let pi = player(w);
    let (x, y) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
    w.pl(pi).a.facing = 3;
    let ei = w.find(w.enemies[0]).unwrap();
    w.en(ei).a.x = x + (56 << 8);
    w.en(ei).a.y = y;
    s
}

fn player(w: &World) -> usize {
    w.player_idx(0).expect("player")
}

/// What one tap of B (the selected skill) does in a battle: frames from the press to the shot's spawn sound, the
/// EP spent at the start, the first damage the player deals within `frames`.
#[derive(Debug, Default)]
struct Tap {
    skill: Option<i32>,
    ep_spent: i32,
    shot_after: Option<u32>,
    first_damage: Option<i64>,
}

fn tap_b(s: &mut Session, frames: u32, every: u32) -> Tap {
    let me = s.world.ents[player(&s.world)].id;
    let mut t = Tap::default();
    let mut started = None;
    for f in 0..frames {
        let ep0 = s.world.pl_ref(player(&s.world)).rec.ep;
        let k = if f % every < 2 { btn::B } else { 0 };
        let ev: Vec<Event> = s.step(&[k]).to_vec();
        for e in &ev {
            match e {
                Event::SkillStart { id } if started.is_none() => {
                    t.skill = Some(*id);
                    started = Some(f);
                    t.ep_spent = ep0 - s.world.pl_ref(player(&s.world)).rec.ep;
                }
                Event::Sfx { id: 20 | 21, .. } if t.shot_after.is_none() => t.shot_after = started.map(|s0| f - s0),
                Event::Damage { attacker, dmg, .. } if *attacker == me && t.first_damage.is_none() => t.first_damage = Some(*dmg),
                _ => {}
            }
        }
    }
    t
}

fn render_ok(p: &Packs, v: &RenderView) {
    let mut r = wf_render::Renderer::new();
    r.render(p, v, 0, 480, 320);
    assert!(r.errors.is_empty(), "render errors: {:?}", r.errors);
}

// ---------------------------------------------------------------------------------------------------------------

#[test]
fn no_mods_keeps_the_builtin_tables() {
    let Some(p) = packs(&[]) else { return };
    let d = data(&p);
    assert_eq!(d.fighters.len(), wf_core::battle::FIGHTERS.len());
    assert_eq!(d.arenas.len(), wf_core::battle::ARENAS.len());
    assert!(d.skills.values().all(|s| s.engine.is_none()), "no skill has mod parameters");
    for (f, c) in d.fighters.iter().zip(wf_core::battle::FIGHTERS) {
        assert_eq!((f.id.as_str(), f.character, f.fusion, f.forms.as_slice(), f.default_form), (c.0, c.1, c.2, c.3, c.4));
    }
}

#[test]
fn balance_tweak_changes_enemy_stats_and_skill_cost_and_damage() {
    let (Some(base), Some(modded)) = (packs(&[]), packs(&["balance-tweak"])) else { return };
    let (db, dm) = (data(&base), data(&modded));
    assert_eq!(db.enemy(68).unwrap().hp, 500);
    assert_eq!(dm.enemy(68).unwrap().hp, 1500);
    assert_eq!(dm.enemy(68).unwrap().str_, 30);
    // the spawned Majin Fighter has the patched HP
    let s = battle(&modded, "goku", "t68");
    let e = s.world.enemies[0];
    let (_, en) = s.world.enemy_by_id(e).unwrap();
    assert_eq!(en.max_hp1024 >> 10, 1500, "enemy HP in 1/1024");
    // Energy Blast: base = 5% of max EP and 1024/1024 damage; mod = 20 EP flat and 2048/1024
    let tb = tap_b(&mut battle(&base, "goku", "t68"), 300, 60);
    let tm = tap_b(&mut battle(&modded, "goku", "t68"), 300, 60);
    assert_eq!((tb.skill, tm.skill), (Some(7), Some(7)));
    assert_eq!(tm.ep_spent, 20, "patched cost");
    assert_ne!(tb.ep_spent, 20);
    let (b, m) = (tb.first_damage.expect("base hit"), tm.first_damage.expect("mod hit"));
    assert!(m * 10 >= b * 17 && m * 10 <= b * 23, "damage x2: base {b}, mod {m}");
}

#[test]
fn new_skill_rapid_blast_fires_for_goku() {
    let (Some(base), Some(p)) = (packs(&[]), packs(&["new-skill"])) else { return };
    let d = data(&p);
    let sk = d.skills.get(&24).expect("skill 24 loaded");
    assert_eq!(sk.name, "Rapid Blast");
    assert_eq!(d.skill_kind(24), 7);
    assert_eq!(d.chars[0].skills[0], 24, "assigned to Goku");
    // the icon: derived from the Energy Blast icon (palette remap), not shipped
    let icon = wf_app::wf_content::image::decode_indexed(&p.read("icons/skills/skill_24.png").expect("icon")).unwrap();
    let base_icon = wf_app::wf_content::image::decode_indexed(&p.read("icons/skills/skill_07.png").unwrap()).unwrap();
    assert_eq!(icon.idx, base_icon.idx);
    assert_ne!(icon.palette, base_icon.palette);
    // in battle: selected first, 12 EP, the shot leaves 10 frames earlier than the Energy Blast's
    let mut s = battle(&p, "goku", "t68");
    let tm = tap_b(&mut s, 300, 60);
    let tb = tap_b(&mut battle(&base, "goku", "t68"), 300, 60);
    assert_eq!(tm.skill, Some(24));
    assert_eq!(tm.ep_spent, 12);
    let (sb, sm) = (tb.shot_after.expect("base shot"), tm.shot_after.expect("mod shot"));
    assert_eq!(sb - sm, 10, "fire frame 6 vs 16");
    assert!(tm.first_damage.is_some(), "the rapid blast hits");
    // the HUD shows its icon
    let mut v = RenderView::default();
    Sim::view(&s, &mut v);
    assert_eq!(v.hud.first().and_then(|h| h.skill), Some(24));
    render_ok(&p, &v);
}

#[test]
fn new_character_shadow_goku_is_playable() {
    let Some(p) = packs(&["new-character"]) else { return };
    let d = data(&p);
    let f = d.fighter("shadow_goku").expect("fighter");
    assert_eq!((f.character, f.default_form), (9, 54));
    let form = d.form(54).expect("form 54");
    assert_eq!((form.char_, form.gfx), (9, 909));
    assert_eq!(d.sprite(900).map(|s| s.id.as_str()), Some("900_shadow_goku"));
    // same frames as Goku, recoloured sheet
    let a = wf_app::wf_content::image::decode_rgba(&p.read("sprites/900_shadow_goku/sheet.png").unwrap()).unwrap();
    let b = wf_app::wf_content::image::decode_rgba(&p.read("sprites/106_goku/sheet.png").unwrap()).unwrap();
    assert_eq!((a.w, a.h), (b.w, b.h));
    assert!(a.px.iter().zip(&b.px).all(|(x, y)| x >> 24 == y >> 24), "alpha kept");
    let changed = a.px.iter().zip(&b.px).filter(|(x, y)| x != y).count();
    assert!(changed > 1000, "recoloured pixels: {changed}");
    // the battle: its own record (stats + growth from characters/shadow_goku.json), form, sprite and skills
    let mut s = battle(&p, "shadow_goku", "t68");
    let goku = battle(&p, "goku", "t68");
    let (pl, gk) = (s.world.pl_ref(player(&s.world)).clone(), goku.world.pl_ref(player(&goku.world)).clone());
    assert_eq!(pl.form, 54);
    assert_eq!(pl.a.sprite, Some(900));
    assert_eq!(pl.skills, vec![7, 15, 14]);
    assert_ne!((pl.rec.max_hp, pl.rec.max_ep, pl.rec.pow), (gk.rec.max_hp, gk.rec.max_ep, gk.rec.pow));
    assert!(pl.rec.max_ep > gk.rec.max_ep, "more EP growth than Goku");
    let t = tap_b(&mut s, 300, 60);
    assert_eq!(t.skill, Some(7));
    assert!(t.first_damage.is_some(), "Shadow Goku's blast hits");
    // walks: 60 frames of RIGHT move it
    let x0 = s.world.pl_ref(player(&s.world)).a.x;
    for _ in 0..60 {
        s.step(&[btn::RIGHT]);
    }
    assert!(s.world.pl_ref(player(&s.world)).a.x > x0);
    let mut v = RenderView::default();
    Sim::view(&s, &mut v);
    assert!(v.sprites.iter().any(|sp| sp.sheet == "sprites/900_shadow_goku"));
    render_ok(&p, &v);
}

// ---- story-mode mods ------------------------------------------------------------------------------------------

fn story(p: &Packs, area: (i32, i32)) -> StorySim {
    StorySim::new(p, area, vec![], 5).expect("story")
}

fn w(s: &StorySim) -> &World {
    s.world()
}
fn wm(s: &mut StorySim) -> &mut World {
    &mut s.session.world
}
fn area(s: &StorySim) -> (i32, i32) {
    let f = w(s).field.as_ref().unwrap();
    (f.area.world, f.area.area)
}
fn settled(s: &StorySim) -> bool {
    w(s).field.as_ref().is_some_and(|f| matches!(f.mode, wf_core::field::Mode::Normal))
}
fn place(s: &mut StorySim, x: i32, y: i32, facing: u8) {
    let i = player(w(s));
    let a = &mut wm(s).pl(i).a;
    a.x = x << 8;
    a.y = y << 8;
    a.facing = facing;
}
/// steps holding `k` (A is pressed / released alternately so text boxes and choices advance)
fn run(s: &mut StorySim, frames: u32, k: u16, mut until: impl FnMut(&StorySim) -> bool) -> bool {
    let mut ev = vec![];
    for f in 0..frames {
        let a = if k & btn::A != 0 && f % 6 >= 3 { k & !btn::A } else { k };
        s.step(&[a], &mut ev);
        assert!(s.errors.is_empty(), "script errors: {:?}", s.errors);
        if until(s) {
            return true;
        }
    }
    false
}
fn npcs(s: &StorySim) -> Vec<(i32, i32, Option<i32>)> {
    w(s).ents
        .iter()
        .filter(|e| !e.removed)
        .filter_map(|e| match &e.body {
            Body::Npc(n) => Some((n.a.px(), n.a.py(), n.talk)),
            _ => None,
        })
        .collect()
}

#[test]
fn new_arena_scenario_runs_to_completion() {
    let Some(p) = packs(&["new-arena-scenario"]) else { return };
    let mut s = story(&p, (8, 3));
    run(&mut s, 60, 0, |_| false);
    let (nx, ny, talk) = npcs(&s).into_iter().find(|n| n.2 == Some(900)).expect("the announcer stands in 8-3");
    assert_eq!((nx, ny, talk), (488, 472, Some(900)));
    let senzu0 = w(&s).story.supplies[21];
    // talk to him from below, accept (choice 0 = Yes): the warp to the arena
    place(&mut s, nx, ny + 18, 1);
    assert!(run(&mut s, 1500, btn::A, |s| area(s) == (90, 1) && settled(s)), "warped into the Tournament of Fury");
    assert_eq!(w(&s).field.as_ref().unwrap().map, "area_08_071");
    // three rounds: each champion appears after the announcer's line, and its defeat starts the next round
    for ty in [21, 123, 101] {
        let found = run(&mut s, 1500, btn::A, |s| w(s).enemies.iter().any(|&id| w(s).enemy_by_id(id).is_some_and(|(_, e)| e.ty == ty && !e.defeated)));
        assert!(found, "round with enemy type {ty} started");
        let mut v = RenderView::default();
        s.view(&mut v);
        render_ok(&p, &v);
        let id = *w(&s).enemies.iter().find(|&&id| w(&s).enemy_by_id(id).is_some_and(|(_, e)| e.ty == ty)).unwrap();
        let i = w(&s).find(id).unwrap();
        wf_core::enemy::die(wm(&mut s), i, false);
    }
    // the prize, the champion flag, back to the forest
    assert!(run(&mut s, 3000, btn::A, |s| area(s) == (8, 3) && settled(s) && w(s).story.flag(1100)), "scenario finished");
    assert_eq!(w(&s).story.supplies[21], senzu0 + 2, "two Senzu Beans");
    // a champion gets the rematch greeting (flag-dependent branch) - and can decline
    let (nx, ny, _) = npcs(&s).into_iter().find(|n| n.2 == Some(900)).unwrap();
    place(&mut s, nx, ny + 18, 1);
    let says = |s: &StorySim, what: &str| {
        let mut v = RenderView::default();
        s.view(&mut v);
        v.dialog.as_ref().is_some_and(|d| d.text.contains(what))
    };
    assert!(run(&mut s, 200, btn::A, |s| says(s, "rematch")), "the champion's greeting");
}

#[test]
fn new_area_is_reachable_and_its_npc_talks() {
    let Some(p) = packs(&["new-area"]) else { return };
    let d = data(&p);
    let fd = d.field.as_ref().unwrap();
    let a = fd.area(90, 2).expect("area 90-2");
    assert_eq!(a.maps, vec!["mod_hidden_grove".to_string()]);
    assert!(fd.maps.contains_key("mod_hidden_grove"));
    let mut s = story(&p, (8, 3));
    run(&mut s, 60, 0, |_| false);
    // walk up into the forest's top-right alcove: the new exit
    place(&mut s, 440, 36, 1);
    assert!(run(&mut s, 400, btn::UP, |s| area(s) == (90, 2) && settled(s)), "entered the Hidden Grove");
    assert_eq!(w(&s).field.as_ref().unwrap().map, "mod_hidden_grove");
    let mut v = RenderView::default();
    s.view(&mut v);
    assert_eq!(v.map.as_ref().map(|m| m.id.as_str()), Some("mod_hidden_grove"));
    render_ok(&p, &v);
    // collision from the row form: the hermit's grove is walled in
    let x0 = w(&s).pl_ref(player(w(&s))).a.px();
    run(&mut s, 300, btn::LEFT, |_| false);
    assert!(w(&s).pl_ref(player(w(&s))).a.px() >= 16 && w(&s).pl_ref(player(w(&s))).a.px() < x0, "walked left to the wall");
    // the hermit: first talk = a Senzu Bean and flag 1101
    let (nx, ny, _) = npcs(&s).into_iter().find(|n| n.2 == Some(910)).expect("the hermit");
    let senzu0 = w(&s).story.supplies[21];
    place(&mut s, nx, ny + 18, 1);
    assert!(run(&mut s, 1500, btn::A, |s| s.world().story.flag(1101) && !s.world().field.as_ref().unwrap().busy), "talked to the hermit");
    assert_eq!(w(&s).story.supplies[21], senzu0 + 1);
    // and back out through the path at the bottom
    place(&mut s, 160, 212, 0);
    assert!(run(&mut s, 400, btn::DOWN, |s| area(s) == (8, 3) && settled(s)), "back in the forest");
}

#[test]
fn all_example_mods_together_are_deterministic() {
    let all = ["balance-tweak", "new-skill", "new-character", "new-arena-scenario", "new-area"];
    let Some(p) = packs(&all) else { return };
    let d = data(&p);
    let objs = &d.field.as_ref().unwrap().area(8, 3).unwrap().objects;
    let base = packs(&[]).unwrap();
    let base_objs = data(&base).field.as_ref().unwrap().area(8, 3).unwrap().objects.len();
    assert_eq!(objs.len(), base_objs + 2, "both mods appended to 8-3 (announcer + grove exit)");
    let go = |p: &Packs| {
        let mut s = story(p, (8, 3));
        let mut ev = vec![];
        for f in 0..400u32 {
            let k = [btn::UP, btn::RIGHT, btn::A, btn::LEFT][(f / 50) as usize % 4];
            s.step(&[k], &mut ev);
        }
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        s.state_hash()
    };
    assert_eq!(go(&p), go(&p));
    // battle with every mod: the new fighter with the new skill, snapshot / restore replays identically
    let cfg = BattleConfig { players: vec![wf_core::battle::FighterSetup { fighter: "shadow_goku".into(), form: 54, ..Default::default() }, Default::default()], ..wf_app::battle::config("crater", "t68", 2, "goku", None, None, 2, 3) };
    let mut s = wf_app::battle::session(&p, cfg).unwrap();
    let keys = |f: u32| [if f % 40 < 2 { btn::B } else { btn::RIGHT }, if f % 30 < 2 { btn::B } else { btn::LEFT }];
    for f in 0..120 {
        s.step(&keys(f));
    }
    let snap = s.snapshot();
    let mut h1 = vec![];
    for f in 120..400 {
        s.step(&keys(f));
        h1.push(s.hash());
    }
    s.restore(&snap);
    for (k, f) in (120..400).enumerate() {
        s.step(&keys(f));
        assert_eq!(s.hash(), h1[k], "frame {f}");
    }
    // the lobby / replay content fingerprints tell modded content apart
    assert_ne!(wf_net::netsim::content_id(&d), wf_net::netsim::content_id(&data(&base)));
    assert_ne!(wf_app::battle::content_id(&p).mods_hash, wf_app::battle::content_id(&base).mods_hash);
}
