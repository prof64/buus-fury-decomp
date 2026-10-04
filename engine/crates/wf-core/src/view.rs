//! The frontend contract (`wf-view::Sim`) for a [`Session`]: inputs in, events out, and a read-only `RenderView`
//! built from the world state (entities in draw order, per-player cameras, HUD values). Rendering never touches
//! simulation state; nothing here needs a window or an audio device.
//!
//! Draw rules follow the reference sim's: actors show their sprite step (players: the state-driven frame, enemies:
//! the time-driven step) with the header shadow; effect objects show their record; projectiles show the record their
//! class uses where it is a plain record (beam piece chains are not drawn yet).

use crate::battle::Outcome;
use crate::fx::Fx;
use crate::session::Session;
use crate::shots::{ShotKind, SName};
use crate::world::{Body, Event, World};
use wf_view::{AudioEvent, CameraView, FrameSel, HudView, MapView, RenderView, Sim, SimEvent, SpriteDraw, SpriteEffect};

fn draw_key(w: &World, i: usize) -> (i32, i32, u32) {
    let e = &w.ents[i];
    let (layer, depth) = match &e.body {
        Body::Player(p) => (0, p.a.py()),
        Body::Enemy(n) => {
            let d = if n.part { w.enemy_by_id(n.parent).map(|(_, p)| p.a.py() + 1).unwrap_or(n.a.py()) } else { n.a.py() };
            (0, d)
        }
        Body::Hand(h) => (0, h.a.py()),
        Body::Rock { y, .. } => (0, y + 40),
        Body::Shot(s) => (if matches!(s.kind, ShotKind::DarkSphere { .. }) { 2 } else { 1 }, s.py()),
        Body::Fx(f) => match f.as_ref() {
            Fx::Anim { y, .. } | Fx::Loop { y, .. } => (1, *y),
            Fx::Debris { y, .. } => (1, (y >> 8) + 8),
            Fx::ItMarkers { .. } => (3, 0),
            _ => (1, 0),
        },
        Body::Injector(_) | Body::Breakable(_) => (9, 0),
        Body::Pickup(p) => (0, p.bx[3]),
        Body::MapSprite(m) => (0, m.y),
        Body::Npc(n) => (0, n.a.py()),
        Body::Mod(m) => (1, m.y >> 8),
    };
    (layer, depth, e.id)
}

fn actor_draw(w: &World, id: u32, a: &crate::actor::Actor, step: Option<(u16, bool, Option<(i32, i32)>)>, out: &mut RenderView) {
    let Some(sp) = a.sprite.and_then(|s| w.data.sprite(s)) else { return };
    let effect = if a.flash > 0 && a.flash & 2 != 0 { SpriteEffect::White } else if a.alpha < 256 { SpriteEffect::Blend { eva: (a.alpha.clamp(0, 256) >> 4) as u8, evb: (16 - (a.alpha.clamp(0, 256) >> 4)) as u8 } } else { SpriteEffect::None };
    if let Some((index, flip, attach)) = step {
        // OAM order of an actor: halo, body, shadow
        if let Some((hx, hy)) = attach {
            out.sprites.push(SpriteDraw { entity: id, sheet: "effects/halo".into(), frame: FrameSel::Frame { index: 0, flip: false }, x: a.px() + hx, y: a.py() + a.z + hy, priority: 3, mirror: false, effect: SpriteEffect::None });
        }
        out.sprites.push(SpriteDraw { entity: id, sheet: format!("sprites/{}", sp.id), frame: FrameSel::Frame { index, flip }, x: a.px(), y: a.py() + a.z, priority: 3, mirror: false, effect });
    }
    // a 0 x 0 header shadow is no shadow (Yemma's office NPCs requested effects/shadow_0x0)
    if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
        out.sprites.push(SpriteDraw { entity: id, sheet: format!("effects/{sh}"), frame: FrameSel::Frame { index: 0, flip: false }, x: a.px(), y: a.py(), priority: 3, mirror: false, effect: SpriteEffect::None });
    }
}

