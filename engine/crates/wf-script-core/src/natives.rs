//! The 248 natives, setters 8..17 and event nodes on top of `World.story` / `World.scene` / `World.field`.
//! Semantics: `re/buu/SCRIPT.md` §9 (one row per native); which system each one uses: `binding.rs`.

use crate::text::{self, Handle};
use crate::{char_of_form, char_sprite, count_actors, current_form, leader, resolve_actor, CoreHost, DropKind, DropRequest, HostState, Pending, Phase, Present, SavedField};
use wf_core::input::A;
use wf_core::scene::{self, BeamKind, Cmd, FadeKind, MenuKind, Owner, ShootKind, Visual, VisualKind};
use wf_core::world::{Body, Event, World};
use wf_script::vm::{Call, ExtCall, NativeResult, NodeCall};

type R = Result<NativeResult, String>;

fn arg(c: &[i32], i: usize) -> i32 {
    c.get(i).copied().unwrap_or(0)
}

fn done(v: i32) -> R {
    Ok(NativeResult::Done(Some(v)))
}

fn unit() -> R {
    Ok(NativeResult::Done(None))
}

fn block(s: &mut HostState, p: Pending) -> R {
    s.pending = Some(p);
    Ok(NativeResult::Block)
}

fn lead(w: &World, n: u8) -> Result<usize, String> {
    leader(w).ok_or_else(|| format!("native 0x{n:02x}: no player in the World"))
}

fn leader_char(w: &World) -> i32 {
    leader(w).and_then(|i| char_of_form(w, w.pl_ref(i).form)).unwrap_or(w.story.current)
}

/// the live fighter playing character `c`
fn fighter_of(w: &World, c: i32) -> Option<usize> {
    w.players.iter().filter_map(|&id| w.find(id)).find(|&i| char_of_form(w, w.pl_ref(i).form) == Some(c))
}

/// a queued actor command: the track of the resolved actor (dropped when it resolves to nothing); blocks unless a
/// batch is open
fn enqueue(h: &mut CoreHost, actor: i32, cmd: Cmd) -> R {
    let owner = resolve_actor(h.w, h.s, actor).map(|i| Owner::Entity(h.w.ents[i].id));
    enqueue_owner(h, owner, cmd)
}

fn enqueue_owner(h: &mut CoreHost, owner: Option<Owner>, cmd: Cmd) -> R {
    let blocks = match owner {
        Some(o) => scene::enqueue(h.w, o, cmd),
        // the game deletes the command and returns at once (micro dropped_cmd: no frame passes)
        None => false,
    };
    if blocks {
        block(h.s, Pending::Director)
    } else {
        unit()
    }
}

fn entity_of(h: &CoreHost, actor: i32) -> Option<wf_core::world::EntityId> {
    resolve_actor(h.w, h.s, actor).map(|i| h.w.ents[i].id)
}

/// a sprite argument as `0x08000158` resolves it: 0 the player's, 1..8 character id-1's current form, else the id
fn sprite_of(w: &World, id: i32) -> i32 {
    match id {
        0 => leader(w).and_then(|i| scene::sprite_id(w, i)).unwrap_or(0),
        1..=8 => char_sprite(w, id - 1).unwrap_or(id),
        _ => id,
    }
}

fn mv(x: i32, y: i32, speed: i32, state: i32) -> Cmd {
    Cmd::MoveTo { x, y, speed, state, target: None, n: 0, t: 0, sx: 0, sy: 0 }
}

