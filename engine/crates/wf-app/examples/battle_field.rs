//! Battle mode against the field: the same fighter / form / action (and the same dummy enemy) run in a battle session
//! (arena "forest" = area 8-3) and in a field session of area 8-3 at the same spot; every frame the drawn actors and
//! effects (relative to the player), the damage numbers, the screen effects and the shake are compared. The field is
//! the pixel-verified rendering (tests/pixels.rs); a difference is a Battle-mode drawing bug (or a rule difference
//! that shows). Headless, muted.
//!   cargo run --release -p wf-app --example battle_field [-- FILTER] [--all] [--verbose]
//! FILTER: a substring of the scenario name (fighter/form/action). Prints one line per scenario (first differing
//! frame, how many frames differ) and the first differences with both sides' lists.

use std::collections::BTreeMap;
use std::sync::Arc;

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::battle::{BattleConfig, FighterSetup, RngInit};
use wf_core::content::GameData;
use wf_core::field::FieldConfig;
use wf_core::session::Session;
use wf_core::world::{Body, World};
use wf_view::{FrameSel, RenderView, Sim};

const A: u16 = 1;
const B: u16 = 2;
const L: u16 = 0x200;
const R: u16 = 0x100;
const RIGHT: u16 = 0x10;
const LEFT: u16 = 0x20;
const UP: u16 = 0x40;
const DOWN: u16 = 0x80;

#[derive(Clone)]
struct Scn {
    name: String,
    fighter: String,
    form: i32,
    /// select this skill slot before the inputs (None: keep)
    sel: Option<usize>,
    /// per frame held keys
    keys: Vec<u16>,
    /// dummy enemies: (type, dx, dy) from the player, no AI
    enemies: Vec<(i32, i32, i32)>,
    level: i32,
}

fn hold(k: u16, n: usize) -> Vec<u16> {
    vec![k; n]
}

fn ents_kind(w: &World, id: u32) -> Option<&'static str> {
    w.ents.iter().find(|e| e.id == id && !e.removed).map(|e| match &e.body {
        Body::Player(_) => "player",
        Body::Enemy(_) => "enemy",
        Body::Shot(_) => "shot",
        Body::Fx(_) => "fx",
        Body::Hand(_) => "hand",
        _ => "other",
    })
}

/// the drawn sprites of the fight (players, enemies, shots, effects), relative to player 0, in draw order
fn norm(w: &World, v: &RenderView) -> Vec<String> {
    let Some(pi) = w.player_idx(0) else { return vec![] };
    let (px, py) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    let pid = w.ents[pi].id;
    // entity ids differ between the sessions: name them by kind + their order of first appearance
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let mut n = 0;
    let mut out = vec![];
    for s in &v.sprites {
        let kind = match ents_kind(w, s.entity) {
            Some(k) if k != "other" => k,
            // a sprite of an entity removed this frame (the presenter's late OAM): still part of the fight
            None if s.sheet.starts_with("effects/") || s.sheet.starts_with("sprites/") => "gone",
            _ => continue,
        };
        // the field's scenery (props, map sprites, NPCs) is not part of a battle arena
        if kind == "gone" && !w.ents.iter().any(|e| e.id == s.entity) && !s.sheet.starts_with("effects/") && !s.sheet.starts_with("sprites/") {
            continue;
        }
        // (map props: the field's scenery objects, drawn by the map - compared separately)
        if s.sheet.starts_with("effects/props_") {
            continue;
        }
        let name = if s.entity == pid {
            "P".to_string()
        } else {
            let _ = (&mut names, &mut n);
            kind.to_string()
        };
        let fr = match &s.frame {
            FrameSel::Frame { index, flip } => format!("{index}{}", if *flip { "f" } else { "" }),
            FrameSel::Anim { anim, dir, t, looping } => format!("a:{anim}/{dir}/{t}/{looping}"),
        };
        out.push(format!(
            "{name} {} #{fr} ({},{}) p{}{}{}",
            s.sheet,
            s.x - px,
            s.y - py,
            s.priority,
            if s.mirror { " m" } else { "" },
            match s.effect {
                wf_view::SpriteEffect::None => String::new(),
                e => format!(" {e:?}"),
            }
        ));
    }
    for d in &v.damage_numbers {
        out.push(format!("num {} ({},{}) crit {}", d.value, d.x - px, d.y - py, d.crit));
    }
    let cam = v.cameras.first().copied().unwrap_or_default();
    let shake = v.map.as_ref().map(|m| m.shake).unwrap_or((0, 0));
    out.push(format!(
        "screen fade {} pal {:?} scale {:?} shake {:?} objshake {:?} hud {:?}",
        v.fade_black,
        v.palette_fade.as_ref().map(|p| format!("{p:?}")),
        v.palette_scale.as_ref().map(|p| format!("{p:?}")),
        shake,
        v.obj_shake,
        v.hud.first().map(|h| (h.hidden, h.y_off, h.hp, h.ep, h.ss, h.skill, h.skill_alt)),
    ));
    // BF_CAM: where the screen frames the player (the camera's offset from the player's position)
    if std::env::var_os("BF_CAM").is_some() {
        out.push(format!("cam ({},{})", cam.x - px, cam.y - py));
    }
    out
}