fn record(w: &World, id: u32, addr: u32, x: i32, y: i32, out: &mut RenderView) {
    if let Some((eff, idx)) = w.data.record_frames.get(&addr) {
        out.sprites.push(SpriteDraw { entity: id, sheet: format!("effects/{eff}"), frame: FrameSel::Frame { index: *idx, flip: false }, x, y, priority: 3, mirror: false, effect: SpriteEffect::None });
    }
}
fn effect(id: u32, run: u32, index: i32, x: i32, y: i32, out: &mut RenderView) {
    out.sprites.push(SpriteDraw { entity: id, sheet: format!("effects/fx_{run:08x}"), frame: FrameSel::Frame { index: index.max(0) as u16, flip: false }, x, y, priority: 3, mirror: false, effect: SpriteEffect::None });
}

/// fill `out` from the world (read-only)
pub fn render_view(w: &World, out: &mut RenderView) {
    out.clear();
    out.frame = w.frame as u64;
    let map_id = match &w.field {
        Some(f) => f.map.clone(),
        None => w.data.areas.get(&w.arena.map).map(|a| a.0.clone()).unwrap_or_default(),
    };
    let mode7 = w.field.as_ref().and_then(|f| f.flight.as_ref().map(|fl| wf_view::Mode7View { x: fl.cam[0], y: fl.cam[1], height: fl.cam[2], heading: fl.cam[3], pitch: fl.cam[4], label: f.label.map(|l| l as i32) }));
    // tile animations run on the map's own timers, which start at the area load (field_parity)
    let anim_frame = w.field.as_ref().map(|f| w.frame.saturating_sub(f.entered)).unwrap_or(w.frame);
    out.map = Some(MapView { id: map_id, anim_frame, shake: w.g.shake_off, mode7: mode7.clone() });
    // the area transition's palette fade (field FadeOut k -> level 8k, FadeIn k -> 256 - 8k; 0x08006D98 / 0x08006DD6)
    if let Some(f) = w.field.as_ref() {
        let level = match f.mode {
            crate::field::Mode::FadeOut { k, step, .. } => crate::field::fade_out_level(k, step),
            crate::field::Mode::FadeIn { k } => crate::field::fade_in_level(w, k),
            crate::field::Mode::Normal => 0,
        };
        if level > 0 {
            out.palette_fade = Some(wf_view::PaletteFade { level: level as u16, target: 0 });
        }
    }
    // menu screens (menu/mod.rs): the field fading out / in around them, or the menu screen itself
    if let Some(level) = crate::menu::field_fade(w) {
        out.palette_fade = Some(wf_view::PaletteFade { level, target: 0 });
    }
    out.menu = crate::menu::view(w);
    // the world map: script prop visibility against the content's flags, the mini-map box (spawn_box); the BG
    // camera and the flyer come from `Presenter` / the renderer
    if let (Some(f), Some(_)) = (w.field.as_ref(), mode7.as_ref()) {
        let md = w.data.field.as_ref().and_then(|d| d.maps.get(&f.map));
        let mut wm = wf_view::WorldMapView::default();
        for (k, p) in f.props.iter().enumerate() {
            let def = md.and_then(|m| m.props.get(k)).map(|d| d.visible).unwrap_or(true);
            if p.visible && !def {
                wm.props_shown.push(k as u16);
            } else if !p.visible && def {
                wm.props_hidden.push(k as u16);
            }
        }
        wm.minimap_box = w.scene.visuals.iter().rev().find_map(|v| match v.kind {
            crate::scene::VisualKind::Box { w: bw, h: bh } => Some([v.x, v.y, bw, bh]),
            _ => None,
        });
        out.world_map = Some(wm);
    }
    if let Some((text, t)) = w.scene.banner.as_ref().filter(|b| b.1 <= 238) {
        out.banner = Some(wf_view::BannerView { text: text.clone(), t: *t });
    }
    // the draw order: the world-list objects by (layer, depth) - the actors and the field's props (each prop its own
    // object, `fieldview::prop_keys`); at an equal key a prop is behind
    let mut order: Vec<((i32, i32, u32), Option<usize>, usize)> = (0..w.ents.len()).filter(|&i| !w.ents[i].removed && !crate::vram::suspended(w, w.ents[i].id)).map(|i| (draw_key(w, i), Some(i), 0)).collect();
    order.extend(crate::fieldview::prop_keys(w).into_iter().map(|(k, l, d)| ((l, d, 0), None, k)));
    order.sort_by_key(|o| o.0);
    // field: the debris chips, the item-get icon (front-most), then the actors and props
    crate::fieldview::front(w, out);
    // OAM order: front-most first
    for &(_, ent, prop) in order.iter().rev() {
        let Some(i) = ent else {
            crate::fieldview::prop(w, prop, out);
            continue;
        };
        let e = &w.ents[i];
        let id = e.id;
        match &e.body {
            Body::Player(p) => {
                // a script's puppet plays the scene animation instead of the player state's
                let (raw, frame) = match crate::scene::scene_anim(w, i) {
                    Some(a) => (a.raw, a.frame),
                    None => (p.ap.raw, p.ap.frame),
                };
                // Majin Buu's candy: the def is 0x36 (sprite 45, the sweets); eaten: not in the world
                let mut a = std::borrow::Cow::Borrowed(&p.a);
                if p.candy_def {
                    a.to_mut().sprite = Some(45);
                }
                let step = a.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| sp.anim_raw(raw).or_else(|| sp.anim_raw(1))).and_then(|an| {
                    let st = an.steps(a.facing);
                    st.get((frame.max(0) as usize).min(st.len().saturating_sub(1))).map(|s| (s.frame, s.flip, s.attach))
                });
                if p.a.alpha > 0 && !p.gone {
                    actor_draw(w, id, &a, step, out);
                }
            }
            Body::Enemy(n) => {
                let step = n.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| n.a.anim.and_then(|k| sp.anims.get(k as usize))).and_then(|a| {
                    let looped = n.a.anim_loop.unwrap_or(a.looped);
                    a.step_index_at(n.a.facing, n.a.anim_t, looped).map(|k| (a.steps(n.a.facing)[k].frame, a.steps(n.a.facing)[k].flip, a.steps(n.a.facing)[k].attach))
                });
                if n.a.alpha > 0 {
                    actor_draw(w, id, &n.a, step, out);
                }
            }
            Body::Hand(h) => {
                let step = h.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| h.a.anim.and_then(|k| sp.anims.get(k as usize))).and_then(|a| a.step_index_at(h.a.facing, h.a.anim_t, false).map(|k| (a.steps(h.a.facing)[k].frame, a.steps(h.a.facing)[k].flip, None)));
                actor_draw(w, id, &h.a, step, out);
            }
            Body::Rock { x, y, hp } => effect(id, 0x0875_f7ac, if *hp >= 2 { 0 } else { 1 }, *x, *y, out),
            Body::Fx(f) => match f.as_ref() {
                Fx::Anim { id: run, x, y, rec, .. } | Fx::Loop { id: run, x, y, rec, .. } => effect(id, *run, *rec, *x, *y, out),
                Fx::Debris { x, y, .. } => out.sprites.push(SpriteDraw { entity: id, sheet: "effects/debris".into(), frame: FrameSel::Frame { index: 0, flip: false }, x: x >> 8, y: y >> 8, priority: 3, mirror: false, effect: SpriteEffect::None }),
                Fx::DragonFist { seg, dir, dying, .. } => {
                    for k in 0..6 {
                        let ix = if k != 0 { 0 } else if *dying < 0 { 1 } else { 1 + (dying >> 3).min(2) };
                        record(w, id, 0x0877_ad10 + 12 * (*dir as u32 * 4 + ix as u32), seg[k][0] >> 8, seg[k][1] >> 8, out);
                    }
                }
                _ => {}
            },
            Body::Shot(s) => {
                let (x, y, t, d) = (s.px(), s.py(), s.t as i32, s.dir as i32);
                match &s.kind {
                    ShotKind::Straight(o) => {
                        let (run, idx) = match o.name {
                            SName::KiBlast => {
                                let f = t >> 3;
                                (0x0871_370c, if o.dead >= 0 { 16 } else { d * 4 + if f > 3 { 2 + (f & 1) } else { f } })
                            }
                            SName::Gun => (0x0874_e968, d + 4 * [0, 1, 2, 1][((t >> 2) & 3) as usize]),
                            SName::Rifle => (0x0874_e614, d),
                            SName::Shell => (0x0871_e630, d),
                            SName::Star => (0x0875_4a1c, (t >> 3) & 1),
                            SName::Wrap | SName::WrapDiag => (0x0875_71f4, 3 * (d >> 1) + ((t % 24) >> 3) + if o.vx != 0 && o.vy != 0 { 6 } else { 0 }),
                            SName::Spit => (0x0875_79b8, o.sector + ((t >> 2) & 1)),
                            SName::Barrage => (0x0872_d6b8, 5 * d + (t >> 2).min(4)),
                            SName::Burning => (0x0875_01ec, if o.dead >= 0 { 2 + ((t >> 3) & 1) } else { (t >> 3) & 1 }),
                            SName::Volley => (0x0876_e264, 3 * d + 2),
                        };
                        effect(id, run, idx, x, y, out);
                    }
                    ShotKind::Flame { .. } => {
                        if (t >> 2) <= 4 {
                            record(w, id, 0x0872_d6b8 + 60 * d as u32 + 12 * (t >> 2) as u32, x, y, out);
                        }
                    }
                    ShotKind::Bomb { .. } => {
                        let mut k = (t >> 2) & 3;
                        if k == 3 {
                            k = 1;
                        }
                        record(w, id, 0x0871_e984 + 12 * k as u32, x, y, out);
                    }
                    // a flying mummy bandage (records 0x087571F4: straight 3 * (dir >> 1) + tick >> 3, diagonal + 6); stuck
                    // ones are drawn with the player
                    ShotKind::Wrap(m) if !m.stuck => effect(id, 0x0875_71f4, (if m.diag { 6 } else { 0 }) + 3 * (d >> 1) + (m.tick >> 3), x, y, out),
                    ShotKind::SjWave => record(w, id, 0x0875_a6a8 + 12 * d as u32, x, y, out),
                    _ => {}
                }
            }
            Body::Npc(n) => {
                let step = n.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| sp.anim_raw(n.anim.raw).or_else(|| sp.anim_raw(1))).and_then(|a| {
                    let st = a.steps(n.a.facing);
                    st.get((n.anim.frame.max(0) as usize).min(st.len().saturating_sub(1))).map(|s| (s.frame, s.flip, s.attach))
                });
                actor_draw(w, id, &n.a, step, out);
            }
            Body::Pickup(p) => crate::fieldview::pickup(w, id, p, out),
            Body::MapSprite(m) => crate::fieldview::map_sprite(w, id, m, out),
            Body::Injector(_) | Body::Breakable(_) => {}
            Body::Mod(m) => crate::modkit::draw(w, id, m, out),
        }
    }
    // cutscene presentation: fade, dialog box
    out.fade_black = crate::scene::fade_black(w);
    if let Some(d) = &w.scene.dialog {
        if d.phase != crate::scene::DialogPhase::Closed {
            let text_box = d.kind != 3;
            out.dialog = Some(wf_view::DialogView {
                text: d.text.clone(),
                // arg 7: the 2-line style (0x08012012 -> 0x0800B188 style index)
                style: (text_box && d.portrait == 7) as u8,
                page: d.page,
                position: match d.pos {
                    crate::scene::BoxPos::Top => wf_view::DialogPos::Top,
                    crate::scene::BoxPos::Middle => wf_view::DialogPos::Middle,
                    crate::scene::BoxPos::Bottom => wf_view::DialogPos::Bottom,
                },
                portrait: if text_box { dialog_portrait(w, d.portrait) } else { None },
                portrait_left: d.portrait_left,
            });
        }
    }
    out.dialog_anim = dialog_anim(w);
    for s in 0..w.players.len() {
        let c = w.cameras[s.min(crate::world::MAX_PLAYERS - 1)].shown;
        out.cameras.push(CameraView { x: c.x0, y: c.y0 });
        if let Some(pi) = w.player_idx(s) {
            let p = w.pl_ref(pi);
            let name = w.data.form(p.form).map(|f| f.name.clone()).unwrap_or_default();
            // experience line: steps 0..72 between this level's total and the next (0x0801363E(player, 72))
            let lv = p.rec.level;
            let t = &w.data.xp_table;
            let k = (lv - 50).max(0) as usize;
            let xp72 = match (t.get(k), t.get(k + 1)) {
                (Some(&a), Some(&b)) if b > a => (((p.rec.xp - a).clamp(0, b - a) * 72) / (b - a)) as u8,
                (Some(_), None) => 72,
                _ => 0,
            };
            let skill = p.skill();
            out.hud.push(HudView {
                name,
                level: lv.max(0) as u32,
                hp: p.hp(),
                hp_max: p.max_hp(),
                ep: p.ep(),
                ep_max: p.max_ep(),
                ss: p.ss_gauge,
                ss3: (p.ss3_gauge >> 16) as i32,
                xp72,
                skill: (skill > 0).then_some(skill as u8),
                // areas without combat (towns, offices) grey the icon out (emulator HUD)
                skill_alt: !crate::player::can_use_skill(w, pi, skill) || w.field.as_ref().is_some_and(|f| !f.combat),
                hidden: w.scene.hud_hidden,
                stat_mark: stat_mark_frame(w),
            });
        }
    }
}

