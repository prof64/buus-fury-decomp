//! Demo world: a placeholder until `wf-core`'s World drives the frontend. Loads an arena map + its collision from
//! content and lets N characters walk / dash with the game's walk rules (horizontal step then vertical, corner
//! sliding, footprint vs the walk grid, facing table, camera easing). Deterministic, integer-only, and it obeys the
//! same contract as the real core: inputs in, [`RenderView`] + events out.

use wf_content::map::{Collision, Map};
use wf_content::sprite::SpriteDef;
use wf_content::{Error, Packs, Result};
use wf_view::{buttons as b, AudioEvent, CameraView, FrameSel, HudView, MapView, RenderView, Sim, SimEvent, SpriteDraw, SpriteEffect, GBA_W};

#[derive(Clone, Debug)]
pub struct DemoConfig {
    /// Area key (`8-3` = Southwest Forest); the map, music and collision come from content.
    pub area: String,
    /// Character record (`characters/<id>.json`) for stats.
    pub character: String,
    /// Sprite key of the walker.
    pub sprite: String,
    pub spawn: (i32, i32),
    pub players: usize,
}

impl Default for DemoConfig {
    fn default() -> Self {
        DemoConfig { area: "8-3".into(), character: "goku".into(), sprite: "sprites/106_goku".into(), spawn: (96, 248), players: 1 }
    }
}