/// the launcher's setup of a pick (`Launcher::battle_config`): stat points spent evenly, the story's skill level at
/// this level, a full SS gauge; Hercule / Videl / Pikkon on Goku's record
fn setup(data: &GameData, fighter: &str, form: i32, level: i32) -> FighterSetup {
    let ch = data.fighter(fighter).filter(|fd| fd.fusion.is_none()).map(|fd| fd.character);
    let skill_level = match ch {
        Some(0..=4) if level >= 150 => 2,
        Some(0..=4) if level >= 100 => 1,
        _ => 0,
    };
    let mut f = FighterSetup { fighter: fighter.into(), form, level, auto_spend: "even".into(), skill_level, full_gauge: true, ..Default::default() };
    if data.fighter(fighter).is_some_and(|fd| fd.character == 8 && fd.fusion.is_none()) {
        let (r, _, _) = wf_core::battle::build_record(data, &FighterSetup { fighter: "goku".into(), level, auto_spend: "even".into(), ..Default::default() });
        f.fixed = Some(wf_core::battle::FixedRecord { hp: r.max_hp, ep: r.max_ep, stats: [r.str_, r.pow, r.end, r.spd], skills: None });
    }
    f
}

fn battle_session(data: &Arc<GameData>, s: &Scn) -> Session {
    let f = setup(data, &s.fighter, s.form, s.level);
    let cfg = BattleConfig { arena: "forest".into(), players: vec![f], phases: vec![], rng: RngInit::Seed(7), xp: false };
    Session::new(Arc::clone(data), cfg)
}

fn field_session(data: &Arc<GameData>, b: &World) -> Session {
    let pi = b.player_idx(0).unwrap();
    let (px, py) = (b.pl_ref(pi).a.px(), b.pl_ref(pi).a.py());
    let mut s = Session::new_field(
        Arc::clone(data),
        FieldConfig { world: 8, area: 3, pos: Some((px, py)), facing: 3, rng: RngInit::Seed(7), xp: false, ..Default::default() },
    );
    let w = &mut s.world;
    // only the player: the area's enemies / NPCs / pickups go
    for k in 0..w.ents.len() {
        let keep = matches!(w.ents[k].body, Body::Player(_) | Body::MapSprite(_));
        if !keep {
            if matches!(w.ents[k].body, Body::Enemy(_)) {
                wf_core::enemy::unmark_self(w, k);
            }
            // (their sprite memory too: the field's allocator decides whether a damage number / puff appears)
            let id = w.ents[k].id;
            wf_core::vram::release(w, wf_core::vram::Owner::Ent(id));
            w.ents[k].removed = true;
        }
    }
    w.ents.retain(|e| !e.removed);
    w.enemies.clear();
    // the battle's fighter as it is (record, form, skills, gauges), in the field's player object
    let fpi = w.player_idx(0).unwrap();
    if let (Body::Player(fp), Body::Player(bp)) = (&mut w.ents[fpi].body, &b.ents[pi].body) {
        let slot = fp.slot;
        let mut np = bp.clone();
        np.slot = slot;
        **fp = *np;
    }
    s
}