/// the stat-points mark's animation frame (draw 0x0800B55C: (counter >> 3) & 3), while it exists
fn stat_mark_frame(w: &World) -> Option<u8> {
    let v = w.field.as_ref()?.vram.as_ref()?;
    if !v.owners.iter().any(|o| o.1 == crate::vram::Owner::StatMark) {
        return None;
    }
    Some(((w.frame.wrapping_sub(v.stat_at) >> 3) & 3) as u8)
}

/// the frame's events as frontend events
pub fn sim_events(w: &World, out: &mut Vec<SimEvent>) {
    for e in &w.events {
        match e {
            Event::Sfx { id, vol } => out.push(SimEvent::Audio(AudioEvent::PlaySfx { id: *id as u8, volume: (*vol).clamp(0, 64) as u8, pan: 0 })),
            Event::SfxStop { id } => out.push(SimEvent::Audio(AudioEvent::StopSfx(*id as u8))),
            Event::Song { id } => out.push(SimEvent::Audio(AudioEvent::PlaySong(*id as u16))),
            Event::SfxStopAll => out.push(SimEvent::Audio(AudioEvent::StopAllSfx)),
            Event::SongStop => out.push(SimEvent::Audio(AudioEvent::StopSong)),
            Event::MasterVolumes { music, sfx } => out.push(SimEvent::Audio(AudioEvent::SetMasterVolumes { music: *music as u8, sfx: *sfx as u8 })),
            _ => {}
        }
    }
}