pub fn call(h: &mut CoreHost, c: &Call) -> R {
    let n = c.native;
    let a = c.args;
    if std::env::var_os("T_NAT").is_some() {
        eprintln!("  t-nat f{} 0x{n:02x} {:?}", h.w.frame, &a[..4]);
    }
    match n {
        0x00..=0x07 => Err(format!("native 0x{n:02x} is handled by the VM")),
        0x08..=0x0b => done(h.w.story.l[n as usize - 8]),
        0x0c | 0x0d | 0x0e | 0x0f | 0xc3 | 0xc8 => {
            let i = lead(h.w, n)?;
            let p = h.w.pl_ref(i);
            let f = h.w.data.form(p.form);
            done(match n {
                0x0c => p.rec.hp,
                0x0d => p.rec.max_hp,
                0x0f => p.rec.max_ep,
                0xc3 => p.rec.xp as i32,
                0xc8 => p.rec.level,
                _ => match f {
                    Some(f) if f.ss3 => (p.ss3_gauge >> 16) as i32,
                    Some(f) if f.ss => p.ss_gauge,
                    _ => p.rec.ep,
                },
            })
        }
        0x10 => done(h.w.story.zenie as i32),
        0x11 => done(h.w.story.variant_byte),
        0x12..=0x14 => {
            let (wd, m, v) = area(h);
            done([wd, m, v][(n - 0x12) as usize])
        }
        0x15 => done(leader_char(h.w)),
        0x16 | 0x17 => {
            let i = lead(h.w, n)?;
            let f = h.w.pl_ref(i).form;
            done(if n == 0x16 { Handle::FormName(f) } else { Handle::FormLabel(f) }.encode())
        }
        0x18 => done(h.s.menu_result),
        0x19 => done(h.w.rng.rand(arg(a, 0) as u32)),
        0x1a => done(h.w.rng.chance((arg(a, 0).wrapping_mul(0x28F5D) >> 8) as i64) as i32),
        0x1b => done(h.w.story.flag(arg(a, 0)) as i32),
        0x1c => done(!h.w.story.flag(arg(a, 0)) as i32),
        0x1d | 0x1e => {
            h.w.story.set_flag(arg(a, 0), n == 0x1d);
            unit()
        }
        0x1f => {
            h.w.sfx(arg(a, 0), 64);
            unit()
        }
        0x20 => {
            h.w.sfx(arg(a, 0), arg(a, 1));
            unit()
        }
        0x21 => {
            h.w.sfx_stop(arg(a, 0));
            unit()
        }
        0x22 => {
            h.w.events.push(Event::SfxStopAll);
            unit()
        }
        0x23 => {
            scene::play_song(h.w, arg(a, 0));
            unit()
        }
        0x24 => {
            h.w.events.push(Event::SongStop);
            h.w.scene.song = -1;
            unit()
        }
        0x25 | 0x26 => {
            let l = arg(a, 0) as usize & 3;
            h.w.scene.screen.layers.visible[l] = n == 0x25;
            unit()
        }
        0x27 => {
            h.w.scene.screen.layers.priority[arg(a, 0) as usize & 3] = Some((arg(a, 1) & 3) as u8);
            unit()
        }
        0x28 => {
            h.w.scene.screen.layers.blend[arg(a, 0) as usize & 3] = Some((arg(a, 1) & 3) as u8);
            unit()
        }
        0x29 => {
            h.w.scene.screen.layers.offset[arg(a, 0) as usize & 3] = (arg(a, 1), arg(a, 2));
            unit()
        }
        0x2a => {
            let k = |p: i32| ((p.wrapping_mul(0x290)) >> 12).clamp(0, 31) as u8;
            h.w.scene.screen.layers.alpha = Some((k(arg(a, 0)), k(arg(a, 1))));
            unit()
        }
        0x2b..=0x2e => {
            let r = [arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3)];
            scene::set_area(h.w, r, n <= 0x2c, n == 0x2b || n == 0x2d);
            unit()
        }
        0x2f | 0x30 => {
            let was = h.w.scene.hud_hidden;
            h.w.scene.hud_hidden = n == 0x30;
            if was != h.w.scene.hud_hidden {
                wf_core::vram::hud_shown(h.w, n == 0x2f);
            }
            unit()
        }
        0x31 => {
            // a missing actor would scroll toward garbage forever on the GBA; here the scroll is skipped
            let Some(i) = resolve_actor(h.w, h.s, arg(a, 0)) else { return unit() };
            let (x, y) = scene::center_on(h.w, i); if std::env::var("DBG").is_ok() { eprintln!("center_on actor {} -> ent {} sprite {:?} pos {:?} foot {:?} target ({x},{y}) cam ({},{})", arg(a,0), h.w.ents[i].id, scene::sprite_id(h.w, i), h.w.actor(i).map(|a| (a.px(), a.py())), h.w.actor(i).map(|a| (a.foot_w, a.foot_h)), h.w.cameras[0].x, h.w.cameras[0].y); }
            scene::start_scroll(h.w, x, y, arg(a, 1));
            block(h.s, Pending::Scroll)
        }
        0x32 => {
            scene::start_scroll(h.w, arg(a, 0), arg(a, 1), arg(a, 2));
            block(h.s, Pending::Scroll)
        }
        0x33 => {
            if let Some(id) = entity_of(h, arg(a, 0)) {
                scene::follow(h.w, id);
            }
            unit()
        }
        0x34 => {
            h.w.scene.cam.mode = scene::CamMode::Fixed;
            unit()
        }
        0x35 => {
            let sprite = sprite_of(h.w, arg(a, 0));
            scene::spawn_npc(h.w, sprite, arg(a, 1), arg(a, 2), 0);
            unit()
        }
        0x36 | 0x8a | 0xa9 | 0xaa => {
            let (list, idx) = match n {
                0x36 => (1u8, arg(a, 0)),
                0x8a => (0, arg(a, 0)),
                _ => (3, arg(a, 0)),
            };
            if n == 0xaa && h.w.field.is_some() {
                // spawn_map_item: the item is thrown from the loot block (fieldobj::throw_map_item)
                let at = wf_core::fieldcontent::ObjRef::List(list, idx as u16);
                wf_core::field::throw_map_item(h.w, at);
            } else if n == 0xa9 && h.w.field.is_some() {
                // spawn_map_item_at (0x0800262A): the same dropped-item constructor (4 RNG draws) from the position
                // (x, y) with the zero velocity 0x08054FF8; the loot block is not touched (13-53: the 6-Star ball)
                let at = wf_core::fieldcontent::ObjRef::List(list, idx as u16);
                let keep = h.w.field.as_ref().map(|f| f.loot);
                wf_core::fieldobj::set_loot(h.w, arg(a, 1), arg(a, 2), 0, 0);
                wf_core::field::throw_map_item(h.w, at);
                if let (Some(f), Some(l)) = (h.w.field.as_mut(), keep) {
                    f.loot = l;
                }
            } else if h.w.field.is_some() {
                let at = wf_core::fieldcontent::ObjRef::List(list, idx as u16);
                let id = wf_core::field::spawn_map_object(h.w, at, true);
                if n == 0xa9 {
                    if let Some(k) = id.and_then(|id| h.w.find(id)) {
                        if let Body::Pickup(p) = &mut h.w.ents[k].body {
                            p.x = arg(a, 1);
                            p.y = arg(a, 2);
                        }
                    }
                }
            }
            unit()
        }
        0x37 => done(count_actors(h.w, h.s, arg(a, 0))),
        0x38 => {
            scene::begin_batch(h.w);
            unit()
        }
        0x39 => {
            if let Some(id) = entity_of(h, arg(a, 0)) {
                scene::batch_group(h.w, Owner::Entity(id));
            }
            unit()
        }
        0x3a => {
            scene::commit(h.w);
            block(h.s, Pending::Director)
        }
        0x3b => enqueue(h, arg(a, 0), Cmd::Beam { kind: BeamKind::Wave, dir: arg(a, 1), frames: arg(a, 2), t: 0 }),
        0x3c => enqueue(h, arg(a, 0), Cmd::PlayAnim { anim: arg(a, 1) }),
        0x3d => enqueue(h, arg(a, 0), Cmd::SetAnim { anim: arg(a, 1) }),
        0x3e => enqueue(h, arg(a, 0), Cmd::Shoot { kind: ShootKind::Ball, x: arg(a, 1), y: arg(a, 2), speed: 0, t: 0, n: 0 }),
        0x3f => enqueue(h, arg(a, 0), Cmd::Puff { x: 0, y: 0, absolute: false }),
        0x40 => enqueue(h, arg(a, 0), Cmd::Puff { x: arg(a, 1), y: arg(a, 2), absolute: false }),
        0x41 => {
            let Some(t) = resolve_actor(h.w, h.s, arg(a, 1)) else { return enqueue_owner(h, None, Cmd::HitFlash) };
            let (x, y) = scene::target_center(h.w, t);
            enqueue(h, arg(a, 0), Cmd::Shoot { kind: ShootKind::Blast, x, y, speed: arg(a, 2), t: 0, n: 0 })
        }
        0x42 => enqueue(h, arg(a, 0), Cmd::Shoot { kind: ShootKind::Blast, x: arg(a, 1), y: arg(a, 2), speed: arg(a, 3), t: 0, n: 0 }),
        0x43 => enqueue(h, arg(a, 0), Cmd::HitFlash),
        0x44 => enqueue(h, arg(a, 0), mv(arg(a, 1), arg(a, 2), arg(a, 3), 0x0f)),
        0x45 => enqueue(h, arg(a, 0), Cmd::JumpTo { x: arg(a, 1), y: arg(a, 2), air: arg(a, 3), t: 0, sx: 0, vy: 0 }),
        0x46 => enqueue(h, arg(a, 0), Cmd::Beam { kind: BeamKind::Beam, dir: arg(a, 1), frames: arg(a, 2), t: 0 }),
        0x47 => {
            let Some(t) = entity_of(h, arg(a, 1)) else { return enqueue_owner(h, None, Cmd::HitFlash) };
            enqueue(h, arg(a, 0), Cmd::FaceActor { target: t })
        }
        0x48 => enqueue(h, arg(a, 0), Cmd::Puff { x: arg(a, 1), y: arg(a, 2), absolute: true }),
        0x49 => enqueue(h, arg(a, 0), Cmd::GlideTo { x: arg(a, 1), y: arg(a, 2), speed: arg(a, 3), n: 0, t: 0, sx: 0, sy: 0 }),
        0x4a => enqueue(h, arg(a, 0), Cmd::Wait { left: arg(a, 1) }),
        0x4b => enqueue(h, arg(a, 0), Cmd::Sfx { id: arg(a, 1) }),
        0x4c => enqueue(h, arg(a, 0), Cmd::ScreenFlash { r: arg(a, 1), g: arg(a, 2), b: arg(a, 3), frames: arg(a, 4), t: 0 }),
        0x4d => enqueue(h, arg(a, 0), Cmd::Remove),
        0x4e => {
            let sprite = sprite_of(h.w, arg(a, 1));
            enqueue(h, arg(a, 0), Cmd::ChangeSprite { sprite })
        }
        0x4f => enqueue(h, arg(a, 0), Cmd::BecomeMapObject { index: arg(a, 1) }),
        0x50 => enqueue(h, arg(a, 0), mv(arg(a, 1), arg(a, 2), arg(a, 3), 2)),
        0x51 => enqueue(h, arg(a, 0), Cmd::SetPose { anim: arg(a, 1), frame: arg(a, 2) }),
        0x52 => enqueue(h, arg(a, 0), Cmd::Tint { r: arg(a, 1), g: arg(a, 2), b: arg(a, 3), a: arg(a, 4) }),
        0x53 => enqueue(h, arg(a, 0), Cmd::Face { dir: arg(a, 1) }),
        0x54 => enqueue(h, arg(a, 0), Cmd::SetSpeed { v: arg(a, 1) }),
        0x55 => enqueue(h, arg(a, 0), Cmd::SetAttachment { kind: arg(a, 1), variant: arg(a, 2), dx: arg(a, 3), dy: arg(a, 4) }),
        0x56 => enqueue(h, arg(a, 0), Cmd::SetByte8 { v: arg(a, 1) }),
        0x57 => enqueue(h, arg(a, 0), Cmd::Shake { intensity: arg(a, 1), frames: arg(a, 2), t: 0 }),
        0x58 => enqueue(h, arg(a, 0), Cmd::ItemGet { item: arg(a, 1), t: 0 }),
        0x59 => enqueue(h, arg(a, 0), Cmd::PlayAnimFrames { anim: arg(a, 1), each: arg(a, 2), loops: arg(a, 3), wait: arg(a, 4) != 0, loop_start: arg(a, 5), last: arg(a, 6) }),
        0x5a | 0x5b => enqueue(h, arg(a, 0), Cmd::Emote { emote: arg(a, 1), wait: n == 0x5a, t: 0 }),
        0x5c => enqueue(h, arg(a, 0), Cmd::Beam { kind: BeamKind::BeamAlt, dir: arg(a, 1), frames: arg(a, 2), t: 0 }),
        0x5d => enqueue(h, arg(a, 0), Cmd::SetPos { x: arg(a, 1), y: arg(a, 2) }),
        0x5e => enqueue(h, arg(a, 0), Cmd::Beam { kind: BeamKind::Blast, dir: arg(a, 1), frames: arg(a, 2), t: 0 }),
        0x5f => {
            let t = entity_of(h, arg(a, 1));
            let cmd = Cmd::MoveTo { x: 0, y: 0, speed: 100, state: 1, target: t.map(|t| (t, arg(a, 2), arg(a, 3))), n: 0, t: 0, sx: 0, sy: 0 };
            if t.is_none() {
                return enqueue_owner(h, None, cmd);
            }
            enqueue(h, arg(a, 0), cmd)
        }
        0x60 => enqueue(h, arg(a, 0), mv(arg(a, 1), arg(a, 2), 100, 1)),
        0x61 => enqueue(h, arg(a, 0), mv(arg(a, 1), arg(a, 2), arg(a, 3), 1)),
        0x62 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::PropPoof { vanish: arg(a, 1) != 0, t: 0 }),
        0x63 | 0x64 => {
            // an area without that map prop: no owner object, the command is dropped at once (3-10's on-enter
            // prop_hide(0): the player arrives in the load's frame)
            let id = arg(a, 0);
            let missing = h.w.field.as_ref().is_some_and(|f| id < 0 || id as usize >= f.props.len());
            enqueue_owner(h, (!missing).then_some(Owner::Prop(id)), Cmd::PropShow { on: n == 0x64 })
        }
        0x65 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::Puff { x: arg(a, 1), y: arg(a, 2), absolute: true }),
        0x66 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::PropMoveTo { x: arg(a, 1), y: arg(a, 2), speed: arg(a, 3), n: 0, t: 0, sx: 0, sy: 0 }),
        0x67 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::PropJumpTo { x: arg(a, 1), y: arg(a, 2), t: 0, vx: 0, vy: 0 }),
        0x68 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::Wait { left: arg(a, 1) }),
        0x69 => enqueue_owner(h, Some(Owner::Prop(arg(a, 0))), Cmd::PropSetPos { x: arg(a, 1), y: arg(a, 2), solo: !h.w.scene.director.batch }),
        0x6a => {
            let cmd = match entity_of(h, arg(a, 0)) {
                Some(t) => Cmd::CamFollowWait { target: t, left: arg(a, 1) },
                None => Cmd::CamReleaseWait { left: arg(a, 1) },
            };
            enqueue_owner(h, Some(Owner::Camera), cmd)
        }
        0x6b => enqueue_owner(h, Some(Owner::Camera), Cmd::CamPanTo { x: arg(a, 0), y: arg(a, 1), speed: arg(a, 2), n: 0, t: 0, sx: 0, sy: 0 }),
        0x6c => enqueue_owner(h, Some(Owner::Camera), Cmd::CamReleaseWait { left: arg(a, 0) }),
        0x6d | 0x6e => enqueue_owner(h, Some(Owner::Camera), Cmd::CamParam { which: if n == 0x6d { 8 } else { 12 }, v: arg(a, 0) }),
        0x6f => enqueue_owner(h, Some(Owner::Camera), Cmd::CamSet { x: arg(a, 0), y: arg(a, 1), z: None }),
        0x70 => enqueue_owner(h, Some(Owner::Camera), Cmd::CamSet { x: arg(a, 0), y: arg(a, 1), z: Some(arg(a, 2)) }),
        0x71 => {
            h.s.save_armed = true;
            unit()
        }
        0x72 | 0x74 => {
            // an armed save is taken at the call, before the fade: the field's objects leave the world list for the
            // 64 frames of the fade-out (HFIL 18-2's switch script: world list empty from the call on; sc_hfil)
            if h.s.save_armed && h.w.field.is_some() {
                save_field(h.w, h.s);
                scene::send_offstage(h.w);
                h.w.ents.clear();
                h.w.enemies.clear();
            }
            let (wd, m, v, x, y) = (arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4));
            let list = vec![
                Phase::Fade { kind: FadeKind::Black, out: true, frames: 64 },
                Phase::LoadScene { world: wd, map: m, variant: v, x, y, objects: n == 0x72 },
                Phase::Fade { kind: FadeKind::Black, out: false, frames: 16 },
            ];
            start_phases(h, list)
        }
        0x73 => {
            if let Some(f) = h.w.field.as_mut() {
                f.defer_objects = h.s.nest_area_enter;
            }
            load_scene(h.w, h.s, arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4), true);
            if let Some(f) = h.w.field.as_mut() {
                f.defer_objects = false;
            }
            // the scene's on-enter script runs inside the load (nested): its commands' frames come before the
            // script's next action (s025: 0-16 hides two props before the actors appear)
            if h.s.nest_area_enter && h.w.field.as_ref().is_some_and(|f| f.on_enter_queued) {
                return block(h.s, Pending::Arrival);
            }
            unit()
        }
        0x75 => {
            let list = vec![
                Phase::Fade { kind: FadeKind::Black, out: true, frames: 16 },
                Phase::ReturnToField,
                Phase::Fade { kind: FadeKind::Black, out: false, frames: 16 },
            ];
            start_phases(h, list)
        }
        0x76 | 0x77 => {
            let (wd, m, v, x, y) = (arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4));
            // a warp rebuilds the player and its camera: the script's camera control ends
            // (the fading warp's camera holds until its load: s515's world-map take-off)
            if n == 0x77 {
                h.w.scene.cam.mode = scene::CamMode::Player;
                h.w.scene.cam.scroll = None;
            }
            if h.w.field.is_some() {
                if n == 0x77 {
                    // the immediate warp: the player goes idle first (vt+0x28(0): 1 x rand(255)), then EnterArea
                    // re-creates it facing down (story playthrough: s061 -> Grand Kai's Mansion)
                    // (not the 3D map's flyer: its flight state takes no SetAnim(0) - Earth 3-1 / 3-11, a circle's warp)
                    let flying = wf_core::fieldworld::flying(h.w);
                    for s in 0..h.w.players.len() {
                        if let Some(i) = h.w.player_idx(s).filter(|_| !flying) {
                            wf_core::player::set_idle(h.w, i);
                        }
                    }
                    if let Some(f) = h.w.field.as_mut() {
                        f.nest_load = h.s.nest_area_enter;
                    }
                    wf_core::field::load_area(h.w, wf_core::field::AreaId { world: wd, area: m, variant: v }, x, y, Some(0));
                    if let Some(f) = h.w.field.as_mut() {
                        f.nest_load = false;
                    }
                    // the load waits for the new area's on-enter script (no event holds the field: Other World travel
                    // s511 / s512): a nesting host runs it now, the warp returns when the players arrived
                    if h.s.nest_area_enter && h.w.field.as_ref().is_some_and(|f| f.on_enter_queued) {
                        return block(h.s, Pending::Arrival);
                    }
                } else {
                    wf_core::field::warp_fade_slow(h.w, wd, m, v, x, y);
                    if let Some(f) = h.w.field.as_mut() {
                        f.nest_fade = h.s.nest_area_enter && h.w.scene.depth > 0;
                    }
                }
                if n == 0x76 {
                    return block(h.s, Pending::Transition);
                }
            } else {
                h.s.area = (wd, m, v);
                if let Some(i) = leader(h.w) {
                    let p = h.w.pl(i);
                    p.a.x = x << 8;
                    p.a.y = y << 8;
                }
                h.w.scene.requests.push(scene::Request::Warp { world: wd, map: m, variant: v, x, y });
            }
            unit()
        }
        0x78..=0x7e => {
            let (kind, out) = match n {
                0x78 => (FadeKind::Black, true),
                0x79 => (FadeKind::Black, false),
                0x7a => (FadeKind::White, true),
                0x7b => (FadeKind::White, false),
                0x7c => (FadeKind::Mosaic, true),
                0x7d => (FadeKind::Mosaic, false),
                _ => (FadeKind::Flash, true),
            };
            scene::start_fade(h.w, kind, out, arg(a, 0));
            block(h.s, Pending::Fade)
        }
        0x7f => done(h.w.story.supply(arg(a, 0))),
        0x80 => {
            add_supply_u8(h.w, arg(a, 0), 1, true);
            h.s.present.push(Present::ItemGet { item: arg(a, 0), equipment: false });
            enqueue(h, 0, Cmd::ItemGet { item: arg(a, 0), t: 0 })
        }
        0x81 => {
            add_supply_u8(h.w, arg(a, 0), arg(a, 1), true);
            unit()
        }
        0x82 | 0x83 => {
            let k = if n == 0x82 { 1 } else { arg(a, 1) };
            add_supply_u8(h.w, arg(a, 0), -k, false);
            unit()
        }
        0x84..=0x87 => {
            let (kind, (x, y)) = match n {
                0x84 => (DropKind::Zenie(arg(a, 0)), h.s.loot_pos),
                0x85 => (DropKind::Supply(arg(a, 0)), h.s.loot_pos),
                0x86 => (DropKind::Equipment(arg(a, 0)), h.s.loot_pos),
                _ => (DropKind::Zenie(arg(a, 2)), (arg(a, 0), arg(a, 1))),
            };
            if let Some(loot) = h.w.field.as_ref().map(|f| f.loot) {
                // the field's loot position / velocity (set by the dying enemy / breakable); 0x87 at (x, y), still
                if n == 0x87 {
                    wf_core::fieldobj::set_loot(h.w, x, y, 0, 0);
                }
                match kind {
                    DropKind::Zenie(sz) => wf_core::fieldobj::drop_zenie(h.w, sz),
                    DropKind::Supply(i) => wf_core::fieldobj::drop_item(h.w, i),
                    DropKind::Equipment(i) => wf_core::fieldobj::drop_item(h.w, -i),
                };
                if n == 0x87 {
                    wf_core::fieldobj::set_loot(h.w, loot.0, loot.1, loot.2, loot.3);
                }
            }
            h.s.drops.push(DropRequest { kind, x, y });
            unit()
        }
        0x88 => block(h.s, Pending::Frames { left: arg(a, 0), result: None }),
        0x89 => block(h.s, Pending::Button),
        0x8b => {
            wf_core::field::refresh_triggers(h.w);
            unit()
        }
        0x8c => {
            if leader(h.w).is_none() {
                return Ok(NativeResult::Done(None)); // the game pushes nothing without a player object
            }
            if h.w.data.menu.is_some() {
                wf_core::menu::save::open_save(h.w, arg(a, 0) != 0);
            } else {
                scene::open_menu(h.w, MenuKind::Save);
            }
            block(h.s, Pending::Menu { native: n, push: true })
        }
        0x8d => {
            if leader(h.w).is_none() {
                return Ok(NativeResult::Done(None)); // `0x080027DE`: nothing without a player object
            }
            if h.w.data.menu.is_some() {
                wf_core::menu::charsel::open_charsel(h.w);
            } else {
                scene::open_menu(h.w, MenuKind::Character);
            }
            block(h.s, Pending::Menu { native: n, push: false })
        }
        0x8e => {
            h.w.events.push(Event::SongStop);
            h.w.scene.requests.push(scene::Request::ReturnToTitle);
            unit()
        }
        0x8f | 0x90 => {
            let Some(id) = entity_of(h, arg(a, 0)) else { return unit() };
            let (x, y) = if n == 0x90 { (arg(a, 1), arg(a, 2)) } else { (0, 100) };
            h.w.scene.boss_bar = Some(scene::BossBar { entity: id, a: x, b: y });
            wf_core::vram::release(h.w, wf_core::vram::Owner::BossBar);
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::BossBar, 0x200);
            unit()
        }
        0x91 => {
            h.w.scene.boss_bar = None;
            wf_core::vram::release_later(h.w, wf_core::vram::Owner::BossBar, wf_core::vram::BOSS_BAR_OUT);
            unit()
        }
        0x92 => done(h.w.story.equipment_count(arg(a, 0))),
        0x93 => {
            h.w.story.add_equipment(arg(a, 0));
            h.s.present.push(Present::ItemGet { item: arg(a, 0), equipment: true });
            enqueue(h, 0, Cmd::ItemGet { item: -arg(a, 0), t: 0 })
        }
        0x94 => {
            h.w.story.take_equipment(arg(a, 0));
            unit()
        }
        0x95 => {
            switch_character(h.w, arg(a, 0))?;
            // the script's rebuilt player is a new object at the end of the world's list: it updates (and ticks its
            // script track) before the older ones (s218 -> s501 as Trunks: his walk and Goten's end in one frame,
            // his idle draw first)
            if let Some(i) = leader(h.w) {
                let e = h.w.ents.remove(i);
                h.w.ents.push(e);
            }
            unit()
        }
        0x96 => done(h.w.story.chars.get(arg(a, 0) as usize).is_some_and(|c| c.flags & 1 != 0) as i32),
        0x97..=0x9a => {
            if let Some(c) = h.w.story.chars.get_mut(arg(a, 0) as usize) {
                match n {
                    0x97 => c.flags |= 1,
                    0x98 => c.flags &= !1,
                    0x99 => c.flags |= 2,
                    _ => c.flags &= !2,
                }
            }
            unit()
        }
        0x9b => {
            let i = lead(h.w, n)?;
            let f = h.w.data.form(h.w.pl_ref(i).form);
            done(f.map(|f| f.ss || f.ss3).unwrap_or(false) as i32)
        }
        0x9c => done(char_sprite(h.w, arg(a, 0)).unwrap_or(0)),
        0x9d => {
            let (c, sprite) = (arg(a, 0), arg(a, 1));
            let Some(form) = h.w.data.forms.iter().find(|f| f.char_ == c && f.gfx == sprite).map(|f| f.id) else { return unit() };
            if let Some(cs) = h.w.story.chars.get_mut(c as usize) {
                cs.form = form;
            }
            if let Some(i) = fighter_of(h.w, c) {
                let mut p = h.w.pl_ref(i).clone();
                let gfx = |w: &World, f: i32| w.data.form(f).map(|f| f.gfx);
                let new_sprite = gfx(h.w, p.form) != gfx(h.w, form);
                let old = p.a.sprite;
                p.apply_form(h.w, form);
                *h.w.pl(i) = p;
                wf_core::vram::sprite_changed(h.w, i, old);
                // the active character's sprite is rebuilt, then vt+0x28(0) (s067: set_char_form(1, 108)); also with
                // the same sprite (s328: set_char_form(4, 279) on Vegeta already in form 279 draws once)
                let _ = new_sprite;
                if leader(h.w) == Some(i) {
                    wf_core::player::set_idle(h.w, i);
                }
            }
            unit()
        }
        0x9e | 0x9f => {
            let kind = if n == 0x9e { VisualKind::MarkerA } else { VisualKind::MarkerB };
            // the marker object (`0x08050690`): 0x40 of object tiles
            let vid = h.w.next_serial();
            h.w.scene.visuals.push(Visual { kind, x: arg(a, 0), y: arg(a, 1), owner: 0, t: 0, life: -1, vid });
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Visual(vid), 0x40);
            unit()
        }
        0xa0 => {
            // the box object (`0x080279E0`): 0x40
            let vid = h.w.next_serial();
            h.w.scene.visuals.push(Visual { kind: VisualKind::Box { w: arg(a, 2), h: arg(a, 3) }, x: arg(a, 0), y: arg(a, 1), owner: 0, t: 0, life: -1, vid });
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Visual(vid), 0x40);
            unit()
        }
        0xa1..=0xa3 => {
            let Some(i) = resolve_actor(h.w, h.s, arg(a, 0)) else { return done(0) };
            let ac = h.w.actor(i).ok_or("actor")?;
            done(match n {
                0xa1 => ac.px(),
                0xa2 => ac.py(),
                _ => ac.facing as i32,
            })
        }
        0xa4..=0xa6 | 0xe9 => {
            skill_edit(h.w, n, arg(a, 0), arg(a, 1), arg(a, 2));
            unit()
        }
        0xea => {
            let c = arg(a, 0);
            let lv = h.w.story.chars.get(c as usize).map(|cs| cs.skill_level(arg(a, 1))).unwrap_or(0);
            done(lv)
        }
        0xa7 => {
            // the mode (`0x08027DB0`) takes 0x2800 tile bytes, purging first (segment sb 6-6 f3477)
            wf_core::vram::purge(h.w, 0x2800);
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Minigame, 0x2800);
            if h.w.data.menu.is_some() {
                wf_core::menu::minigame::open_watch(h.w, arg(a, 0));
            } else {
                scene::open_menu(h.w, MenuKind::WatchThis { level: arg(a, 0) });
            }
            block(h.s, Pending::Menu { native: n, push: true })
        }
        0xa8 => {
            fuse(h.w, arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4));
            unit()
        }
        0xab => {
            h.w.scene.radar = Some(scene::Radar::default());
            unit()
        }
        0xac => {
            if let Some(r) = h.w.scene.radar.as_mut() {
                r.targets.push((arg(a, 0), arg(a, 1)));
            }
            unit()
        }
        0xad => {
            wf_core::field::respawn_breakables(h.w);
            unit()
        }
        0xae => {
            h.s.timers.countdown = arg(a, 0) * 60;
            // (a countdown switched on makes its object, whose first update counts at once; one already on - at 0
            // after its hook - is only set: 21-25's second minute reads 3600 in the frame it is set)
            // (not inside the countdown hook's own event: that runs within the field frame's timer update
            // 0x0800425C, so it ends mid-frame and the field frame does not run again in its exit frame - the real
            // route's second minute, s009 -> s461 -> s462, reads 3600 in the set frame)
            h.s.timers.fresh = !h.s.timers.countdown_on && !h.s.timer_event;
            h.s.timers.countdown_on = true;
            unit()
        }
        0xaf => {
            h.s.timers.countdown_on = false;
            unit()
        }
        0xb0 | 0xbc => {
            h.w.scene.timer_hud = Some(if n == 0xb0 { scene::TimerHud::Countdown } else { scene::TimerHud::Stopwatch });
            // its object (`0x0802FCA0`, vtable 0x0804BACC): 0x400 tile bytes (segment sb 21-25 f29581)
            wf_core::vram::release(h.w, wf_core::vram::Owner::Timer);
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Timer, 0x400);
            unit()
        }
        0xb1 | 0xbd => {
            h.w.scene.timer_hud = None;
            wf_core::vram::release(h.w, wf_core::vram::Owner::Timer);
            unit()
        }
        0xb2 => {
            h.w.scene.title = Some(scene::TitleText { string: arg(a, 0), x: arg(a, 1), y: arg(a, 2) });
            wf_core::vram::title_shown(h.w, arg(a, 0));
            unit()
        }
        0xef => {
            h.w.scene.title = None;
            wf_core::vram::release(h.w, wf_core::vram::Owner::Title);
            block(h.s, Pending::Frames { left: 1, result: None })
        }
        0xb3 => done(scene::sin_scale(arg(a, 0), arg(a, 1))),
        0xb4 => done(scene::sin_scale(arg(a, 0), arg(a, 1).wrapping_add(64))),
        0xb5..=0xb9 | 0xd6..=0xd8 => Err(format!("native 0x{n:02x} is handled by the VM")),
        0xba => {
            let (first, cnt) = (arg(a, 0), arg(a, 1));
            done((0..cnt).fold(0, |v, k| v | ((h.w.story.flag(first + k) as i32) << k)))
        }
        0xbb => {
            let (first, cnt) = (arg(a, 0), arg(a, 1));
            let v = arg(a, 2).clamp(0, if cnt >= 31 { i32::MAX } else { (1 << cnt) - 1 });
            for k in 0..cnt {
                h.w.story.set_flag(first + k, v >> k & 1 == 1);
            }
            unit()
        }
        0xbe => {
            h.s.timers.stopwatch = 0;
            unit()
        }
        0xbf | 0xc0 => {
            h.s.timers.stopwatch_on = n == 0xbf;
            unit()
        }
        0xc1 => done(h.s.timers.stopwatch),
        0xc2 => {
            // the kill-XP routine; a level-up runs the level-up hook #3 inline (blocks the script while shown)
            let i = lead(h.w, n)?;
            if wf_core::player::gain_xp_levels(h.w, i, arg(a, 0) as i64) {
                return Ok(NativeResult::RunEvent(3));
            }
            unit()
        }
        0xc4 | 0xc5 | 0xeb => {
            let bit = if n == 0xeb { 0x40 } else { 0x20 };
            if n == 0xc5 {
                h.w.story.game_bits &= !bit;
            } else {
                h.w.story.game_bits |= bit;
            }
            unit()
        }
        0xdb => {
            if arg(a, 0) != 0 {
                h.w.story.game_bits |= 0x100;
            } else {
                h.w.story.game_bits &= !0x100;
            }
            unit()
        }
        0xc6 => {
            // the exact screen (menu/minigame.rs) needs no menu content: used with or without it
            // the mode's gauge object (`0x080301F8`, vtable 0x0804C020): 0x380 tile bytes while it runs (segment
            // sb 1-5 f4204)
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Minigame, 0x380);
            wf_core::menu::minigame::open_power(h.w);
            block(h.s, Pending::Menu { native: n, push: false })
        }
        0xc7 => {
            if h.w.data.menu.is_some() {
                wf_core::menu::minigame::open_punch(h.w, arg(a, 0));
            } else {
                scene::open_menu(h.w, MenuKind::Punch { mode: arg(a, 0) });
            }
            block(h.s, Pending::Menu { native: n, push: true })
        }
        0xc9 => {
            h.w.scene.requests.push(scene::Request::Broadcast { msg: 0x1f, a: 0, b: 0 });
            unit()
        }
        0xca => {
            for c in h.w.story.chars.iter_mut() {
                c.rec.hp = c.rec.max_hp;
                c.rec.ep = c.rec.max_ep;
            }
            let ids = h.w.players.clone();
            for id in ids {
                if let Some(i) = h.w.find(id) {
                    let p = h.w.pl(i);
                    p.rec.hp = p.rec.max_hp;
                    p.rec.ep = p.rec.max_ep;
                }
            }
            unit()
        }
        0xcb | 0xcc => {
            let Some(i) = resolve_actor(h.w, h.s, arg(a, 0)) else { return done(0) };
            let (x, y) = scene::screen_rect_at(h.w, i, arg(a, 1), arg(a, 2));
            done(if n == 0xcb { x } else { y })
        }
        0xcd => {
            h.w.scene.shop.push(scene::ShopItem { category: arg(a, 0), item: arg(a, 1), price: arg(a, 2), flag: arg(a, 3) });
            unit()
        }
        0xce => {
            if h.w.data.menu.is_some() {
                wf_core::menu::shop::open_shop_buy(h.w);
            } else {
                scene::open_menu(h.w, MenuKind::ShopBuy);
            }
            block(h.s, Pending::Menu { native: n, push: false })
        }
        0xcf => {
            if h.w.data.menu.is_some() {
                wf_core::menu::shop::open_shop_sell(h.w, arg(a, 0));
            } else {
                scene::open_menu(h.w, MenuKind::ShopSell { percent: arg(a, 0) });
            }
            block(h.s, Pending::Menu { native: n, push: false })
        }
        0xd0 => {
            let i = lead(h.w, n)?;
            if h.w.data.form(arg(a, 0)).is_none() {
                return Err(format!("native 0xd0: unknown form {}", arg(a, 0)));
            }
            wf_core::player::script_set_form(h.w, i, arg(a, 0));
            unit()
        }
        0xd1 => {
            if h.w.data.menu.is_some() {
                wf_core::menu::credits::open_credits(h.w);
            } else {
                scene::open_menu(h.w, MenuKind::Credits);
                h.w.scene.requests.push(scene::Request::Credits);
            }
            block(h.s, Pending::Menu { native: n, push: false })
        }
        0xd2 => done((h.w.scene.keys[0].pressed & A != 0) as i32),
        0xd3 => done(current_form(h.w, arg(a, 0)).map(|f| Handle::FormName(f).encode()).unwrap_or(0)),
        0xd4 => {
            h.w.scene.requests.push(scene::Request::Sleep);
            unit()
        }
        0xd5 => {
            let i = lead(h.w, n)?;
            done(h.w.pl_ref(i).equip.contains(&arg(a, 0)) as i32)
        }
        0xd9 => {
            h.w.scene.dest_label = Some((arg(a, 0), vec![0; 32]));
            // the label object (`0x0803643C`, vtable 0x0804BAF4): 0xF40 tile bytes (segment sb 18-1 -> 0-51)
            wf_core::vram::release(h.w, wf_core::vram::Owner::Dest);
            wf_core::vram::acquire(h.w, wf_core::vram::Owner::Dest, 0xF40);
            // the ctor starts on trigger list entry [count - set - 1] (= our trigger index `set`)
            h.w.scene.dest_sel = arg(a, 0);
            unit()
        }
        0xda => {
            if let Some((_, v)) = h.w.scene.dest_label.as_mut() {
                if let Some(e) = v.get_mut(arg(a, 0) as usize) {
                    *e = arg(a, 1);
                }
            }
            unit()
        }
        0xdc => {
            h.w.scene.requests.push(scene::Request::Broadcast { msg: 8, a: arg(a, 0), b: arg(a, 1) });
            unit()
        }
        0xdd => {
            let mut sum = 0i64;
            for c in 0..5 {
                sum += match fighter_of(h.w, c) {
                    Some(i) => h.w.pl_ref(i).rec.xp,
                    None => h.w.story.chars.get(c as usize).map(|x| x.rec.xp).unwrap_or(0),
                };
            }
            done(sum as i32)
        }
        0xde => done(Handle::String(arg(a, 0)).encode()),
        0xdf..=0xe8 | 0xf5 => {
            let k = if n == 0xf5 { 15 } else { (n - 0xdf) as usize };
            let v = arg(a, 0);
            let e = &mut h.s.eq_accum[k];
            match n {
                0xdf => *e += (v * 0x445) >> 8,
                0xe0 => *e += (v * 0xa4) >> 6,
                0xe5..=0xe7 => *e += 1, // halvings counted (applied by the stat recalculation)
                0xe8 => *e += 1,
                _ => *e += v,
            }
            unit()
        }
        0xec => {
            h.w.scene.visuals.push(Visual { kind: VisualKind::MapMarker { highlight: arg(a, 2) != 0 }, x: arg(a, 0) - 4, y: arg(a, 1) - 4, owner: 0, t: 0, life: -1, vid: 0 });
            unit()
        }
        0xed => {
            let c = arg(a, 0);
            done(match fighter_of(h.w, c) {
                Some(i) => h.w.pl_ref(i).rec.points,
                None => h.w.story.chars.get(c as usize).map(|x| x.rec.points).unwrap_or(0),
            })
        }
        0xee => {
            let c = arg(a, 0);
            match fighter_of(h.w, c) {
                Some(i) => h.w.pl(i).rec.points += arg(a, 1),
                None => {
                    if let Some(x) = h.w.story.chars.get_mut(c as usize) {
                        x.rec.points += arg(a, 1);
                    }
                }
            }
            unit()
        }
        0xf0 => {
            // actor+0x1A8 = value << 10, positive = up (the height the field's enemy runner keeps; the world map's
            // on-enter event lowers its ground encounters with it)
            if let Some(i) = resolve_actor(h.w, h.s, arg(a, 0)) {
                wf_core::fieldpatrol::set_z(h.w, i, arg(a, 1) << 10);
                if !matches!(h.w.ents[i].body, Body::Enemy(_)) {
                    if let Some(ac) = actor_mut(h.w, i) {
                        ac.z = -arg(a, 1);
                    }
                }
            }
            unit()
        }
        0xf1 => {
            let (c, slot) = (arg(a, 0), arg(a, 1) as usize & 3);
            const DEFAULT: [i32; 4] = [0, 44, 77, 112];
            if let Some(i) = fighter_of(h.w, c) {
                let old = h.w.pl_ref(i).equip[slot];
                if old != DEFAULT[slot] {
                    h.w.story.equipment.push(old);
                    // 0x08004544 changes the record's slot and the owned list only: the live fighter keeps its
                    // stats until the next recalculation (Thieves Den 9-15: after s362's monkey takes the Bandana
                    // the player object still has STR 67 / POW 46 / END 53 for the bandits' fight)
                    h.w.pl(i).equip[slot] = DEFAULT[slot];
                }
            } else if let Some(cs) = h.w.story.chars.get_mut(c as usize) {
                let old = cs.equip[slot];
                if old != DEFAULT[slot] {
                    cs.equip[slot] = DEFAULT[slot];
                    h.w.story.equipment.push(old);
                }
            }
            unit()
        }
        0xf2 => {
            let i = lead(h.w, n)?;
            let mut p = h.w.pl_ref(i).clone();
            if arg(a, 0) == 0 {
                // the record's base stats are bytes (rec+0x28..0x2B): the add wraps
                p.rec.str_ = (p.rec.str_ + arg(a, 1)) & 0xff;
                p.rec.pow = (p.rec.pow + arg(a, 2)) & 0xff;
                p.rec.end = (p.rec.end + arg(a, 3)) & 0xff;
                p.rec.spd = (p.rec.spd + arg(a, 4)) & 0xff;
            } else {
                // the timed buff (GD+0x4DC frames, +0x4E0..0x4EC values): one global buff, replaced
                h.s.boost = [arg(a, 0) * 60, arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4), 0];
                p.holy_left = arg(a, 0) * 60;
                p.boost = [arg(a, 1), arg(a, 2), arg(a, 3), arg(a, 4)];
            }
            p.refresh_stats(h.w);
            *h.w.pl(i) = p;
            unit()
        }
        0xf3 => {
            let c = arg(a, 0);
            let target = arg(a, 1).min(200);
            let g = h.w.data.chars.iter().find(|x| x.index == c).map(|x| x.growth).ok_or("character")?;
            let mut rec = match fighter_of(h.w, c) {
                Some(i) => h.w.pl_ref(i).rec.clone(),
                None => h.w.story.chars.get(c as usize).map(|x| x.rec.clone()).ok_or("character")?,
            };
            let old = rec.level;
            if target > old {
                wf_core::progression::level_up(&mut rec, &g, old, target, &mut h.w.rng);
                rec.level = target;
                if let Some(&t) = h.w.data.xp_table.get((target - 50).max(0) as usize) {
                    rec.xp = t;
                }
                match fighter_of(h.w, c) {
                    Some(i) => {
                        let mut p = h.w.pl_ref(i).clone();
                        p.rec = rec;
                        p.refresh_stats(h.w);
                        *h.w.pl(i) = p;
                    }
                    None => {
                        if let Some(x) = h.w.story.chars.get_mut(c as usize) {
                            x.rec = rec;
                        }
                    }
                }
            }
            unit()
        }
        0xf4 => {
            h.w.scene.visuals.retain(|v| v.kind != VisualKind::EnergySphere);
            h.w.scene.visuals.push(Visual { kind: VisualKind::EnergySphere, x: arg(a, 0), y: arg(a, 1), owner: arg(a, 2) as u32, t: arg(a, 3), life: -1, vid: 0 });
            unit()
        }
        0xf6 => done(0),
        0xf7 => {
            // the timed buff with no stat bonus and the special flag (GD+0x4F0 = 1 -> record byte +0x67, whose
            // effect is not identified: kept in HostState.boost[5])
            h.s.boost = [arg(a, 0) * 60, 0, 0, 0, 0, 1];
            let i = lead(h.w, n)?;
            let mut p = h.w.pl_ref(i).clone();
            p.holy_left = arg(a, 0) * 60;
            p.boost = [0; 4];
            p.refresh_stats(h.w);
            *h.w.pl(i) = p;
            unit()
        }
        _ => Err(format!("native 0x{n:02x} out of table")),
    }
}