fn add_dummies(s: &mut Session, en: &[(i32, i32, i32)]) {
    let w = &mut s.world;
    let pi = w.player_idx(0).unwrap();
    let (px, py) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    // (a negative type: the enemy keeps its own AI)
    let active = en.iter().any(|e| e.0 < 0);
    for &(ty, dx, dy) in en {
        if let Some(i) = wf_core::battle::spawn_enemy_at(w, ty.abs(), px + dx, py + dy, 0, None, !active) {
            w.en(i).a.facing = 2;
            // BF_TOUGH: the enemy survives the punches (its AI's reactions without the defeat, which the field leaves to
            // its story scene)
            if std::env::var("BF_TOUGH").is_ok() {
                w.en(i).hp1024 = 99_999 << 10;
            }
        }
    }
    if !active {
        inert(w);
    }
}

/// the dummies' own (initial) AI state does nothing (a `Never` node): they stand, take hits through the state's real
/// on-hit tree (knockback, hurt states run as the game's) and come back to standing
fn inert(w: &mut World) {
    for k in 0..w.ents.len() {
        let Body::Enemy(e) = &mut w.ents[k].body else { continue };
        let st0 = w.data.enemy(e.ty).map(|t| t.ai).unwrap_or(0);
        for se in e.ai.stack.iter_mut() {
            if se.state == st0 && se.tree.nodes.first().is_none_or(|n| n.k != wf_core::ai::K::Never) {
                let node = wf_core::ai::CNode { k: wf_core::ai::K::Never, name: "Never".into(), addr: 0, p: Default::default(), kids: vec![], script: 0, call: String::new(), args: vec![] };
                se.tree = Arc::new(wf_core::ai::BehTree { state: st0, nodes: vec![node] });
                se.rt = vec![Default::default()];
            }
        }
    }
}