/// The screen's timing around a step (verified frame by frame against the emulator, docs/frontend.md): the picture
/// shows the previous frame's OBJ build (sprites, HUD, banner, dialogue box: shadow OAM / OBJ tiles are copied at
/// the next VBlank) over the current BG scroll and palette; on the 3D world map the ground's scanline table was
/// built from the camera before the step (`WorldMapView.bg_camera`). Call `before_step` before every step and
/// `apply` after `render_view`.
#[derive(Clone, Debug, Default)]
pub struct Presenter {
    #[allow(clippy::type_complexity)]
    prev: Option<(Vec<SpriteDraw>, Vec<HudView>, Option<wf_view::BannerView>, Option<wf_view::DialogView>, Option<wf_view::DialogAnim>)>,
    prev_mode7: Option<wf_view::Mode7View>,
}

impl Presenter {
    pub fn before_step(&mut self, w: &World) {
        let mut v = RenderView::default();
        render_view(w, &mut v);
        self.prev_mode7 = v.map.as_ref().and_then(|m| m.mode7.clone());
        self.prev = Some((v.sprites, v.hud, v.banner, v.dialog, v.dialog_anim));
    }
    /// the session started mid-area (harnesses): nothing of the previous build to show
    pub fn reset(&mut self) {
        *self = Presenter::default();
    }
    pub fn apply(&self, out: &mut RenderView) {
        if let Some((sprites, hud, banner, dialog, anim)) = &self.prev {
            out.sprites = sprites.clone();
            out.hud = hud.clone();
            out.banner = banner.clone();
            out.dialog = dialog.clone();
            // the box OBJs are a frame late; the text surface is redrawn and uploaded every frame (the typewriter
            // shows the current count)
            let typed = out.dialog_anim.and_then(|a| a.typed);
            out.dialog_anim = anim.map(|mut a| {
                if a.typed.is_some() {
                    a.typed = typed.or(a.typed);
                }
                a
            });
        }
        if let (Some(wm), Some(c)) = (out.world_map.as_mut(), self.prev_mode7.clone()) {
            wm.bg_camera = Some(c);
        }
    }
}