fn actor_mut(w: &mut World, i: usize) -> Option<&mut wf_core::actor::Actor> {
    match &mut w.ents[i].body {
        Body::Player(p) => Some(&mut p.a),
        Body::Enemy(e) => Some(&mut e.a),
        Body::Npc(n) => Some(&mut n.a),
        _ => None,
    }
}

fn area(h: &CoreHost) -> (i32, i32, i32) {
    match &h.w.field {
        Some(f) => (f.area.world, f.area.area, f.area.variant),
        None => h.s.area,
    }
}

/// supply counters are u8 (`GameData+2+item`): adds saturate at 255, takes wrap without a check (the game)
fn add_supply_u8(w: &mut World, item: i32, n: i32, saturate: bool) {
    if item < 0 {
        return;
    }
    let i = item as usize;
    if i >= w.story.supplies.len() {
        w.story.supplies.resize(i + 1, 0);
    }
    let v = w.story.supplies[i] as i32 + n;
    w.story.supplies[i] = if saturate { v.clamp(0, 255) as u8 } else { (v & 0xff) as u8 };
}

/// `set_character`: the leader becomes character `c` (its persistent record, form, equipment and skills)
pub fn switch_character(w: &mut World, c: i32) -> Result<(), String> {
    let Some(i) = leader(w) else {
        if !w.scene.offstage.iter().any(|e| matches!(e.body, Body::Player(_))) {
            return Err("native 0x95: no player".into());
        }
        // no player object (offstage during a cutscene scene: s217's set_character(0) between its load_scene and
        // its warp): the current character changes and the player kept offstage becomes it - no idle draw; the
        // next load builds the new character (6-1: Goku, and the NPC whose condition reads get_character())
        let cs = w.story.chars.get(c as usize).cloned().ok_or("native 0x95: character")?;
        let form = if w.data.form(cs.form).is_some_and(|f| f.char_ == c) {
            cs.form
        } else {
            w.data.chars.iter().find(|x| x.index == c).map(|x| x.form).ok_or("character")?
        };
        if let Some(k) = w.scene.offstage.iter().position(|e| matches!(e.body, Body::Player(_))) {
            let Body::Player(old) = &w.scene.offstage[k].body else { unreachable!() };
            let mut p = (**old).clone();
            if cs.rec.max_hp > 0 {
                p.rec = cs.rec.clone();
                p.equip = cs.equip;
                if !cs.skills.is_empty() {
                    p.skills = cs.skills.clone();
                }
                p.sel = (cs.sel.max(0) as usize).min(p.skills.len().saturating_sub(1));
            }
            p.holy_left = 0;
            p.regen_timer = 0;
            p.apply_form(w, form);
            w.scene.offstage[k].body = Body::Player(Box::new(p));
        }
        w.story.current = c;
        return Ok(());
    };
    wf_core::story::sync_from_players(w);
    let cs = w.story.chars.get(c as usize).cloned().ok_or("native 0x95: character")?;
    let form = if w.data.form(cs.form).is_some_and(|f| f.char_ == c) {
        cs.form
    } else {
        w.data.chars.iter().find(|x| x.index == c).map(|x| x.form).ok_or("character")?
    };
    let mut p = w.pl_ref(i).clone();
    if cs.rec.max_hp > 0 {
        p.rec = cs.rec.clone();
        p.equip = cs.equip;
        if !cs.skills.is_empty() {
            p.skills = cs.skills.clone();
        }
        // the selected skill is the character's own (record +0x15): Goku's L-cycled slot does not follow to Gohan
        // (app soak, 3-1 after s125: Gohan fired skill slot 2 instead of his slot 0)
        p.sel = (cs.sel.max(0) as usize).min(p.skills.len().saturating_sub(1));
    }
    p.holy_left = 0;
    // the new character's form controller starts its regen count (+0xC) at 0 (Kid Buu's arena: after s-script
    // hand-overs the bench's 5% tick comes 596 pass frames after the event that switched)
    p.regen_timer = 0;
    // the hit-damage remainder stays: set_character keeps the player object (unlike a load or the candy respawn,
    // which rebuild it). The real route's arena, s450 (Goku -> Vegeta): Super Janemba's first hit on Vegeta takes
    // 91 with Goku's remainder, 90 without; with it the whole 6,940-frame cut rj_arena_000_18_14 is exact
    let gfx = |w: &World, f: i32| w.data.form(f).map(|f| f.gfx);
    let new_sprite = gfx(w, p.form) != gfx(w, form);
    let old = p.a.sprite;
    p.apply_form(w, form);
    *w.pl(i) = p;
    wf_core::vram::sprite_changed(w, i, old);
    w.story.current = c;
    // 0x08010692 rebuilds the player and ends with vt+0x28(0): the idle state is entered again (rand(255); s067:
    // Goku -> Gohan on the Planet of the Kais; also with the same sprite: s124's set_character(1) right after its
    // warp to Hercule City draws once more after the new player's two)
    let _ = new_sprite;
    wf_core::player::set_idle(w, i);
    Ok(())
}

