//! Every opponent the Battle page offers, played (headless, muted): the launcher's own battle for each enemy (its auto
//! arena / level, Goku base), 600 frames standing still, then a bot that walks to the nearest enemy and punches for
//! up to 3,000 frames. Per opponent: is it drawn, does it move / hurt the player on its own, does it lose HP, how the
//! fight ends - the Battle page's list must hold only opponents that fight and can be beaten.
//!
//!   cargo run --release -p wf-app --example opponents -- CONTENT_BASE_DIR [--all] [--only t68,t60] [--frames 3000]
//!   (--all: every enemy type of the content, not only the listed ones)

use wf_app::launcher::{Launcher, Settings};
use wf_app::App;
use wf_view::{buttons as b, RenderView, Sim};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().expect("content base dir").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let all = args.iter().any(|a| a == "--all");
    let frames: u64 = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(3000);
    let packs = wf_app::mods::load(std::path::Path::new(&dir), &[]).expect("content");
    let l0 = Launcher::new(&packs, Settings::default(), true);
    let mut list: Vec<(String, String)> = l0.enemy_list().to_vec();
    if all {
        let d = wf_app::battle::game_data(&packs).expect("data");
        list = d.enemies.iter().map(|e| (format!("t{}", e.ty), e.name.clone())).collect();
    }
    if let Some(only) = opt("--only") {
        let o: Vec<&str> = only.split(',').collect();
        list.retain(|(id, _)| o.contains(&id.as_str()));
    }
    println!("{} opponents", list.len());
    for (id, name) in list {
        let mut s = Settings::default();
        s.battle.preset_mode = false;
        s.battle.enemy = id.clone();
        let l = Launcher::new(&packs, s, true);
        let cfg = l.battle_config();
        let arena = cfg.arena.clone();
        let level = cfg.players[0].level;
        let mut app = match App::battle(wf_app::mods::load(std::path::Path::new(&dir), &[]).expect("content"), cfg) {
            Ok(a) => a,
            Err(e) => {
                println!("{id:5} {name:40} ERROR {e}");
                continue;
            }
        };
        let mut ev = vec![];
        let obs = |app: &App| wf_core::observe::observe(&wf_app::battle::CoreSim::of(app.sim.as_ref()).unwrap().world, 0);
        let o0 = obs(&app);
        let e0: Vec<(u32, i32, i32, i64)> = o0.enemies.iter().map(|e| (e.actor.id, e.actor.x, e.actor.y, e.hp)).collect();
        let p0 = o0.players.first().map(|p| (p.actor.x, p.actor.y, p.hp)).unwrap_or_default();
        let (mut moved, mut hurt, mut drawn, mut on_screen) = (0i32, false, false, false);
        let mut lost_hp = false;
        let mut end = String::new();
        let mut view = RenderView::default();
        for f in 0..frames + 600 {
            let o = obs(&app);
            let outcome = format!("{:?}", wf_app::battle::CoreSim::of(app.sim.as_ref()).unwrap().outcome);
            if outcome != "Running" {
                end = format!("{outcome} @{f}");
                break;
            }
            let p = o.players.first();
            if f < 600 {
                for e in &o.enemies {
                    if let Some(s) = e0.iter().find(|s| s.0 == e.actor.id) {
                        moved = moved.max((e.actor.x - s.1).abs() + (e.actor.y - s.2).abs());
                    }
                }
                if p.is_some_and(|p| p.hp < p0.2) {
                    hurt = true;
                }
            }
            for e in &o.enemies {
                if let Some(s) = e0.iter().find(|s| s.0 == e.actor.id) {
                    if e.hp < s.3 {
                        lost_hp = true;
                    }
                }
            }
            if f % 30 == 0 {
                app.sim.view(&mut view);
                let ids: Vec<u32> = o.enemies.iter().map(|e| e.actor.id).collect();
                if view.sprites.iter().any(|s| ids.contains(&s.entity)) {
                    drawn = true;
                }
                // on the GBA screen (the camera's 240 x 160)
                let c = o.camera;
                if o.enemies.iter().any(|e| e.actor.x + 16 > c[0] && e.actor.x < c[0] + 240 && e.actor.y + 16 > c[1] && e.actor.y < c[1] + 160) && f < 60 {
                    on_screen = true;
                }
            }
            // the bot: after 600 idle frames, to the nearest live enemy, punch when close
            let mut keys = 0u16;
            if f >= 600 {
                let near = p.and_then(|p| o.enemies.iter().filter(|e| !e.defeated && e.hp > 0).min_by_key(|e| (e.actor.x - p.actor.x).abs() + (e.actor.y - p.actor.y).abs()).map(|e| (p, e)));
                if let Some((p, e)) = near {
                    let (dx, dy) = (e.actor.x - p.actor.x, e.actor.y - p.actor.y);
                    if dx.abs() > 22 {
                        keys |= if dx > 0 { b::RIGHT } else { b::LEFT };
                    } else if dy.abs() > 10 {
                        keys |= if dy > 0 { b::DOWN } else { b::UP };
                    } else if f % 12 < 2 {
                        keys |= b::A;
                    }
                    if dx.abs() <= 22 && dy.abs() <= 10 && f % 12 >= 2 {
                        // face it
                        keys |= if dx.abs() > dy.abs() { if dx > 0 { b::RIGHT } else { b::LEFT } } else if dy > 0 { b::DOWN } else { b::UP };
                        keys &= !(b::RIGHT | b::LEFT | b::UP | b::DOWN);
                    }
                }
            }
            ev.clear();
            app.sim.step(&[keys], &mut ev);
        }
        if end.is_empty() {
            let o = obs(&app);
            end = format!("running, enemy hp {:?}", o.enemies.iter().map(|e| e.hp).collect::<Vec<_>>());
        }
        println!(
            "{id:5} {name:42} arena {arena:9} lv {level:3}  drawn {drawn:5} start-on-screen {on_screen:5} idle-moved {moved:4} idle-hurt {hurt:5} took-hp {lost_hp:5}  {end}"
        );
    }
}
