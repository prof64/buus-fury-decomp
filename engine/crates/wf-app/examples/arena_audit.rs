//! Battle mode's arena audit (headless, muted): every opponent the Battle page offers, in the launcher's own battle
//! (arena rules on), twice - a player standing still (does the opponent come, attack, KO him; how hard it hits) and a
//! bot that walks up beside the opponent (from its left, then its right, every 400 frames) and punches, with a skill
//! now and then (does the opponent take damage on either side, does the fight end by KO). One line per opponent.
//!
//!   cargo run --release -p wf-app --example arena_audit -- CONTENT_BASE_DIR [--only t68,t60] [--frames 5400]
//!        [--arena crater] [--all]

use wf_app::launcher::{Launcher, Settings};
use wf_app::App;
use wf_view::buttons as b;

struct Run {
    end: String,
    first_hurt: Option<u64>,
    max_hit_on_player: i32,
    max_hit_on_enemy: i64,
    hp_left: (i32, i64),
    dmg_side: [i64; 2],
    far: i32,
    off_screen: u64,
    max_hp: (i32, i64),
    end_frame: Option<u64>,
    won: bool,
}

fn run(dir: &str, cfg: &wf_core::battle::BattleConfig, frames: u64, bot: bool) -> Run {
    let mut app = App::battle(wf_app::mods::load(std::path::Path::new(dir), &[]).expect("content"), cfg.clone()).expect("battle");
    let obs = |app: &App| wf_core::observe::observe(&wf_app::battle::CoreSim::of(app.sim.as_ref()).unwrap().world, 0);
    let mut r = Run { end: String::new(), first_hurt: None, max_hit_on_player: 0, max_hit_on_enemy: 0, hp_left: (0, 0), dmg_side: [0; 2], far: 0, off_screen: 0, max_hp: (0, 0), end_frame: None, won: false };
    let mut ev = vec![];
    let mut prev = obs(&app);
    // (the bot's unstick: walking but not moving for 20 frames -> 30 frames up or down, then on)
    let (mut stuck, mut detour, mut detour_dir) = (0u32, 0u32, 0u16);
    for f in 0..frames {
        let o = obs(&app);
        let outcome = format!("{:?}", wf_app::battle::CoreSim::of(app.sim.as_ref()).unwrap().outcome);
        let p = o.players.first();
        if let (Some(p), Some(q)) = (p, prev.players.first()) {
            let d = q.hp - p.hp;
            if d > 0 {
                r.max_hit_on_player = r.max_hit_on_player.max(d);
                r.first_hurt.get_or_insert(f);
            }
        }
        let side = ((f / 400) & 1) as usize;
        for e in &o.enemies {
            if let Some(q) = prev.enemies.iter().find(|q| q.actor.id == e.actor.id) {
                let d = q.hp - e.hp;
                if d > 0 {
                    r.max_hit_on_enemy = r.max_hit_on_enemy.max(d);
                    r.dmg_side[side] += d;
                }
            }
        }
        let target = p.and_then(|p| {
            o.enemies
                .iter()
                .filter(|e| !e.defeated && e.hp > 0 && e.primary)
                .min_by_key(|e| (e.actor.x - p.actor.x).abs() + (e.actor.y - p.actor.y).abs())
                .map(|e| (p, e))
        });
        if let Some((p, e)) = target {
            let dist = (e.actor.x - p.actor.x).abs().max((e.actor.y - p.actor.y).abs());
            r.far = r.far.max(dist);
            let c = o.camera;
            if !(e.actor.x + 24 > c[0] && e.actor.x < c[0] + 240 && e.actor.y + 24 > c[1] && e.actor.y < c[1] + 160) {
                r.off_screen += 1;
            }
        }
        r.hp_left = (p.map(|p| p.hp).unwrap_or(0), o.enemies.iter().filter(|e| e.primary).map(|e| e.hp).sum());
        r.max_hp.0 = r.max_hp.0.max(p.map(|p| p.hp_max).unwrap_or(0));
        r.max_hp.1 = r.max_hp.1.max(o.enemies.iter().filter(|e| e.primary).map(|e| e.hp_max).max().unwrap_or(0));
        if outcome != "Running" {
            r.end = format!("{outcome}@{f}");
            r.end_frame = Some(f);
            r.won = outcome == "Won";
            break;
        }
        if std::env::var("AUDIT_TRACE").is_ok() && f % 60 == 0 {
            if let Some((p, e)) = target {
                if std::env::var("AUDIT_ALL").is_ok() {
                    for x in &o.enemies {
                        eprintln!("    ty {} id {} ({},{}) foot {:?} hp {} def {} prim {} hit {} ai {:?}", x.ty, x.actor.id, x.actor.x, x.actor.y, x.actor.foot, x.hp, x.defeated, x.primary, x.actor.hittable, x.ai);
                    }
                }
                eprintln!("f{f} P ({},{}) fac {} hp {} st {} | E ty {} ({},{},z{}) fac {} body {:?} foot {:?} hittable {} blk {} hp {} ai {:?}", p.actor.x, p.actor.y, p.actor.facing, p.hp, p.state, e.ty, e.actor.x, e.actor.y, e.actor.z, e.actor.facing, e.actor.body, e.actor.foot, e.actor.hittable, e.actor.blocking, e.hp, e.ai);
            }
        }
        // the bot's aim: the nearest live, hittable opponent (a summoned helper too - Janemba's minis), not a part
        let aim = p.and_then(|p| {
            o.enemies
                .iter()
                .filter(|e| !e.defeated && e.hp > 0 && e.actor.hittable && !matches!(e.ty, 52 | 53 | 54 | 55))
                .min_by_key(|e| (e.actor.x - p.actor.x).abs() + (e.actor.y - p.actor.y).abs() + if e.primary { 0 } else { 24 })
                .map(|e| (p, e))
        });
        let mut keys = 0u16;
        if bot {
            if let Some((p, e)) = aim {
                // beside the opponent's footprint (left side, then right side), on its foot line
                let (ex, ey) = ((e.actor.foot[0] + e.actor.foot[2]) / 2, (e.actor.foot[1] + e.actor.foot[3]) / 2);
                let (px, py) = ((p.actor.foot[0] + p.actor.foot[2]) / 2, (p.actor.foot[1] + p.actor.foot[3]) / 2);
                let want_x = if side == 0 { e.actor.foot[0] - 6 } else { e.actor.foot[2] + 6 };
                let (dx, dy) = (want_x - px, ey - py);
                if dx.abs() > 8 {
                    keys |= if dx > 0 { b::RIGHT } else { b::LEFT };
                }
                if dy.abs() > 4 {
                    keys |= if dy > 0 { b::DOWN } else { b::UP };
                }
                // in line with a far opponent (a shooter keeping its distance): face it and fire the skill (the ki
                // blast), every 24 frames
                if dy.abs() <= 6 && (ex - px).abs() > 48 && f % 24 < 2 {
                    let face_right = ex > px;
                    keys = if (face_right && p.actor.facing != 3) || (!face_right && p.actor.facing != 2) {
                        if face_right { b::RIGHT } else { b::LEFT }
                    } else {
                        b::B
                    };
                } else if keys == 0 {
                    // face it, then punch (a skill every 120 frames)
                    let face_right = ex > px;
                    if (face_right && p.actor.facing != 3) || (!face_right && p.actor.facing != 2) {
                        keys |= if face_right { b::RIGHT } else { b::LEFT };
                    } else if f % 120 == 0 {
                        keys |= b::B;
                    } else if f % 10 < 2 {
                        keys |= b::A;
                    }
                }
            }
        }
        if bot {
            let moved = match (o.players.first(), prev.players.first()) {
                (Some(a), Some(q)) => a.actor.x != q.actor.x || a.actor.y != q.actor.y,
                _ => true,
            };
            let walking = keys & (b::LEFT | b::RIGHT | b::UP | b::DOWN) != 0;
            if detour > 0 {
                detour -= 1;
                keys = detour_dir | (keys & (b::LEFT | b::RIGHT));
            } else if walking && !moved {
                stuck += 1;
                if stuck > 20 {
                    stuck = 0;
                    detour = 30;
                    detour_dir = if (f / 97) & 1 == 0 { b::DOWN } else { b::UP };
                }
            } else {
                stuck = 0;
            }
        }
        ev.clear();
        app.sim.step(&[keys], &mut ev);
        prev = o;
    }
    if r.end.is_empty() {
        r.end = "running".into();
    }
    r
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().expect("content base dir").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let frames: u64 = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(5400);
    let packs = wf_app::mods::load(std::path::Path::new(&dir), &[]).expect("content");
    let l0 = Launcher::new(&packs, Settings::default(), true);
    let mut list: Vec<(String, String)> = l0.enemy_list().to_vec();
    if args.iter().any(|a| a == "--all") {
        let d = wf_app::battle::game_data(&packs).expect("data");
        list = d.enemies.iter().filter(|e| e.hp > 0).map(|e| (format!("t{}", e.ty), e.name.clone())).collect();
    }
    if let Some(only) = opt("--only") {
        let o: Vec<&str> = only.split(',').collect();
        list.retain(|(id, _)| o.contains(&id.as_str()));
    }
    println!("{} opponents", list.len());
    let tsv_path = opt("--tsv");
    let mut tsv = String::from("type\tname\tarena\tlevel\tplayer_max_hp\tenemy_max_hp\tenemy_max_hit_pct_of_player\tplayer_max_hit_pct_of_enemy\tidle_end\tidle_frames_to_player_ko\tbot_end\tbot_frames_to_enemy_ko\tbot_dmg_left\tbot_dmg_right\n");
    for (id, name) in list {
        let mut s = Settings::default();
        s.battle.preset_mode = false;
        s.battle.enemy = id.clone();
        let l = Launcher::new(&packs, s, true);
        let mut cfg = l.battle_config();
        cfg.arena_rules = true;
        cfg.phases.truncate(1);
        if let Some(p) = cfg.phases.first_mut() {
            p.count = 1;
        }
        if let Some(a) = opt("--arena") {
            cfg.arena = a;
        }
        for f in cfg.players.iter_mut() {
            f.engage = true;
            f.full_gauge = true;
            f.skill_level = 2;
            f.auto_spend = "even".into();
        }
        cfg.players.truncate(1);
        let level = cfg.players[0].level;
        let idle = run(&dir, &cfg, frames, false);
        let fight = run(&dir, &cfg, frames, true);
        println!(
            "{id:5} {name:34} {arena:9} lv{level:3} | idle: {ie:12} hurt@{ih:5} maxhit {imh:4} far {ifar:4} off {ioff:5} | bot: {fe:12} side L/R {l:7}/{r:7} maxhit on P {fmh:4} on E {feh:7} hp left P {php:5} E {ehp:8} off {foff:5}",
            arena = cfg.arena,
            ie = idle.end,
            ih = idle.first_hurt.map(|f| f.to_string()).unwrap_or("-".into()),
            imh = idle.max_hit_on_player,
            ifar = idle.far,
            ioff = idle.off_screen,
            fe = fight.end,
            l = fight.dmg_side[0],
            r = fight.dmg_side[1],
            fmh = fight.max_hit_on_player,
            feh = fight.max_hit_on_enemy,
            php = fight.hp_left.0,
            ehp = fight.hp_left.1,
            foff = fight.off_screen,
        );
        let pmax = idle.max_hp.0.max(fight.max_hp.0).max(1);
        let emax = idle.max_hp.1.max(fight.max_hp.1).max(1);
        let ko = |r: &Run, want_won: bool| if r.won == want_won { r.end_frame.map(|f| f.to_string()).unwrap_or("-".into()) } else { "-".into() };
        tsv += &format!(
            "{id}\t{name}\t{}\t{level}\t{pmax}\t{}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            cfg.arena,
            emax,
            100.0 * idle.max_hit_on_player.max(fight.max_hit_on_player) as f64 / pmax as f64,
            100.0 * fight.max_hit_on_enemy as f64 / emax as f64,
            idle.end,
            ko(&idle, false),
            fight.end,
            ko(&fight, true),
            fight.dmg_side[0],
            fight.dmg_side[1],
        );
        if let Some(t) = &tsv_path {
            std::fs::write(t, &tsv).ok();
        }
    }
}