/// learn / forget / replace skill, set skill level (0xA4 / 0xA5 / 0xA6 / 0xE9) on the story record and the fighter
fn skill_edit(w: &mut World, n: u8, c: i32, x: i32, y: i32) {
    let Some(cs) = w.story.chars.get_mut(c as usize) else { return };
    match n {
        0xa4 => {
            cs.skills.push(x);
            cs.skill_levels.push(0);
        }
        0xa5 => {
            if let Some(k) = cs.skills.iter().position(|&s| s == x) {
                cs.skills.remove(k);
                if k < cs.skill_levels.len() {
                    cs.skill_levels.remove(k);
                }
                if cs.sel >= cs.skills.len() as i32 {
                    cs.sel = 0;
                }
            }
        }
        0xa6 => {
            if let Some(k) = cs.skills.iter().position(|&s| s == x) {
                cs.skills[k] = y;
            }
        }
        _ => {
            if let Some(k) = cs.skills.iter().position(|&s| s == x) {
                if k < cs.skill_levels.len() {
                    cs.skill_levels[k] = y;
                }
            }
        }
    }
    let skills = cs.skills.clone();
    if let Some(i) = fighter_of(w, c) {
        w.pl(i).skills = skills.clone();
    }
    // a player out of the world during a cutscene scene (scene::send_offstage) is the same record (story playthrough:
    // the prologue's forget_skill(0, 20) while Goku is offstage)
    let forms: Vec<i32> = w.scene.offstage.iter().filter_map(|e| match &e.body {
        Body::Player(p) => Some(p.form),
        _ => None,
    }).collect();
    for (k, form) in forms.into_iter().enumerate() {
        if char_of_form(w, form) == Some(c) {
            if let Body::Player(p) = &mut w.scene.offstage[k].body {
                p.skills = skills.clone();
                if p.sel >= p.skills.len() {
                    p.sel = 0;
                }
            }
        }
    }
}