/// The dialogue box animation from the dialog state (measured against the emulator's OAM, affine PA = 2048 /
/// scale, and screen; wf-app field_parity): the open zoom over the `DIALOG_OPEN` frames (scale = t + 2, 1..8),
/// the typewriter while shown, the close zoom (8 - t).
/// a text node's `arg` as a `portraits/` portrait_index (`0x08012012`): 0 / 7 none; 1..6 the leader's form slot
/// (`Form::portraits[arg - 1]`, global table index = portrait + 8); >= 8 portrait arg - 8
pub fn dialog_portrait(w: &World, arg: u8) -> Option<u16> {
    let global = match arg {
        0 | 7 => return None,
        1..=6 => {
            let form = w.player_idx(0).map(|i| w.pl_ref(i).form).or_else(|| w.story.chars.get(w.story.current.max(0) as usize).map(|c| c.form))?;
            w.data.form(form)?.portraits[arg as usize - 1]
        }
        a => a,
    };
    global.checked_sub(8).map(u16::from)
}

pub fn dialog_anim(w: &World) -> Option<wf_view::DialogAnim> {
    use crate::scene::DialogPhase as P;
    let d = w.scene.dialog.as_ref()?;
    let (scale, typed) = match d.phase {
        P::Opening => ((d.t + 2).clamp(1, 8), Some(0)),
        P::Shown => (8, Some(d.typed.max(0) as u32)),
        P::Closing => (8 - d.t, None),
        P::Closed => return None,
    };
    if scale < 1 {
        return Some(wf_view::DialogAnim { scale: 0xFF, typed });
    }
    Some(wf_view::DialogAnim { scale: scale.min(8) as u8, typed })
}