fn run(data: &Arc<GameData>, s: &Scn, verbose: bool) -> (usize, usize, Vec<String>) {
    let mut bs = battle_session(data, s);
    let mut fs = field_session(data, &bs.world);
    fs.world.rng = bs.world.rng;
    add_dummies(&mut bs, &s.enemies);
    add_dummies(&mut fs, &s.enemies);
    if let Some(k) = s.sel {
        for w in [&mut bs.world, &mut fs.world] {
            let pi = w.player_idx(0).unwrap();
            if let Body::Player(p) = &mut w.ents[pi].body {
                // (a skill the picker's records lack: forced as the only one, `sel` = 100 + id)
                if k >= 100 {
                    p.skills = vec![k as i32 - 100];
                    p.sel = 0;
                } else {
                    p.sel = k.min(p.skills.len().saturating_sub(1));
                }
            }
        }
    }
    // the same RNG from here; the same camera (the field's arrival snap differs from the battle's start)
    fs.world.rng = bs.world.rng;
    fs.world.cameras[0] = bs.world.cameras[0].clone();
    let mut ev = vec![];
    let mut first = None;
    let mut nd = 0;
    let mut notes = vec![];
    // settle (the field's arrival, the presenter's first frame)
    for _ in 0..4 {
        Sim::step(&mut bs, &[0], &mut ev);
        Sim::step(&mut fs, &[0], &mut ev);
    }
    fs.world.rng = bs.world.rng;
    for (k, &keys) in s.keys.iter().enumerate() {
        // BF_TRACE=<frame>: every generator call of that frame with its caller, both sessions
        let tr = std::env::var("BF_TRACE").ok().and_then(|v| v.parse::<usize>().ok()) == Some(k);
        let set = |on: bool| wf_core::rng::TRACE.store(on, std::sync::atomic::Ordering::Relaxed);
        if tr {
            eprintln!("== battle f{k}");
            set(true);
        }
        Sim::step(&mut bs, &[keys], &mut ev);
        if tr {
            eprintln!("== field f{k}");
        }
        Sim::step(&mut fs, &[keys], &mut ev);
        set(false);
        if !s.enemies.iter().any(|e| e.0 < 0) {
            inert(&mut bs.world);
            inert(&mut fs.world);
        }
        let (mut vb, mut vf) = (RenderView::default(), RenderView::default());
        Sim::view(&bs, &mut vb);
        Sim::view(&fs, &mut vf);
        let (nb, nf) = (norm(&bs.world, &vb), norm(&fs.world, &vf));
        if std::env::var("BF_ABS").ok().and_then(|v| v.split_once('-').map(|(a, b)| (a.parse::<usize>().unwrap_or(0), b.parse::<usize>().unwrap_or(0)))).is_some_and(|(a, b)| (a..=b).contains(&k)) {
            for (tag, w, v) in [("b", &bs.world, &vb), ("f", &fs.world, &vf)] {
                let pi = w.player_idx(0).unwrap();
                let p = w.pl_ref(pi);
                let sp: Vec<String> = v.sprites.iter().filter(|s| s.entity == w.ents[pi].id).map(|s| format!("{} ({},{})", s.sheet, s.x, s.y)).collect();
                eprintln!("{tag} f{k} p ({},{}) sprites {:?} cam {:?}", p.a.x, p.a.y, sp, v.cameras.first());
            }
        }
        if std::env::var("BF_GRID").ok().and_then(|v| v.parse::<usize>().ok()) == Some(k) {
            if let (Some(cb), Some(cf)) = (bs.world.col.as_ref(), fs.world.col.as_ref()) {
                let diff: Vec<(usize, usize, u8, u8)> = (0..cb.enemy.len()).filter(|&j| cb.enemy[j] != cf.enemy[j]).map(|j| (j % 256 * 8, j / 256 * 8, cb.enemy[j], cf.enemy[j])).collect();
                let wd = cb.walk.as_ref().map(|v| v.len()).unwrap_or(0) != cf.walk.as_ref().map(|v| v.len()).unwrap_or(0) || cb.walk != cf.walk;
                eprintln!("f{k} enemy grid: {} tiles differ (x, y, battle, field) {:?} | walk grids differ {wd}", diff.len(), &diff[..diff.len().min(40)]);
            }
        }
        if std::env::var_os("BF_RNG").is_some() && (bs.world.rng.calls, bs.world.rng.a) != (fs.world.rng.calls, fs.world.rng.a) {
            eprintln!("f{k} rng b {} {:08x} f {} {:08x}", bs.world.rng.calls, bs.world.rng.a, fs.world.rng.calls, fs.world.rng.a);
        }
        if std::env::var_os("BF_COUNT").is_some() {
            let ehp = |w: &World| w.ents.iter().filter_map(|e| match &e.body { Body::Enemy(n) => Some(n.hp1024 >> 10), _ => None }).collect::<Vec<_>>();
            eprintln!("f{k} nums b {} f {} | ehp b {:?} f {:?}", vb.damage_numbers.len(), vf.damage_numbers.len(), ehp(&bs.world), ehp(&fs.world));
        }
        if std::env::var("BF_SHOTS").ok().and_then(|v| v.split_once('-').map(|(a, b)| (a.parse::<usize>().unwrap_or(0), b.parse::<usize>().unwrap_or(0)))).is_some_and(|(a, b)| (a..=b).contains(&k)) {
            for (tag, w) in [("b", &bs.world), ("f", &fs.world)] {
                for e in w.ents.iter().filter(|e| !e.removed) {
                    match &e.body {
                        Body::Shot(s) => eprintln!("{tag} f{k} shot {:?}", s),
                        Body::Enemy(n) => {
                            eprintln!("{tag} f{k} enemy ({},{}) hp {} st {:?} anim {:?} ea {:?} flash {} {} facing {}", n.a.x, n.a.y, n.hp1024 >> 10, n.ai.states(), n.a.anim, (n.ea.pos.raw, n.ea.pos.frame, n.ea.pos.tick), n.a.flash, n.a.flash_map, n.a.facing);
                            if std::env::var_os("BF_AI").is_some() {
                                if let Some(se) = n.ai.stack.last() {
                                    let names: Vec<String> = se.tree.nodes.iter().enumerate().map(|(j, nd)| format!("{j}:{}{}", nd.name, se.rt.get(j).map(|r| format!("[f{} i{}]", r.f, r.i)).unwrap_or_default())).collect();
                                    eprintln!("    tree {} {}", se.state, names.join(" "));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if std::env::var_os("BF_STATE").is_some() {
            for (tag, w) in [("b", &bs.world), ("f", &fs.world)] {
                let pi = w.player_idx(0).unwrap();
                let p = w.pl_ref(pi);
                eprintln!("{tag} f{k} gs {:?} state {:?} raw {} frame {} tick {} anim {:?} t {} pos ({},{}) body {:?} facing {}", p.gs, p.state, p.ap.raw, p.ap.frame, p.ap.tick, p.a.anim, p.a.anim_t, p.a.x, p.a.y, p.a.body_box(), p.a.facing);
                eprintln!("    room {:?} hp {} owners {:?}", wf_core::vram::room_of(w), p.hp(), w.field.as_ref().and_then(|f| f.vram.as_ref()).map(|v| v.owners.len()));
                eprintln!("    shake {:?} shown {:?} ticked {} shakes {:?} cam ({},{})", w.g.shake_off, w.g.shake_shown, w.g.shakes_ticked == w.frame, w.g.shakes, w.cameras[0].x, w.cameras[0].y);
                for e in &w.ents {
                    if let Body::Enemy(n) = &e.body {
                        eprintln!("    enemy {} pos ({},{}) body {:?} hittable {} hp {}", n.ty, n.a.px(), n.a.py(), n.a.body_box(), n.a.hittable, n.hp1024 >> 10);
                    }
                }
            }
        }
        // the player's KO + 199: the battle ends there (everything holds for the result), the field's death script
        // takes over - not a drawing difference; the comparison stops
        let ko_end = |w: &World| {
            let pi = w.player_idx(0).unwrap();
            let p = w.pl_ref(pi);
            p.ko_at >= 0 && w.frame as i64 - p.ko_at >= 199
        };
        if ko_end(&bs.world) && ko_end(&fs.world) {
            break;
        }
        if nb != nf {
            nd += 1;
            if first.is_none() {
                first = Some(k);
            }
            if notes.len() < if verbose { 12 } else { 3 } {
                let only_b: Vec<&String> = nb.iter().filter(|x| !nf.contains(x)).collect();
                let only_f: Vec<&String> = nf.iter().filter(|x| !nb.contains(x)).collect();
                let order = only_b.is_empty() && only_f.is_empty();
                notes.push(format!(
                    "  f{k}{}\n    battle only: {:?}\n    field only:  {:?}",
                    if order { " (draw order)" } else { "" },
                    only_b,
                    only_f
                ));
                if order && verbose {
                    notes.push(format!("    battle: {nb:?}\n    field:  {nf:?}"));
                }
            }
        }
    }
    (first.unwrap_or(usize::MAX), nd, notes)
}

fn scenarios(data: &GameData) -> Vec<Scn> {
    let mut v = vec![];
    for &(_, _, _, fid, form) in wf_app::launcher::PICKS {
        {
            let level = 120;
            let base = Scn { name: String::new(), fighter: fid.into(), form, sel: None, keys: vec![], enemies: vec![(68, 18, 0)], level };
            // idle + walk
            let mut keys = hold(0, 20);
            keys.extend(hold(RIGHT, 20));
            keys.extend(hold(0, 10));
            keys.extend(hold(UP, 12));
            keys.extend(hold(0, 20));
            v.push(Scn { name: format!("{fid}/{form}/move"), keys, enemies: vec![], ..base.clone() });
            // the punch combo on the dummy
            let mut keys = vec![];
            for _ in 0..4 {
                keys.push(A);
                keys.extend(hold(0, 9));
            }
            keys.extend(hold(0, 70));
            v.push(Scn { name: format!("{fid}/{form}/punch"), keys, ..base.clone() });
            // every skill of the form: B held 50 frames (charges / beams), released, then 140 frames
            let (rec_skills, n) = {
                let f = setup(data, fid, form, level);
                let (_, skills, _) = wf_core::battle::build_record(data, &f);
                let n = skills.len();
                (skills, n)
            };
            for k in 0..n {
                let id = rec_skills[k];
                if id == 14 {
                    // Instant Transmission: the cursor onto the dummy (B held), released: the jump, the punch, the return
                    let mut keys = hold(B | RIGHT, 8);
                    keys.extend(hold(B, 6));
                    keys.extend(hold(0, 200));
                    v.push(Scn { name: format!("{fid}/{form}/skill{k}:{id}/pick"), sel: Some(k), keys, ..base.clone() });
                    continue;
                }
                let mut keys = hold(B, 50);
                keys.extend(hold(0, 140));
                v.push(Scn { name: format!("{fid}/{form}/skill{k}:{id}/hold"), sel: Some(k), keys, ..base.clone() });
                let mut keys = hold(B, 1);
                keys.extend(hold(0, 160));
                v.push(Scn { name: format!("{fid}/{form}/skill{k}:{id}/tap"), sel: Some(k), keys, ..base.clone() });
                // facing up and left too (the effects' per-direction records)
                let mut keys = hold(LEFT, 1);
                keys.extend(hold(0, 2));
                keys.extend(hold(B, 40));
                keys.extend(hold(0, 120));
                v.push(Scn { name: format!("{fid}/{form}/skill{k}:{id}/left"), sel: Some(k), keys, ..base.clone() });
                let mut keys = hold(UP, 1);
                keys.extend(hold(0, 2));
                keys.extend(hold(B, 40));
                keys.extend(hold(0, 120));
                v.push(Scn { name: format!("{fid}/{form}/skill{k}:{id}/up"), sel: Some(k), keys, ..base.clone() });
            }
            // block, dash
            let mut keys = hold(R, 20);
            keys.extend(hold(0, 6));
            keys.push(RIGHT);
            keys.push(0);
            keys.push(RIGHT);
            keys.extend(hold(RIGHT, 20));
            keys.extend(hold(0, 30));
            v.push(Scn { name: format!("{fid}/{form}/block_dash"), keys, ..base.clone() });
        }
    }
    // skills no picker record has (Saiyaman, fusion dance, SS3, the other SS): forced on a fighter of their own
    for &(fid, form, id) in &[("gohan", 10, 18), ("gohan", 10, 19), ("gohan", 14, 18), ("goku", 3, 22), ("goku", 2, 21), ("goten", 19, 12), ("trunks", 21, 12), ("goku", 2, 12), ("vegeta", 26, 12)] {
        let base = Scn { name: String::new(), fighter: fid.into(), form, sel: Some(100 + id as usize), keys: vec![], enemies: vec![(68, 18, 0)], level: 120 };
        let mut keys = hold(B, 50);
        keys.extend(hold(0, 200));
        v.push(Scn { name: format!("{fid}/{form}/forced:{id}/hold"), keys, ..base.clone() });
        let mut keys = hold(B, 1);
        keys.extend(hold(0, 250));
        v.push(Scn { name: format!("{fid}/{form}/forced:{id}/tap"), keys, ..base.clone() });
    }
    // every enemy the launcher offers, with its own AI, against an idle then punching Goku: their attacks, projectiles,
    // hit reactions, deaths
    for e in data.enemies.iter().filter(|e| (e.hp > 0 || e.ai != 0) && !e.name.is_empty()) {
        let mut keys = hold(0, 300);
        for _ in 0..30 {
            keys.push(A);
            keys.extend(hold(0, 9));
        }
        keys.extend(hold(0, 100));
        v.push(Scn { name: format!("vs/t{}/{}", e.ty, e.name), fighter: "goku".into(), form: 2, sel: None, keys, enemies: vec![(-e.ty, 56, 0)], level: 120 });
    }
    let _ = (L, LEFT, DOWN);
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let filter: Option<&String> = args.iter().find(|a| !a.starts_with("--"));
    let repo = wf_rom::testutil::repo_root();
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    let mut bad = 0;
    let mut total = 0;
    for s in scenarios(&data) {
        if filter.is_some_and(|f| !s.name.contains(f.as_str())) {
            continue;
        }
        total += 1;
        let (first, nd, notes) = run(&data, &s, verbose);
        if nd == 0 {
            println!("ok   {}", s.name);
        } else {
            bad += 1;
            println!("DIFF {}: {} of {} frames, first f{}", s.name, nd, s.keys.len(), first);
            for n in notes {
                println!("{n}");
            }
        }
    }
    println!("{bad} of {total} scenarios differ");
}