/// Walk facing table (combo = h (0 none, 1 L, 2 R) + 3 * v (0 none, 1 U, 2 D)) -> new facing per old facing.
const FACE_TBL: [[u8; 4]; 9] = [[0, 1, 2, 3], [2, 2, 2, 2], [3, 3, 3, 3], [1, 1, 1, 1], [1, 1, 2, 2], [1, 1, 3, 3], [0, 0, 0, 0], [0, 0, 2, 2], [0, 0, 3, 3]];
const DASH_VEC: [(i32, i32); 4] = [(0, 1), (0, -1), (-1, 0), (1, 0)];
const DIR_KEY: [u16; 4] = [b::DOWN, b::UP, b::LEFT, b::RIGHT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum State {
    Idle,
    Walk,
    Dash,
}

#[derive(Clone, Debug, Hash)]
struct Walker {
    /// 24.8 fixed-point position (top-left of the footprint = the sprite anchor position).
    x: i32,
    y: i32,
    facing: u8,
    state: State,
    anim_t: u32,
    /// last pressed direction key + frame (double tap = dash)
    last_key: u16,
    last_key_frame: u64,
    prev: u16,
    /// camera, 22.10 fixed point
    cam_x: i32,
    cam_y: i32,
    /// camera on screen this frame: the game writes BG scroll / OAM before its camera update, so the picture
    /// shows the previous frame's camera (verified against the emulator)
    shown: (i32, i32),
}

pub struct DemoWorld {
    frame: u64,
    map: String,
    world_w: i32,
    world_h: i32,
    cam_max: (i32, i32),
    col: Collision,
    bounds: [i32; 4],
    sprite: String,
    shadow: Option<String>,
    foot: (i32, i32),
    walk: i32,
    dash: i32,
    song: Option<u16>,
    name: String,
    level: u32,
    hp: i32,
    ep: i32,
    walkers: Vec<Walker>,
    has_dash_anim: bool,
    skill: Option<u8>,
}

impl DemoWorld {
    pub fn new(packs: &Packs, cfg: &DemoConfig) -> Result<DemoWorld> {
        let areas: serde_json::Value = packs.json("areas.json")?;
        let area = areas["areas"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["key"] == cfg.area.as_str()))
            .ok_or_else(|| Error(format!("areas.json: no area {}", cfg.area)))?;
        let map_id = area["maps"][0].as_str().ok_or_else(|| Error(format!("area {}: no map", cfg.area)))?.to_string();
        let song = area["song"].as_u64().map(|s| s as u16);
        let map = Map::load(packs, &map_id)?;
        let col = Collision::load(packs, &map_id)?;
        let def = SpriteDef::load(packs, &cfg.sprite)?;
        let ch: serde_json::Value = packs.json(&format!("characters/{}.json", cfg.character))?;
        let ng = &ch["new_game"];
        let spd = ng["SPD"].as_i64().unwrap_or(40) as i32;
        // movement (characters/index.json): walk = 5*SPD + 102, dash = walk + 320 (1/256 px per frame)
        let walk = 5 * spd + 102;
        let foot = def.footprint.map(|f| (f[0], f[1])).unwrap_or((16, 8));
        let wb = col.walk_bounds;
        let bounds = [wb[0] * 8, wb[1] * 8, wb[2] * 8, wb[3] * 8];
        let walkers = (0..cfg.players.max(1))
            .map(|p| {
                let (x, y) = (cfg.spawn.0 + 24 * p as i32, cfg.spawn.1);
                Walker {
                    x: x << 8,
                    y: y << 8,
                    facing: 0,
                    state: State::Idle,
                    anim_t: 0,
                    last_key: 0,
                    last_key_frame: 0,
                    prev: 0,
                    cam_x: 0,
                    cam_y: 0,
                    shown: (0, 0),
                }
            })
            .collect();
        let mut w = DemoWorld {
            frame: 0,
            map: map_id,
            world_w: map.world_w,
            world_h: map.world_h,
            cam_max: (map.camera_max_x, map.camera_max_y),
            col,
            bounds,
            sprite: cfg.sprite.clone(),
            shadow: def.shadow.as_ref().map(|s| format!("effects/{s}")),
            foot,
            walk,
            dash: walk + 320,
            song,
            name: ch["name"].as_str().unwrap_or("Player").to_string(),
            level: ng["level"].as_u64().unwrap_or(1) as u32,
            hp: ng["hp"].as_i64().unwrap_or(100) as i32,
            ep: ng["ep"].as_i64().unwrap_or(100) as i32,
            walkers,
            has_dash_anim: def.anim("dash").is_some(),
            skill: ng["skills"][0].as_u64().map(|s| s as u8),
        };
        for i in 0..w.walkers.len() {
            w.camera(i, true);
            w.walkers[i].shown = (w.walkers[i].cam_x >> 10, w.walkers[i].cam_y >> 10);
        }
        Ok(w)
    }

    pub fn world_size(&self) -> (i32, i32) {
        (self.world_w, self.world_h)
    }

    fn fits(&self, x: i32, y: i32) -> bool {
        let [x0, y0, x1, y1] = self.bounds;
        let (fw, fh) = self.foot;
        x >= x0 && y >= y0 && x + fw <= x1 && y + fh <= y1 && self.col.test_walk(x, y, x + fw, y + fh)
    }

    /// Walking try-move 0x08012960: take the new position if the footprint fits; else slide around a corner:
    /// flags & 1 (vertical step) snaps x to the nearest multiple of 8 when within 2 px, flags & 2 (horizontal
    /// step) does the same for y.
    fn walk_try(&mut self, i: usize, vx: i32, vy: i32, flags: u8) {
        let w = &self.walkers[i];
        let (nx24, ny24) = (w.x + vx, w.y + vy);
        let (mut nx, ny) = (nx24 >> 8, ny24 >> 8);
        if self.fits(nx, ny) {
            let w = &mut self.walkers[i];
            w.x = nx24;
            w.y = ny24;
            return;
        }
        let mut x24 = nx24;
        if flags & 1 != 0 && ((nx & 7) < 2 || (nx & 7) > 5) {
            let sx = (nx + 4) & !7;
            if self.fits(sx, ny) {
                x24 = sx << 8;
                let w = &mut self.walkers[i];
                w.x = x24;
                w.y = ny24;
                nx = sx;
            }
        }
        if flags & 2 != 0 && ((ny & 7) < 2 || (ny & 7) > 5) {
            let sy = (ny + 4) & !7;
            if self.fits(nx, sy) {
                let w = &mut self.walkers[i];
                w.x = x24;
                w.y = sy << 8;
            }
        }
    }

    fn set_state(&mut self, i: usize, s: State) {
        let w = &mut self.walkers[i];
        if w.state != s {
            w.state = s;
            w.anim_t = 0;
        }
    }

    fn step_walker(&mut self, i: usize, held: u16, events: &mut Vec<SimEvent>) {
        let prev = self.walkers[i].prev;
        let pressed = held & !prev;
        self.walkers[i].prev = held;
        // double tap a direction within 20 frames -> dash (idle state 0x08013022)
        if pressed & b::DPAD != 0 {
            let key = [b::DOWN, b::UP, b::LEFT, b::RIGHT].into_iter().find(|k| pressed & k != 0).unwrap_or(0);
            let w = &mut self.walkers[i];
            if self.has_dash_anim && key == w.last_key && self.frame - w.last_key_frame <= 20 {
                w.facing = DIR_KEY.iter().position(|&k| k == key).unwrap_or(0) as u8;
                self.set_state(i, State::Dash);
            }
            let w = &mut self.walkers[i];
            w.last_key = key;
            w.last_key_frame = self.frame;
        }
        match self.walkers[i].state {
            State::Dash => {
                let w = &self.walkers[i];
                let key = DIR_KEY[w.facing as usize];
                if held & key == 0 {
                    self.set_state(i, State::Idle);
                } else {
                    let (vx, vy) = DASH_VEC[w.facing as usize];
                    let (nx, ny) = (w.x + vx * self.dash, w.y + vy * self.dash);
                    if !self.fits(nx >> 8, ny >> 8) {
                        // bump: the game enters its hurt state with sound 59; the demo just stops
                        events.push(SimEvent::Audio(AudioEvent::PlaySfx { id: 59, volume: 64, pan: 0 }));
                        self.set_state(i, State::Idle);
                    } else {
                        let ws = self.walk;
                        let f = w.facing;
                        let (mut tx, mut ty) = (nx, ny);
                        if held & b::UP != 0 && f != 1 {
                            ty -= ws;
                        }
                        if held & b::DOWN != 0 && f != 0 {
                            ty += ws;
                        }
                        if held & b::LEFT != 0 && f != 2 {
                            tx -= ws;
                        }
                        if held & b::RIGHT != 0 && f != 3 {
                            tx += ws;
                        }
                        let w = &mut self.walkers[i];
                        w.x = nx;
                        w.y = ny;
                        if self.fits(tx >> 8, ty >> 8) {
                            let w = &mut self.walkers[i];
                            w.x = tx;
                            w.y = ty;
                        }
                    }
                }
            }
            _ => {
                if held & b::DPAD != 0 {
                    let sp = self.walk;
                    let mut combo = 0;
                    if held & b::LEFT != 0 {
                        self.walk_try(i, -sp, 0, 2);
                        combo = 1;
                    } else if held & b::RIGHT != 0 {
                        self.walk_try(i, sp, 0, 2);
                        combo = 2;
                    }
                    if held & b::UP != 0 {
                        self.walk_try(i, 0, -sp, 1);
                        combo += 3;
                    } else if held & b::DOWN != 0 {
                        self.walk_try(i, 0, sp, 1);
                        combo += 6;
                    }
                    let w = &mut self.walkers[i];
                    w.facing = FACE_TBL[combo][w.facing as usize];
                    self.set_state(i, State::Walk);
                } else {
                    self.set_state(i, State::Idle);
                }
            }
        }
        self.walkers[i].anim_t += 1;
    }

    /// Camera 0x080077EC: eases toward the footprint centre (screen point 120, 84) by 3/64 of the distance
    /// horizontally and 9/128 vertically per frame (22.10 fixed point), then clamps to the map.
    fn camera(&mut self, i: usize, snap: bool) {
        let (fw, fh) = self.foot;
        let (cmx, cmy) = self.cam_max;
        let w = &mut self.walkers[i];
        let (px, py) = (w.x >> 8, w.y >> 8);
        let tx = ((px * 2 + fw) >> 1) - 120;
        let ty = ((py * 2 + fh) >> 1) - 84;
        if snap {
            w.cam_x = tx << 10;
            w.cam_y = ty << 10;
        } else {
            w.cam_x += (((tx << 10) - w.cam_x) * 3) >> 6;
            w.cam_y += (((ty << 10) - w.cam_y) * 9) >> 7;
        }
        w.cam_x = w.cam_x.clamp(0, cmx.max(0) << 10);
        w.cam_y = w.cam_y.clamp(0, cmy.max(0) << 10);
    }
}