/// `fuse_characters(fused, kind, a, b, seconds)` (0x0801375E): 60% of the members' summed stats
fn fuse(w: &mut World, fused: i32, kind: i32, ma: i32, mb: i32, seconds: i32) {
    let (Some(a), Some(b)) = (w.story.chars.get(ma as usize).cloned(), w.story.chars.get(mb as usize).cloned()) else { return };
    let mix = |x: i32, y: i32| ((x + y) * 0x9a) >> 8;
    let Some(f) = w.story.chars.get_mut(fused as usize) else { return };
    f.rec.max_hp = mix(a.rec.max_hp, b.rec.max_hp);
    f.rec.hp = mix(a.rec.hp, b.rec.hp);
    f.rec.max_ep = mix(a.rec.max_ep, b.rec.max_ep);
    f.rec.ep = mix(a.rec.ep, b.rec.ep);
    // base stats: the components' mean + 10 (SPD + 16), as `progression::fuse` (sc_janemba: Gogeta from Goku 84 / 68
    // / 67 / 75 and Vegeta 88 / 61 / 75 / 65 is 96 / 74 / 81 / 86 in the game's record)
    let half = |x: i32, y: i32| (x + y) / 2;
    f.rec.str_ = half(a.rec.str_, b.rec.str_) + 10;
    f.rec.pow = half(a.rec.pow, b.rec.pow) + 10;
    f.rec.end = half(a.rec.end, b.rec.end) + 10;
    f.rec.spd = half(a.rec.spd, b.rec.spd) + 16;
    f.flags |= 3;
    f.ss_cooldown = if seconds == -1 { -1 } else { seconds * 60 };
    f.fusion = ((ma & 15) | (mb & 15) << 4 | (kind & 0xff) << 8) as u16;
    if let Some(x) = w.story.chars.get_mut(ma as usize) {
        x.flags &= !1;
    }
    if let Some(x) = w.story.chars.get_mut(mb as usize) {
        x.flags &= !1;
    }
}