impl Sim for Session {
    fn step(&mut self, inputs: &[u16], events: &mut Vec<SimEvent>) {
        if self.world.frame == 0 {
            let song = self.world.data.areas.get(&self.world.arena.map).and_then(|a| a.1);
            events.push(SimEvent::Audio(match song {
                Some(s) => AudioEvent::PlaySong(s),
                None => AudioEvent::StopSong,
            }));
        }
        if self.world.field.is_some() {
            self.presenter.before_step(&self.world);
        }
        Session::step(self, inputs);
        sim_events(&self.world, events);
    }
    fn view(&self, out: &mut RenderView) {
        render_view(&self.world, out);
        if self.world.field.is_some() {
            self.presenter.apply(out);
        }
    }
    fn frame(&self) -> u64 {
        self.world.frame as u64
    }
    fn state_hash(&self) -> u64 {
        self.world.state_hash()
    }
    fn snapshot(&self) -> Option<Vec<u8>> {
        Some(self.snapshot_bytes())
    }
    fn restore(&mut self, s: &[u8]) -> bool {
        self.restore_bytes(s)
    }
    fn players(&self) -> usize {
        self.world.players.len().max(1)
    }
}

impl Session {
    /// prev buttons (4 x u16) + outcome + the world
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        for p in self.prev {
            v.extend_from_slice(&p.to_le_bytes());
        }
        v.push(match self.outcome {
            Outcome::Running => 0,
            Outcome::Won => 1,
            Outcome::Lost => 2,
        });
        v.extend(crate::snapshot::to_bytes(&self.world));
        v
    }
    pub fn restore_bytes(&mut self, s: &[u8]) -> bool {
        let n = 2 * self.prev.len();
        if s.len() < n + 1 {
            return false;
        }
        let Some(w) = crate::snapshot::from_bytes(&self.data, &s[n + 1..]) else { return false };
        for (k, p) in self.prev.iter_mut().enumerate() {
            *p = u16::from_le_bytes([s[2 * k], s[2 * k + 1]]);
        }
        self.outcome = match s[n] {
            1 => Outcome::Won,
            2 => Outcome::Lost,
            _ => Outcome::Running,
        };
        self.world = w;
        true
    }
}