impl Sim for DemoWorld {
    fn step(&mut self, inputs: &[u16], events: &mut Vec<SimEvent>) {
        if self.frame == 0 {
            events.push(SimEvent::Audio(match self.song {
                Some(s) => AudioEvent::PlaySong(s),
                None => AudioEvent::StopSong,
            }));
        }
        for i in 0..self.walkers.len() {
            self.step_walker(i, inputs.get(i).copied().unwrap_or(0), events);
            self.walkers[i].shown = (self.walkers[i].cam_x >> 10, self.walkers[i].cam_y >> 10);
            self.camera(i, false);
        }
        self.frame += 1;
    }

    fn view(&self, out: &mut RenderView) {
        out.clear();
        out.frame = self.frame;
        out.map = Some(MapView { id: self.map.clone(), anim_frame: self.frame as u32, tile_frame: None, shake: (0, 0), shake_sub: (0, 0), mode7: None, layers: None });
        // OAM: y-sorted (front-most first); per actor: body, then shadow (lower index on top)
        let mut order: Vec<usize> = (0..self.walkers.len()).collect();
        order.sort_by_key(|&i| (-(self.walkers[i].y >> 8), i));
        for i in order {
            let w = &self.walkers[i];
            let (x, y) = (w.x >> 8, w.y >> 8);
            let (anim, looping) = match w.state {
                State::Idle => ("idle", true),
                State::Walk => ("walk", true),
                State::Dash => ("dash", true),
            };
            out.sprites.push(SpriteDraw {
                entity: i as u32 + 1,
                sheet: self.sprite.clone(),
                frame: FrameSel::Anim { anim: anim.into(), dir: w.facing, t: w.anim_t, looping },
                x,
                y,
                priority: 3,
                mirror: false,
                effect: SpriteEffect::None,
            });
            if let Some(sh) = &self.shadow {
                out.sprites.push(SpriteDraw {
                    entity: i as u32 + 1,
                    sheet: sh.clone(),
                    frame: FrameSel::Frame { index: 0, flip: false },
                    x,
                    y,
                    priority: 3,
                    mirror: false,
                    effect: SpriteEffect::None,
                });
            }
        }
        let co_op = self.walkers.len() > 1;
        for (k, w) in self.walkers.iter().enumerate() {
            out.cameras.push(CameraView { x: w.shown.0, y: w.shown.1 });
            // local co-op as in the battle view (wf-core view.rs): each player's slot and its "1P" / "2P" tag over
            // the head (about 32 px above the anchor), so each viewport shows its own player's HUD in place
            out.hud.push(HudView {
                name: self.name.clone(),
                level: self.level,
                hp: self.hp,
                hp_max: self.hp,
                ep: self.ep,
                ep_max: self.ep,
                xp72: 0,
                skill: self.skill,
                slot: k as u8,
                tag: co_op.then(|| ((w.x >> 8) + 8, (w.y >> 8) - 32)),
                ..Default::default()
            });
        }
        let _ = GBA_W;
    }

    fn frame(&self) -> u64 {
        self.frame
    }

    fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.frame.hash(&mut h);
        self.walkers.hash(&mut h);
        h.finish()
    }

    fn players(&self) -> usize {
        self.walkers.len()
    }
}