fn start_phases(h: &mut CoreHost, list: Vec<Phase>) -> R {
    h.s.pending = Some(Pending::Phases { list, k: 0, started: false });
    // the first phase begins in this frame
    poll_phases(h.w, h.s);
    Ok(NativeResult::Block)
}

/// drive a Phases pending: start each phase, advance when it is done; Some(None) when all are done
pub fn poll_phases(w: &mut World, s: &mut HostState) -> Option<Option<i32>> {
    loop {
        let Some(Pending::Phases { list, k, started }) = s.pending.clone() else { return Some(None) };
        let n = list.len();
        let Some(ph) = list.get(k).cloned() else { return Some(None) };
        if !started {
            match &ph {
                Phase::Fade { kind, out, frames } => {
                    scene::start_fade(w, *kind, *out, *frames);
                    s.pending = Some(Pending::Phases { list, k, started: true });
                    return None;
                }
                Phase::LoadScene { world, map, variant, x, y, objects } => {
                    load_scene(w, s, *world, *map, *variant, *x, *y, *objects);
                }
                Phase::ReturnToField => return_to_field(w, s),
            }
            s.pending = Some(Pending::Phases { list, k: k + 1, started: false });
            continue;
        }
        if !scene::fade_done(w) {
            return None;
        }
        s.pending = Some(Pending::Phases { list, k: k + 1, started: false });
        if k + 1 >= n {
            return Some(None);
        }
    }
}

/// `load_scene*`: load map world/map as a cutscene backdrop, camera top-left at (x, y)
#[allow(clippy::too_many_arguments)]
/// the armed field save (`save_field_on_next_scene`): the snapshot 0x08009196 takes the field's objects out of the
/// world at once
fn save_field(w: &mut World, s: &mut HostState) {
    if !s.save_armed {
        return;
    }
    s.save_armed = false;
    let (px, py, facing) = leader(w).map(|i| (w.pl_ref(i).a.px(), w.pl_ref(i).a.py(), w.pl_ref(i).a.facing)).unwrap_or_default();
    let area = match &w.field {
        Some(f) => (f.area.world, f.area.area, f.area.variant),
        None => s.area,
    };
    s.saved_field = Some(SavedField { area, x: px, y: py, facing, cam: (w.cameras[0].x, w.cameras[0].y) });
    if w.field.is_some() {
        // the kept field's objects give their shared graphics back and its tile allocator is set aside: the
        // scene gets a fresh one (message 0xA; segment sb 21-5's s483)
        wf_core::vram::world_hide(w);
        w.scene.saved_world = Some(Box::new(wf_core::field::save_world(w)));
    }
}

fn load_scene(w: &mut World, s: &mut HostState, world: i32, map: i32, variant: i32, x: i32, y: i32, objects: bool) {
    save_field(w, s);
    if w.field.is_some() {
        let to = wf_core::field::AreaId { world, area: map, variant };
        // a cutscene scene has no player object (0x08009030): the players wait offstage
        scene::send_offstage(w);
        wf_core::field::load_area_with(w, to, x + 112, y + 76, None, objects);
        // a scene's objects are held by its event like the others (not the warp loads' running ones: segment sb
        // s483's 21-4 backdrop, its Blister spawns nothing)
        if let Some(f) = w.field.as_mut() {
            f.load_ids = (0, 0);
            f.scene_load = true;
        }
    } else {
        s.area = (world, map, variant);
        w.scene.requests.push(scene::Request::LoadScene { world, map, variant, x, y, objects });
    }
    w.scene.cam.mode = scene::CamMode::Fixed;
    if w.field.is_some() {
        // inside the new map's camera limits (7-2 s273: load_scene(7, 4, 1, 1, 1) shows (1, 0))
        scene::set_scene_camera(w, x, y);
    } else {
        wf_core::battle::set_camera(w, 0, x << 10, y << 10);
    }
}

fn return_to_field(w: &mut World, s: &mut HostState) {
    let Some(f) = s.saved_field.take() else {
        w.scene.requests.push(scene::Request::ReturnToField);
        return;
    };
    // the field kept as it was: restored, not reloaded (0x75)
    if let Some(sw) = w.scene.saved_world.take() {
        if w.field.is_some() {
            wf_core::field::restore_world(w, *sw);
            // its allocator back, the objects take their shared graphics again (message 0xB)
            wf_core::vram::world_show(w);
            let _ = f;
            return;
        }
    }
    if w.field.is_some() {
        let to = wf_core::field::AreaId { world: f.area.0, area: f.area.1, variant: f.area.2 };
        scene::bring_back(w);
        wf_core::field::load_area(w, to, f.x, f.y, Some(f.facing));
    } else {
        s.area = f.area;
        w.scene.requests.push(scene::Request::ReturnToField);
    }
    w.scene.cam.mode = scene::CamMode::Player;
    wf_core::battle::set_camera(w, 0, f.cam.0, f.cam.1);
}

/// a menu closed: apply what it did (purchases, sales, character switch)
pub fn menu_closed(w: &mut World, s: &mut HostState, native: u8, r: i32, log: &[(i32, i32)]) {
    match native {
        0xce => {
            for &(k, _) in log {
                let Some(it) = w.scene.shop.get(k as usize).cloned() else { continue };
                if (w.story.zenie as i64) < it.price as i64 {
                    continue;
                }
                w.story.zenie -= it.price.max(0) as u32;
                if it.category == 0 {
                    add_supply_u8(w, it.item, 1, true);
                } else {
                    w.story.add_equipment(it.item);
                }
            }
            w.scene.shop.clear();
        }
        0xcf => {
            let pct = match w.scene.menu.as_ref().map(|m| m.kind) {
                Some(MenuKind::ShopSell { percent }) => percent,
                _ => 50,
            };
            let _ = pct;
            for &(k, _) in log {
                add_supply_u8(w, k, -1, false);
            }
        }
        0xa7 => {
            // ... and gives them back when it ends; the purged objects return (`0x08027E00`)
            wf_core::vram::release(w, wf_core::vram::Owner::Minigame);
            wf_core::vram::restore(w);
        }
        0x8d => {
            if r >= 0 && w.story.chars.get(r as usize).is_some_and(|c| c.in_party()) {
                let _ = switch_character(w, r);
            }
        }
        _ => {}
    }
    let _ = s;
}

pub fn set(h: &mut CoreHost, var: u8, v: i32) -> Result<(), String> {
    match var {
        8..=11 => h.w.story.l[var as usize - 8] = v,
        12..=15 => {
            let i = lead(h.w, var)?;
            let ss = h.w.data.form(h.w.pl_ref(i).form).map(|f| (f.ss, f.ss3)).unwrap_or((false, false));
            let p = h.w.pl(i);
            match var {
                12 => {
                    wf_core::player::script_set_hp(h.w, i, v);
                    return Ok(());
                }
                13 => {
                    p.rec.max_hp = v;
                    p.rec.hp = v;
                }
                14 => {
                    let v = v.clamp(0, p.rec.max_ep);
                    match ss {
                        (_, true) => p.ss3_gauge = (v as i64) << 16,
                        (true, _) => p.ss_gauge = v,
                        _ => p.rec.ep = v,
                    }
                }
                _ => {
                    p.rec.max_ep = v;
                    p.rec.ep = v;
                }
            }
        }
        16 => h.w.story.zenie = v.max(0) as u32,
        17 => h.w.story.variant_byte = v & 0xff,
        _ => return Err(format!("setter {var} out of range")),
    }
    Ok(())
}

/// event text / choice nodes: a dialog box in `World.scene`, blocking until it closes
pub fn node(h: &mut CoreHost, call: NodeCall) -> R {
    match call {
        NodeCall::Text { text, arg, args, .. } => {
            let raw = text::unescape(&text);
            let shown = if args.is_empty() { raw } else { text::format(h.w, &raw, &args) };
            h.s.present.push(Present::Text { text, portrait: arg, args });
            let typing = text::typing(&shown);
            // the box does not depend on the node kind (5 / 6) or the bank reference: both runtimes agree on text
            scene::open_text(h.w, &shown, 0, arg, 5, typing);
            // a text node whose last page lists options ('-' lines) is a choice too: its cursor is the menu result
            // (s011's "-Save / -Switch Characters", a banked text node with box 7: segment sb 21-14)
            let choice = h.w.scene.dialog.as_ref().is_some_and(|d| d.options > 0);
            block(h.s, Pending::Dialog { choice })
        }
        NodeCall::Choice { text, arg, .. } => {
            h.s.present.push(Present::Choice { text: text.clone(), portrait: arg });
            let raw = text::unescape(&text);
            let typing = text::typing(&raw);
            scene::open_choice(h.w, &raw, arg, typing);
            block(h.s, Pending::Dialog { choice: true })
        }
    }
}

/// the multiplayer extension API (`wf_script::vm::EXTENSIONS`)
pub fn ext(h: &mut CoreHost, c: &ExtCall) -> R {
    let a = c.args;
    let pl = |h: &CoreHost, n: i32| crate::player(h.w, n).and_then(|id| h.w.find(id));
    match c.name {
        "player_count" => done(h.w.players.len() as i32),
        "player" => done(if (1..=h.w.players.len() as i32).contains(&arg(a, 0)) {
            if arg(a, 0) == 1 {
                0
            } else {
                pl(h, arg(a, 0)).and_then(|i| scene::sprite_id(h.w, i)).unwrap_or(0)
            }
        } else {
            -1
        }),
        "player_of_actor" => {
            let i = resolve_actor(h.w, h.s, arg(a, 0));
            done(i.and_then(|i| h.w.players.iter().position(|&id| id == h.w.ents[i].id)).map(|k| k as i32 + 1).unwrap_or(0))
        }
        "player_character" => done(pl(h, arg(a, 0)).and_then(|i| char_of_form(h.w, h.w.pl_ref(i).form)).unwrap_or(-1)),
        "player_hp" | "player_max_hp" | "player_energy" | "player_x" | "player_y" | "player_alive" => {
            let Some(i) = pl(h, arg(a, 0)) else { return done(0) };
            let p = h.w.pl_ref(i);
            done(match c.name {
                "player_hp" => p.rec.hp,
                "player_max_hp" => p.rec.max_hp,
                "player_energy" => p.rec.ep,
                "player_x" => p.a.px(),
                "player_y" => p.a.py(),
                _ => !p.dead as i32,
            })
        }
        "player_button" | "player_pressed" => {
            let slot = (arg(a, 0) - 1).clamp(0, 3) as usize;
            let bit = match arg(a, 1) {
                0 => wf_core::input::A,
                1 => wf_core::input::B,
                2 => wf_core::input::L,
                3 => wf_core::input::R,
                _ => wf_core::input::START,
            };
            let k = h.w.scene.keys[slot];
            done(((if c.name == "player_button" { k.held } else { k.pressed }) & bit != 0) as i32)
        }
        "set_player_hp" | "set_player_energy" => {
            if let Some(i) = pl(h, arg(a, 0)) {
                let p = h.w.pl(i);
                if c.name == "set_player_hp" {
                    p.rec.hp = arg(a, 1).clamp(0, p.rec.max_hp);
                } else {
                    p.rec.ep = arg(a, 1).clamp(0, p.rec.max_ep);
                }
            }
            unit()
        }
        "set_player_character" => {
            if let Some(i) = pl(h, arg(a, 0)) {
                if let Some(form) = h.w.data.chars.iter().find(|x| x.index == arg(a, 1)).map(|x| x.form) {
                    let mut p = h.w.pl_ref(i).clone();
                    let old = p.a.sprite;
                    p.apply_form(h.w, form);
                    *h.w.pl(i) = p;
                    wf_core::vram::sprite_changed(h.w, i, old);
                }
            }
            unit()
        }
        "give_item_to" | "give_equipment_to" => {
            let eq = c.name == "give_equipment_to";
            if eq {
                h.w.story.add_equipment(arg(a, 1));
            } else {
                add_supply_u8(h.w, arg(a, 1), 1, true);
            }
            let owner = pl(h, arg(a, 0)).map(|i| Owner::Entity(h.w.ents[i].id));
            enqueue_owner(h, owner, Cmd::ItemGet { item: if eq { -arg(a, 1) } else { arg(a, 1) }, t: 0 })
        }
        "give_xp_to" => {
            if let Some(i) = pl(h, arg(a, 0)) {
                wf_core::player::gain_xp(h.w, i, arg(a, 1) as i64);
            }
            unit()
        }
        "revive_player" => {
            if let Some(i) = pl(h, arg(a, 0)) {
                let p = h.w.pl(i);
                p.dead = false;
                p.rec.hp = p.rec.max_hp;
            }
            unit()
        }
        "warp_player" => {
            if let Some(i) = pl(h, arg(a, 0)) {
                let p = h.w.pl(i);
                p.a.x = arg(a, 4) << 8;
                p.a.y = arg(a, 5) << 8;
            }
            unit()
        }
        "gather_players" => {
            let ids = h.w.players.clone();
            for (k, id) in ids.into_iter().enumerate() {
                if let Some(i) = h.w.find(id) {
                    let p = h.w.pl(i);
                    p.a.x = (arg(a, 0) + 16 * k as i32) << 8;
                    p.a.y = arg(a, 1) << 8;
                }
            }
            unit()
        }
        "say_to" => {
            let s = text::resolve(h.w, Handle::String(arg(a, 2)).encode());
            let typing = text::typing(&s);
            scene::open_text(h.w, &s, 0, arg(a, 1) as u8, 5, typing);
            block(h.s, Pending::Dialog { choice: false })
        }
        other => Err(format!("extension `{other}` not supported")),
    }
}

/// a level-up / XP popup or drop needs the loot position; the engine sets it before a drop script runs
pub fn set_loot_pos(s: &mut HostState, x: i32, y: i32) {
    s.loot_pos = (x, y);
}

#[allow(dead_code)]
fn _unused(w: &World) -> i32 {
    char_sprite(w, 0).unwrap_or(0)
}
