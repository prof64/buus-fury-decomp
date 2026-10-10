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
use crate::shots::{SName, ShotKind};
use crate::world::{Body, Event, World};
use wf_view::{
    AudioEvent, CameraView, FrameSel, HudView, MapView, RenderView, Sim, SimEvent, SpriteDraw,
    SpriteEffect,
};

/// the volleyball an Ultra Volleyball Attack makes of an enemy: sprite 281 (the golden orb, 0x0803C830)
const VOLLEYBALL_SPRITE: i32 = 281;

/// the Dragon Fist chain as its OAM shows it: built before the player control's shift of the frame (root by v, each
/// child 0x20 more; skill audit sa_f41_skill5_steer: the game's OAM x is the unshifted segment's in every frame)
fn dragon_drawn(w: &World, seg: &[[i32; 2]; 6], dir: crate::geom::Dir, shifted: (u32, i32)) -> [[i32; 2]; 6] {
    let mut out = *seg;
    if shifted.0 == w.frame {
        let (dx, dy) = crate::geom::DIR_VEC[(dir & 3) as usize];
        for (k, s) in out.iter_mut().enumerate() {
            s[0] -= dx * (shifted.1 + 0x20 * k as i32);
            s[1] -= dy * (shifted.1 + 0x20 * k as i32);
        }
    }
    out
}

/// an actor's sort depth on the actors' y scale: the game's +4 = y + (footprint h >> 1) (the actor draw 0x080101C6:
/// obj+0x19D; Goku's 8 -> y + 4, Spopovich's 16 -> y + 8 - his fist in front of Goku 3 px below him, opponent audit
/// oa_t70_left f80); here less the usual 4
fn actor_depth(a: &crate::actor::Actor) -> i32 {
    a.py() + (a.foot_h >> 1) - 4
}

fn draw_key(w: &World, i: usize) -> (i32, i32, u32) {
    let e = &w.ents[i];
    let (layer, depth) = match &e.body {
        Body::Player(p) => (0, actor_depth(&p.a)),
        Body::Enemy(n) => {
            // (Janemba's arms 0x34 / 0x35 sort by their own y - behind the body, in front of what is above them;
            // the upper body 0x37 in front of the body: opponent audit oa_t33_idle f200, the shoulders and a bush)
            let d = if n.part && !matches!(n.ty, 0x34 | 0x35) {
                w.enemy_by_id(n.parent)
                    .map(|(_, p)| actor_depth(&p.a) + 1)
                    .unwrap_or(actor_depth(&n.a))
            } else {
                actor_depth(&n.a)
            };
            (0, d)
        }
        Body::Hand(h) => (0, actor_depth(&h.a)),
        Body::Rock { y, .. } => (0, y + 40),
        // the game's sort keys (comparator 0x08052EE0: +8 layer byte, lower in front; +4 depth): an actor's depth is
        // its y + 4 (layer 0x38), most shots are layer 0x37 (in front of the actors: Final Flash 0x0804AF24, Big Bang
        // 0x0804A26C, Super Buu's ball 0x0804A33C), the Energy Blast 0x0804B454 is 0x38 with depth = its y (skill
        // audit sa_f26_skill7: behind Vegeta at 142 / 154 facing right, up; in front of him once below 154 facing
        // down) - here the depth relative to the actors' y: y - 4
        Body::Shot(s) => {
            // (Masenko facing up: the cursor 0x08021594 and the charging ball sorted with the actors by their y,
            // behind Gohan - the cursor under his body, the ball under his hair; skill audit sa_f10_skill17_lv1_up)
            // (once launched the ball is in front again)
            let mz_up = s.dir == 1
                && match &s.kind {
                    ShotKind::MasenkoCursor(_) => true,
                    ShotKind::MasenkoBall(b) => b.phase == crate::shots::MPhase::Charge,
                    _ => false,
                };
            // (Burning Attack 0x0804BFA0 too: layer 0x38, depth its y - behind Trunks facing up / left / right; facing
            // down its layer is 0x37, in front - skill audit sa_f21_skill4)
            // (the player's blasts only: an enemy's Energy Blast is another object, in front of the actors - pixel
            // gate sb_010_20_04 f150, Pui Pui's blast over the explosion clouds)
            // (an Energy Blast fired up is in front again, over an explosion cloud - pixel gate earth_2_10_2 f690:
            // Gohan's blast up the screen over the cloud of the one before)
            // (an enemy's blast of the player's class 0x0804B454 - `shots::ki_blast`, the KiBlast node - is sorted the
            // same: Pikkon's behind his arm, emulator sort node layer 0x38 depth y; opponent audit oa_t5b_idle f273)
            let by_player = w.find(s.owner).is_some_and(|k| matches!(w.ents[k].body, Body::Player(_)))
                || matches!(&s.kind, ShotKind::Straight(o) if o.name == SName::KiBlast && o.fx_end == 16 && !o.fragile);
            let ki = mz_up
                || (by_player
                    && matches!(&s.kind, ShotKind::Straight(o) if (o.name == SName::KiBlast && s.dir != 1) || (o.name == SName::Burning && s.dir != 0)))
                // (the ninja star 0x0804BAA0: layer 0x38, depth its y - behind a ninja throwing it down at an equal
                // depth; opponent audit oa_t4e_above f128)
                || matches!(&s.kind, ShotKind::Straight(o) if o.name == SName::Star)
                // (the Mercenary's rifle bullet too: the new bullet behind him at its muzzle - opponent audit
                // oa_t4b_idle f61, OAM 16x16 after his body)
                || matches!(&s.kind, ShotKind::Straight(o) if o.name == SName::Rifle);
            // a Kamikaze ghost is an actor object (layer 0x38, depth y + 4 like an actor: skill audit sa_f31_skill13,
            // Gotenks' fist in front of the shadow of a ghost above him)
            let ghost = matches!(&s.kind, ShotKind::Ghost(_));
            // the player's beam object (0x080206B8) fired up: layer 0x38 like an actor (0x37 in the other facings), its
            // depth below the player's - Goku's hair and halo over the beam's base (skill audit sa_f02_skill15_up,
            // the object's +8 = 56 facing up, 55 otherwise)
            let ki = ki || (s.dir == 1 && matches!(&s.kind, ShotKind::Kame(_)));
            // Majin Buu's candy beam 0x0804B3AC: layer 0x38, depth its y - behind Super Buu's antenna it comes from
            // (opponent audit oa_t73_idle f22)
            // (and his stretch arm 0x0804CDD4: the player's halo over its fist - oa_t74_idle f77)
            // (the candy beam fired down is in front again, over his legs - oa_t74_above f92: its OAM before his body)
            let ki = ki || matches!(&s.kind, ShotKind::StretchArm { .. }) || (s.dir != 0 && matches!(&s.kind, ShotKind::CandyBeam { .. }));
            (
                if matches!(s.kind, ShotKind::DarkSphere { .. }) {
                    2
                } else if ki || ghost {
                    0
                } else {
                    1
                },
                // (a star at its thrower's depth sorts behind it - the new object at an equal key: oa_t4e_above f128)
                if matches!(&s.kind, ShotKind::Straight(o) if o.name == SName::Star) {
                    s.py() - 5
                } else if ki {
                    s.py() - 4
                } else {
                    s.py()
                },
            )
        }
        Body::Fx(f) => match f.as_ref() {
            // the explosion cloud (run 0x08712FDC) sorts with the actors by its y (skill audit sa_f31_skill13: a
            // ghost's cloud above Gotenks behind him, in every facing)
            // (the rifle's smoke puffs too: `shots::Trail::pre`)
            Fx::Anim { y, sorted: true, .. } => (0, *y - 4),
            Fx::Anim { y, .. } | Fx::Loop { y, .. } => (1, *y),
            // a bomb's chips (0x0800C454: layer 0x38, depth their y) sort with the actors - behind Goku when the
            // Grenadier's grenade bursts at his feet (opponent audit oa_t2b_idle f123)
            Fx::Debris { y, .. } => (0, (y >> 8) - 4),
            Fx::ItMarkers { .. } => (3, 0),
            // Big Bang Kamehameha's flare: in front of the beam's start piece (skill audit sa_f41_skill3)
            Fx::BbkFlare { y, .. } => (1, *y),
            // the beam object's charge state facing up: behind the player (its y - 4 against his; skill audit
            // sa_f19_skill16_up: the sparkle behind Goten's hands)
            Fx::BeamCharge { owner, off, .. }
                if w.actor_by_id(*owner).is_some_and(|a| a.facing == 1) =>
            {
                let a = w.actor_by_id(*owner).unwrap();
                (0, a.py() + off[1].1 - 4)
            }
            // the Flame Kamehameha charge record object facing up: layer 0x38, depth its y (owner y - 2) - behind
            // Trunks (0x37 in front in the other facings; skill audit sa_f21_skill11_up)
            Fx::RecObj { what: crate::fx::RecWhat::FlameCharge, owner, .. }
                if w.actor_by_id(*owner).is_some_and(|a| a.facing == 1) =>
            {
                (0, w.actor_by_id(*owner).unwrap().py() - 2 - 4)
            }
            // the dragon: each segment its own object, sorted with the actors by its own depth (`dragon_keys`)
            Fx::DragonFist { seg, .. } => (0, seg[5][1] >> 8),
            _ => (1, 0),
        },
        Body::Injector(_) | Body::Breakable(_) => (9, 0),
        Body::Pickup(p) => (0, p.bx[3]),
        // a level sign's label is keyed to its prop (`0x0800C300`: the prop object's layer, its depth + 1 - in front
        // of the gate it names; Earth 2-90 f60)
        Body::MapSprite(m) => match m.label.filter(|l| l.1 >= 0).and_then(|l| {
            crate::fieldview::prop_keys(w)
                .into_iter()
                .find(|p| p.0 == l.1 as usize)
        }) {
            Some((_, l, d)) => (l, d + 1),
            None => (0, m.y),
        },
        Body::Npc(n) => (0, n.a.py()),
        Body::Mod(m) => (1, m.y >> 8),
    };
    // (an equal key: the later object in the world list in front - Majin Buu's eat respawns the player at the list's
    // end, shipr_338 8-16 f7050: Buu y + 8 = Vegeta y + 4, Vegeta in front)
    // (Janemba's strike hand joins the list's head: at an equal key behind the older objects - opponent audit
    // oa_t33_left f22, the hand rising on his row under his body)
    let tie = match &e.body {
        Body::Player(p) => e.id.max(p.list_seq) | 0x8000_0000,
        Body::Hand(_) => 0x7fff_ffff - e.id.min(0x7fff_ffff),
        _ => e.id | 0x8000_0000,
    };
    (layer, depth, tie)
}

/// the cel an actor's body shows (sheet frame, flip, halo attach point): what `render_view` draws and what the
/// segment harness compares with the game's (animation, cel, facing) - a player / NPC by its animation state (a
/// script's puppet first for a player), an enemy / hand by its animation index and time
pub fn actor_cel(w: &World, i: usize) -> Option<(u16, bool, Option<(i32, i32)>)> {
    let pick = |sp: &crate::content::Sprite, raw: i32, frame: i32, facing: u8| {
        sp.anim_raw(raw).or_else(|| sp.anim_raw(1)).and_then(|an| {
            let st = an.steps(facing);
            st.get((frame.max(0) as usize).min(st.len().saturating_sub(1)))
                .map(|s| (s.frame, s.flip, s.attach))
        })
    };
    match &w.ents[i].body {
        Body::Player(p) => {
            // the player state's animation (a script's commands reach it too: `player::script_pose`, the frame-range
            // ticks - the leftover puppet of an ended command does not show; real route 12-6 f54: walking)
            let (raw, frame) = (p.ap.raw, p.ap.frame);
            let s = if p.candy_def { Some(45) } else { p.a.sprite };
            // a form whose sprite has no such animation shows no body (the shadow stays): Goku's form 1 (the NPC
            // sprite, walk / idle only) firing an Energy Blast - skill audit sa_f01_skill7
            s.and_then(|s| w.data.sprite(s))
                .filter(|sp| p.candy_def || sp.anim_raw(raw).is_some())
                .and_then(|sp| pick(sp, raw, frame, p.a.facing))
        }
        Body::Npc(n) => {
            n.a.sprite
                .and_then(|s| w.data.sprite(s))
                .and_then(|sp| pick(sp, n.anim.raw, n.anim.frame, n.a.facing))
        }
        // an enemy by its animation controller's position (`eanim`: raw, cel - obj+0x10's state as the game draws it)
        // (a script's puppet: the scene animation - the controller holds still under it; Kid Buu's scenes, 14-1)
        Body::Enemy(n) => {
            let (raw, frame) = crate::scene::scene_anim(w, i)
                .map(|a| (a.raw, a.frame))
                .unwrap_or(if n.ty == 0x33 && !n.part && n.ea.pos.raw == 20 {
                    // Janemba's body under the ascent's controller: the frame byte lags a frame (battles too)
                    (n.shown_prev.0, n.shown_prev.1)
                } else {
                    (n.ea.pos.raw, n.ea.pos.frame)
                });
            n.drawn_sprite
                .0
                .or(n.a.sprite)
                .and_then(|s| w.data.sprite(s))
                .and_then(|sp| pick(sp, raw, frame, n.a.facing))
        }
        Body::Hand(h) => {
            h.a.sprite
                .and_then(|s| w.data.sprite(s))
                .and_then(|sp| h.a.anim.and_then(|k| sp.anims.get(k as usize)))
                .and_then(|a| {
                    a.step_index_at(h.a.facing, h.a.anim_t, false).map(|k| {
                        (
                            a.steps(h.a.facing)[k].frame,
                            a.steps(h.a.facing)[k].flip,
                            None,
                        )
                    })
                })
        }
        _ => None,
    }
}

fn actor_draw(
    w: &World,
    id: u32,
    a: &crate::actor::Actor,
    step: Option<(u16, bool, Option<(i32, i32)>)>,
    out: &mut RenderView,
) {
    let Some(sp) = a.sprite.and_then(|s| w.data.sprite(s)) else {
        return;
    };
    // the hit flash (0x0800E8E6): the tiles through colour-index map counter >> 2 (2 = the red ramp) while the
    // counter runs (`ui/hit_flash.json`; sb_007 6-9 f19380: Super Buu red under Gohan's Kamehameha)
    // (the map the last tick set: a hit's 12 shows the earlier map until its first tick - sb_006 6-6 f24179 ..
    // f24184: two plain frames, then red for four, then the tint)
    let effect = if a.flash_map >= 0 {
        SpriteEffect::Flash(a.flash_map as u8)
    } else if a.alpha < 256 {
        SpriteEffect::Blend {
            eva: (a.alpha.clamp(0, 256) >> 4) as u8,
            evb: (16 - (a.alpha.clamp(0, 256) >> 4)) as u8,
        }
    } else {
        SpriteEffect::None
    };
    if let Some((index, flip, attach)) = step {
        // OAM order of an actor: halo, body, shadow
        if let Some((hx, hy)) = attach {
            out.sprites.push(SpriteDraw {
                entity: id,
                sheet: "effects/halo".into(),
                frame: FrameSel::Frame {
                    index: 0,
                    flip: false,
                },
                x: a.px() + hx,
                y: a.py() + a.z + hy,
                priority: 3,
                mirror: false,
                effect: SpriteEffect::None,
            });
        }
        out.sprites.push(SpriteDraw {
            entity: id,
            sheet: format!("sprites/{}", sp.id),
            frame: FrameSel::Frame { index, flip },
            x: a.px(),
            y: a.py() + a.z,
            priority: 3,
            mirror: false,
            effect,
        });
    }
    // a 0 x 0 header shadow is no shadow (Yemma's office NPCs requested effects/shadow_0x0)
    // (an actor whose footprint is off every screen shows no shadow: the game makes no OAM for it, and the shadow's
    // byte offsets (`Sprite::shadow_off`) would bring Dabura's back from 58 px left of the screen - pixel gate
    // shipr_338_8-16 f450 - or from above it, shipr_238_8-16 f5010)
    let fp = crate::geom::Rect::new(a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h);
    let off_screen = !w.cameras.is_empty()
        && w.cameras.iter().all(|c| fp.x1 <= c.shown.x0 || fp.x0 >= c.shown.x1 || fp.y1 <= c.shown.y0 || fp.y0 >= c.shown.y1);
    if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0" && !off_screen) {
        out.sprites.push(SpriteDraw {
            entity: id,
            sheet: format!("effects/{sh}"),
            frame: FrameSel::Frame {
                index: 0,
                flip: false,
            },
            // (offset by the header's shadow dx / dy: Super Janemba's at (8, 8) - OAM, real route 18-14)
            x: a.px() + shadow_at(w, id, sp).0,
            y: a.py() + shadow_at(w, id, sp).1,
            priority: 3,
            mirror: false,
            // an enemy's shadow going up off the screen is dropped while fewer than 5 of its OBJ's rows are on screen
            // (the emulator, opponent audit oa_t4e_above f170 .. f182: a ninja walking up off the screen shows its 16x8
            // shadow at tops -1 .. -3, none at -4 .. -7 while its feet still show; oa_t30 / oa_t2c / oa_t6c above the
            // same; pixel gate shipr_338_8-16 f4230: Super Buu's 32x16 one at -9 shows); the player's and an NPC's
            // are not cut so (pixel gate db6_019 13-50 ..) - judged by the camera the picture is drawn with (the
            // presenter's OBJ delay: pixel gate rsbi_010_21_11 f120)
            effect: if w.find(id).is_some_and(|k| matches!(w.ents[k].body, Body::Enemy(_))) {
                SpriteEffect::TopCull { h: if sh.as_str() == "shadow_24x10" { 16 } else { 8 } }
            } else {
                SpriteEffect::None
            },
        });
    }
}

/// The shadow's offset from the sprite header's dx / dy bytes: an enemy's shadow object takes them signed (Dabura
/// fought in a battle: his at (-8, -4) under his feet - opponent audit oa_t12_idle; Yamu's dy 0xF8 8 px up -
/// oa_t94), every other actor's (players, scene NPCs) unsigned - the story's Dabura (an NPC) shows none: dx 248 puts
/// it off screen (pixel gate shipr_238_8-16 f5040 .., his dy 0xFC). Only these two sprites have a byte >= 0x80.
fn shadow_at(w: &World, id: u32, sp: &crate::content::Sprite) -> (i32, i32) {
    let enemy = w.find(id).is_some_and(|k| matches!(w.ents[k].body, Body::Enemy(_)));
    if enemy {
        (sp.shadow_off.0 as u8 as i8 as i32, sp.shadow_off.1 as u8 as i8 as i32)
    } else {
        sp.shadow_off
    }
}

fn record(w: &World, id: u32, addr: u32, x: i32, y: i32, out: &mut RenderView) {
    if let Some((eff, idx)) = w.data.record_frames.get(&addr) {
        out.sprites.push(SpriteDraw {
            entity: id,
            sheet: format!("effects/{eff}"),
            frame: FrameSel::Frame {
                index: *idx,
                flip: w.data.record_flip.contains(&addr),
            },
            x,
            y,
            priority: 3,
            mirror: false,
            effect: SpriteEffect::None,
        });
    }
}
/// a beam (draw 0x08020DAC; reference fx.ts `BeamGfx.draw`): the start piece while firing (the tail while it
/// retracts), the head (the impact piece once a hit / wall stopped it) at position + dir * len, then the body pieces
/// from the position: 32-px ones while `len + 7 - step > o`, one 16 while `len - 9 > o`, one 8 while `len - 1 > o`.
/// Piece variants: phase = t >> 2; mode 2: phase & 3 (3 -> 1), else phase & 1; the body's: mode 1 phase & 3.
/// Stacking (the skill audit against the game, every facing): the body pieces at the back, the start / tail piece over
/// them, the head / impact in front - pushed here in the reverse order (the first OAM entry on top).
#[allow(clippy::too_many_arguments)]
fn beam(w: &World, id: u32, set: &str, dir: i32, x: i32, y: i32, len: i32, t: i32, firing: bool, stopped: bool, out: &mut RenderView) {
    let Some(b) = w.data.beams.get(set) else { return };
    let d = (dir & 3) as usize;
    let phase = t >> 2;
    let k = if b.mode == 2 {
        match phase & 3 {
            3 => 1,
            v => v,
        }
    } else {
        phase & 1
    };
    let body_v = if b.mode == 1 { phase & 3 } else { k };
    let (dx, dy) = crate::geom::DIR_VEC[d];
    let mut ps: Vec<SpriteDraw> = Vec::new();
    let mut piece = |name: &str, v: i32, px: i32, py: i32| {
        let Some(vs) = b.pieces.get(name).map(|p| &p[d]) else { return };
        let Some(&(index, flip)) = vs.get(v as usize).or(vs.first()) else { return };
        ps.push(SpriteDraw {
            entity: id,
            sheet: format!("effects/{set}"),
            frame: FrameSel::Frame { index, flip },
            x: px,
            y: py,
            priority: 3,
            mirror: false,
            effect: SpriteEffect::None,
        });
    };
    let mut start = |piece: &mut dyn FnMut(&str, i32, i32, i32)| {
        if firing {
            piece("start", k, x, y);
            if b.flags & 1 != 0 {
                piece("start2", k, x, y);
            }
        } else {
            piece("tail", k, x, y);
            if b.flags & 2 != 0 {
                piece("tail2", k, x, y);
            }
        }
    };
    let (hx, hy) = (x + dx * len, y + dy * len);
    // the head (or the impact) over the body pieces: pushed after them (skill audit sa_f02_skill15 / sa_f10_skill6 /
    // sa_f19_skill16 f132 ..: the body's last 8 / 16 piece showed over the head's back while the beam grew)
    let mut head = |piece: &mut dyn FnMut(&str, i32, i32, i32)| {
        if stopped {
            piece("impact", k & 1, hx, hy);
            if b.flags & 8 != 0 {
                piece("impact2", k & 1, hx, hy);
            }
        } else {
            piece("head", k, hx, hy);
            if b.flags & 4 != 0 {
                piece("head2", k, hx, hy);
            }
        }
    };
    let step = match b.step[d >> 1] {
        0 => 32,
        s => s,
    };
    let mut o = 0;
    while len + 7 - step > o {
        piece("body32", body_v, x + dx * o, y + dy * o);
        o += step;
    }
    if len - 9 > o {
        piece("body16", body_v, x + dx * o, y + dy * o);
        o += 16;
    }
    if len - 1 > o {
        piece("body8", body_v, x + dx * o, y + dy * o);
    }
    // the body under the start / tail piece, the head over both (skill audit: facing up the start's ring over the
    // body's stripes, sa_f02_skill15_up f131 ..)
    start(&mut piece);
    head(&mut piece);
    out.sprites.extend(ps.into_iter().rev());
}

/// a record placed with its origin added (pieces: reference skillfx.ts `drawRecAdd`, x + 2 * ox)
fn record_add(w: &World, id: u32, addr: u32, x: i32, y: i32, out: &mut RenderView) {
    if let Some(g) = w.data.records.get(&addr) {
        record(w, id, addr, x + 2 * g[0], y + 2 * g[1], out);
    }
}

/// record `index` of the effect run at `run` (the run's step `index`: record run + 12 * index - its own frame and flip;
/// the runs share frames between records, so the step is not the frame: the Energy Blast facing right is steps 12 ..
/// 15 of run 0x0871370C, frames 9 .. 12 - drawn as the missing frames 12 .. 15 it was invisible)
fn effect(w: &World, id: u32, run: u32, index: i32, x: i32, y: i32, out: &mut RenderView) {
    let addr = run.wrapping_add(12 * index.max(0) as u32);
    if w.data.record_frames.contains_key(&addr) {
        return record(w, id, addr, x, y, out);
    }
    out.sprites.push(SpriteDraw {
        entity: id,
        sheet: format!("effects/fx_{run:08x}"),
        frame: FrameSel::Frame {
            index: index.max(0) as u16,
            flip: false,
        },
        x,
        y,
        priority: 3,
        mirror: false,
        effect: SpriteEffect::None,
    });
}

/// the Instant Transmission ring (class 0x080222E0, OAM priority 0; reference skillfx.ts `ItMarkersFx`): record
/// `group + 12 * ((t >> 2) & 3)` with its bottom on (x, y) and again vertically flipped below it
fn it_ring(w: &World, id: u32, group: u32, t: i32, x: i32, y: i32, out: &mut RenderView) {
    let addr = group + 12 * ((t >> 2) & 3) as u32;
    let Some((eff, idx)) = w.data.record_frames.get(&addr) else { return };
    for effect in [SpriteEffect::None, SpriteEffect::VFlip] {
        out.sprites.push(SpriteDraw {
            entity: id,
            sheet: format!("effects/{eff}"),
            frame: FrameSel::Frame { index: *idx, flip: w.data.record_flip.contains(&addr) },
            x,
            y,
            priority: 0,
            mirror: false,
            effect,
        });
    }
}

/// fill `out` from the world (read-only)
pub fn render_view(w: &World, out: &mut RenderView) {
    out.clear();
    out.frame = w.frame as u64;
    // (battle mode: the players show through foreground scenery - not the story, which is the game's picture)
    if w.field.is_none() {
        out.see_through.extend(w.players.iter().copied());
    }
    let map_id = match &w.field {
        Some(f) => f.map.clone(),
        None => w
            .data
            .areas
            .get(&w.arena.map)
            .map(|a| a.0.clone())
            .unwrap_or_default(),
    };
    let mode7 = w.field.as_ref().and_then(|f| {
        f.flight.as_ref().map(|fl| wf_view::Mode7View {
            x: fl.cam[0],
            y: fl.cam[1],
            height: fl.cam[2],
            heading: fl.cam[3],
            pitch: fl.cam[4],
            label: f.label.map(|l| l as i32),
        })
    });
    // the map's clock: the camera object's counter (0x030019A4 +0x40, +1 per map update 0x080092BC - every field
    // iteration, a world held by a scene too; 0 at the area load) drives the wrap layers' drift (0x08008924) and
    // runs with the tile animations' ticks (0x08009CC2): `Field.iters` since the load
    // (counts since a base on a wrapping counter: a cut's base may lie before 0; one just ahead is the load's - 0)
    let since = |it: u32, base: u32| {
        let d = it.wrapping_sub(base);
        if d >= 0x8000_0000 {
            0
        } else {
            d
        }
    };
    let anim_frame = w
        .field
        .as_ref()
        .map(|f| since(f.cam_iters(), f.drift_iter.unwrap_or(f.entered_iter)))
        .unwrap_or(w.frame)
        // (the wrap layers' drift is the update's before the clock's tick: BG HOFS = vel * (clock - 1) - db7_041
        // 0-9's scene, rfat_010 11-17 row 1,200: 201 - 4 * 442 at clock 443)
        .saturating_sub(1);
    // the tile animations' own phase when a cut found it apart from the camera's (rkb 19-3: a scene's area load
    // restarts the tiles, the camera counter runs on)
    let tile_frame = w
        .field
        .as_ref()
        .map(|f| since(f.cam_iters(), f.entered_iter))
        .filter(|&t| t != anim_frame);
    // a script's BG control (hide / priority / blend targets / weights: scene::Layers)
    let sl = &w.scene.screen.layers;
    let layers = (*sl != crate::scene::Layers::default()).then(|| wf_view::BgLayers {
        visible: sl.visible,
        priority: sl.priority,
        blend: sl.blend,
        alpha: sl.alpha,
        offset: sl.offset,
    });
    out.map = Some(MapView {
        id: map_id,
        anim_frame,
        tile_frame,
        // (`shake_off` is 22.10: the BG layers take its whole px - layer objects +0x34 / +0x38, real route sb 6-6
        // f4,693: (3790, -2978) -> (3, -3))
        shake: {
            // (the offset before this frame's shake update: the BG registers are copied before it - the OBJs take
            // the current one, `obj_shake_baked`)
            let s = if w.g.shakes_ticked == w.frame { w.g.shake_shown } else { w.g.shake_off };
            (s.0 >> 10, s.1 >> 10)
        },
        shake_sub: {
            let s = if w.g.shakes_ticked == w.frame { w.g.shake_shown } else { w.g.shake_off };
            (s.0 & 1023, s.1 & 1023)
        },
        mode7: mode7.clone(),
        layers,
        // (the field camera's limits 0 ..= camera_max, `scene::cam_clamp`; a battle arena's view rect)
        frame: match (&w.field, w.arena.view) {
            (Some(f), _) => Some([0, 0, f.camera_max.0 + crate::world::GBA_W, f.camera_max.1 + crate::world::GBA_H]),
            (None, Some(v)) => Some([v.x0, v.y0, v.x1, v.y1]),
            _ => None,
        },
    });
    // the area transition's palette fade (field FadeOut k -> level 8k, FadeIn k -> 256 - 8k; 0x08006D98 / 0x08006DD6)
    if let Some(f) = w.field.as_ref() {
        let level = match f.mode {
            crate::field::Mode::FadeOut { k, step, .. } => crate::field::fade_out_level(k, step),
            crate::field::Mode::FadeIn { k } => crate::field::fade_in_level(w, k),
            crate::field::Mode::Normal => 0,
        };
        // a script's slow fade-out (`warp_fade_slow`, step 4) scales the palette as it is per 5-bit channel, like the
        // script fade_out; the doors' fade writes `0x08007074` from the stored colours (palette RAM, segments
        // rclose_002 4-1 f1,388-1,446 / db5_001 12-2 f295-300)
        let scaled = matches!(f.mode, crate::field::Mode::FadeOut { step, .. } if step > 0);
        if level > 0 && scaled {
            out.palette_scale = Some(wf_view::PaletteFade {
                level: level as u16,
                target: 0,
            });
        } else if level > 0 {
            out.palette_fade = Some(wf_view::PaletteFade {
                level: level as u16,
                target: 0,
            });
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
            let def = md
                .and_then(|m| m.props.get(k))
                .map(|d| d.visible)
                .unwrap_or(true);
            if p.visible && !def {
                wm.props_shown.push(k as u16);
            } else if !p.visible && def {
                wm.props_hidden.push(k as u16);
            }
        }
        // the patrolling enemies (airships ...): drawn by the renderer like the flyer, at their altitude
        for (i, e) in w.ents.iter().enumerate() {
            let Body::Enemy(en) = &e.body else { continue };
            if e.removed {
                continue;
            }
            let Some((frame, flip, _)) = actor_cel(w, i) else { continue };
            let Some(s) = en.a.sprite.and_then(|s| w.data.sprite(s)) else { continue };
            let z = f.patrols.iter().find(|p| p.entity == e.id).map(|p| p.z).unwrap_or(0);
            wm.actors.push(wf_view::FlyerView { x: en.a.x, y: en.a.y, z, sheet: format!("sprites/{}", s.id), frame, flip });
        }
        wm.minimap_box = w.scene.visuals.iter().rev().find_map(|v| match v.kind {
            crate::scene::VisualKind::Box { w: bw, h: bh } => Some([v.x, v.y, bw, bh]),
            _ => None,
        });
        out.world_map = Some(wm);
    }
    // the BG sphere effect (its tiles are drawn in the update, before the objects)
    // the script markers: screen positions less the def's origin ((1, 1) / (3, 3))
    out.markers = w
        .scene
        .visuals
        .iter()
        .filter_map(|v| match v.kind {
            crate::scene::VisualKind::MarkerA => Some((0u8, v.x - 1, v.y - 1)),
            crate::scene::VisualKind::MarkerB => Some((1u8, v.x - 3, v.y - 3)),
            _ => None,
        })
        .collect();
    out.sphere = w.scene.visuals.iter().find(|v| v.kind == crate::scene::VisualKind::EnergySphere).map(|v| wf_view::SphereView {
        x: v.x,
        y: v.y,
        radius: v.owner as i32,
        // (the colour: the native's 4th argument, kept in `vid` - `t` counts on)
        colour: v.vid as u8,
        jitter: w.scene.sphere_jitter.clone(),
    });
    // Dabura's dark sphere (0x0803AEA8): the same BG0 disc, all colour 2 (0x7FFF), BG0 at priority 1 (BG0CNT
    // 0x1C81, over the sprites) blended (BLDCNT 0x1E41: BG0 first target, BG1-3 + OBJ second) - BLDALPHA 8/8 while
    // it grows, fading EVA = (((128 - r) * 50) >> 6) * 656 >> 12, EVB 16 (re/buu/fxC_sphere.py, fxC_dabura.py)
    if out.sphere.is_none() {
        let dark = w.ents.iter().filter(|e| !e.removed).find_map(|e| match &e.body {
            Body::Shot(s) => match s.kind {
                ShotKind::DarkSphere { drawn, .. } => Some((s.px(), s.py(), drawn >> 8)),
                _ => None,
            },
            _ => None,
        });
        if let Some((x, y, r)) = dark {
            let (eva, evb) = if w.g.sphere_state == 2 {
                (((((128 - r).max(0) * 50) >> 6) * 656) >> 12, 16)
            } else {
                (8, 8)
            };
            out.sphere = Some(wf_view::SphereView { x, y, radius: r, colour: 2, jitter: Vec::new() });
            if let Some(m) = out.map.as_mut() {
                let l = m.layers.get_or_insert_with(|| wf_view::BgLayers { visible: [true; 4], ..Default::default() });
                l.priority[0] = Some(1);
                l.blend = [Some(1), Some(2), Some(2), Some(2)];
                l.alpha = Some((eva as u8, evb));
            }
        }
    }
    // the countdown / stopwatch display (`show_timer_hud` 0xB0 / 0xBC: object 0x0802FCA0)
    // a script's title text (`show_title_text` 0xB2)
    if let Some(d) = w.data.menu.as_ref() {
        out.title = w
            .scene
            .titles
            .iter()
            .map(|t| wf_view::TitleView {
                text: d.text.s(t.string.max(0) as usize).to_vec(),
                x: t.x,
                y: t.y,
            })
            .collect();
    }
    out.damage_numbers = crate::vram::damage_numbers(w)
        .into_iter()
        .map(|(x, y, value, crit, heal, enemy)| wf_view::DamageNumberView { x, y, value, crit, heal, enemy })
        .collect();
    out.timer = w.scene.timer_hud.map(|k| match k {
        crate::scene::TimerHud::Countdown => w.scene.timers.countdown,
        crate::scene::TimerHud::Stopwatch => w.scene.timers.stopwatch,
    });
    // a battle's survive fight: the time left on the same countdown display
    if out.timer.is_none() {
        out.timer = crate::battle::survive_left(w).map(|f| f as i32);
    }
    // boss HP bars (the shown one, one sliding out)
    if let Some(b) = &w.scene.boss_bar {
        out.boss_bars.push(wf_view::BossBarView {
            y: b.y(w.frame),
            fill: b.fill(w).clamp(0, 53) as u8,
        });
    }
    if let Some((b, y)) = w
        .scene
        .boss_bar_out
        .as_ref()
        .and_then(|b| Some((b, b.y_at(w.frame)?)))
    {
        out.boss_bars.push(wf_view::BossBarView {
            y,
            fill: b.fill.clamp(0, 53) as u8,
        });
    }
    // (a purge for a text box suspends the banner object: not drawn while the box is up - rkb_006 19-3 f30)
    if let Some((text, t)) = w
        .scene
        .banner
        .as_ref()
        .filter(|b| b.1 <= 238)
        .filter(|_| !crate::vram::owner_suspended(w, crate::vram::Owner::Banner))
    {
        out.banner = Some(wf_view::BannerView {
            text: text.clone(),
            t: *t,
        });
    }
    // the draw order: the world-list objects by (layer, depth) - the actors and the field's props (each prop its own
    // object, `fieldview::prop_keys`); at an equal key a prop is behind
    type Entry = ((i32, i32, u32), Option<usize>, usize);
    let live = |i: usize| !w.ents[i].removed && !crate::vram::suspended(w, w.ents[i].id);
    let is_dragon = |i: usize| matches!(&w.ents[i].body, Body::Fx(f) if matches!(f.as_ref(), Fx::DragonFist { .. }));
    let mut order: Vec<Entry> = (0..w.ents.len())
        .filter(|&i| live(i) && !is_dragon(i))
        .map(|i| (draw_key(w, i), Some(i), 0))
        .collect();
    order.extend(
        crate::fieldview::prop_keys(w)
            .into_iter()
            .map(|(k, l, d)| ((l, d, 0), None, k)),
    );
    // (the game sorts the frame's draw array with an unstable quicksort - 0x080115B0 / 0x080531A4, median of three,
    // comparator 0x08052EE0 - over the drawable world-list objects from the list's last to its first; an equal key
    // comes out where the partitions leave it, which depends on every object in the array: the emulator's arrays
    // (work/ub/l2/arr.py) reproduce exactly with it, but the array holds objects whose sort keys are never set - the
    // HUD's stat-points mark read (56, 329) in 13-45 and (255, 141541984) in 2-11: heap leftovers - so the tie order
    // cannot be derived in general. Kept: at an equal key the later list object in front, which all locked pixel-gate
    // segments show)
    order.sort_by_key(|o| (o.0, o.2));
    // Dragon Fist Explosion: the chain builder 0x08038AD8 makes six objects, each placed among the others by its own
    // depth (Gogeta in front of a segment above him, behind one below), but among themselves in a fixed order: the
    // head in front down to the root (facing up: the root in front) - skill audit sa_f41_skill5 in every facing and
    // steered, the game's OAM. Merged back to front into the sorted list; the third field = 1 + k
    for i in (0..w.ents.len()).filter(|&i| live(i) && is_dragon(i)) {
        let Body::Fx(f) = &w.ents[i].body else { continue };
        let Fx::DragonFist { seg, dir, shifted, .. } = f.as_ref() else { continue };
        let seg = dragon_drawn(w, seg, *dir, *shifted);
        let (l, _, id) = draw_key(w, i);
        // (the actors sort by their footprint's bottom, a segment by its y: here against the actors' top, the
        // segment's y less the owner's footprint height - sa_f41_skill5_steer f127, a segment 3 px below Gogeta's top
        // still behind him)
        let owner = match f.as_ref() {
            Fx::DragonFist { owner, .. } => *owner,
            _ => 0,
        };
        let fh = w.actor_by_id(owner).map(|a| a.foot_h).unwrap_or(0);
        let seq: Vec<usize> = if *dir == 1 { vec![5, 4, 3, 2, 1, 0] } else { vec![0, 1, 2, 3, 4, 5] };
        let mut merged: Vec<Entry> = Vec::with_capacity(order.len() + 6);
        let mut rest = order.into_iter().peekable();
        for k in seq {
            let key = (l, (seg[k][1] >> 8) - fh, id);
            while rest.peek().is_some_and(|o| o.0 <= key) {
                merged.push(rest.next().unwrap());
            }
            merged.push((key, Some(i), k + 1));
        }
        merged.extend(rest);
        order = merged;
    }
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
                // Majin Buu's candy: the def is 0x36 (sprite 45, the sweets); eaten: not in the world
                let mut a = std::borrow::Cow::Borrowed(&p.a);
                if p.candy_def {
                    a.to_mut().sprite = Some(45);
                }
                // the defuse's tint (s503 `tint`: `ui/hit_flash.json` maps[3 + preset]) unless a hit flash shows
                if let Some(k) = p.defuse.and_then(|d| crate::player::defuse_tint(d.0 + 1)).filter(|_| p.a.flash_map < 0) {
                    a.to_mut().flash_map = (3 + k) as i8;
                }
                let step = actor_cel(w, i);
                if p.a.alpha > 0 && !p.gone {
                    // the energy shield (block + B: object 0x0801DFE0, made after the player - in front of it): record
                    // power >> 3 of run 0x087202E8 (0 .. 2 fading in, 3 .. 6 the loop) on the body box centre (base
                    // draw 0x08019C44; reference player.ts SHIELD_FX)
                    if let Some((pow, _)) = p.shield {
                        let (cx, cy) = p.a.center();
                        effect(w, id, 0x0872_02e8, pow >> 3, cx, cy, out);
                    }
                    actor_draw(w, id, &a, step, out);
                }
            }
            // a volleyball (Ultra Volleyball Attack's state 0x10): 0x0803C830 draws the struck enemy as sprite 281 (the
            // golden orb), raw animation 1, frame 0, facing right (reference enemy.ts)
            Body::Enemy(n) if n.volleyball && n.a.alpha > 0 => {
                let mut a = n.a.clone();
                a.sprite = Some(VOLLEYBALL_SPRITE);
                a.facing = 3;
                let step = w
                    .data
                    .sprite(VOLLEYBALL_SPRITE)
                    .and_then(|sp| sp.anim_raw(1))
                    .and_then(|an| an.steps(3).first().map(|s| (s.frame, s.flip, s.attach)));
                actor_draw(w, id, &a, step, out);
            }
            Body::Enemy(n) => {
                let step = actor_cel(w, i);
                if n.a.alpha > 0 {
                    // Janemba's body: the height before this frame's AI step (`Enemy::z_prev`; battles too)
                    let mut a = std::borrow::Cow::Borrowed(&n.a);
                    if n.ty == 0x33 && !n.part {
                        a.to_mut().z = n.z_prev;
                    }
                    if n.drawn_sprite.0.is_some() {
                        a.to_mut().sprite = n.drawn_sprite.0;
                    }
                    actor_draw(w, id, &a, step, out);
                }
            }
            Body::Hand(h) => {
                let step = actor_cel(w, i);
                actor_draw(w, id, &h.a, step, out);
            }
            Body::Rock { x, y, hp } => {
                effect(w, id, 0x0875_f7ac, if *hp >= 2 { 0 } else { 1 }, *x, *y, out)
            }
            Body::Fx(f) => match f.as_ref() {
                Fx::Anim {
                    id: run, x, y, rec, ..
                }
                | Fx::Loop {
                    id: run, x, y, rec, ..
                } => effect(w, id, *run, *rec, *x, *y, out),
                // the beam object in its charge state (0x080206B8 state 0): the sparkle at owner + off[facing], record
                // charge_index for t < 4, then charge_index + 1 + ((t >> 2) & 1) (reference fx.ts `drawCharge`; the
                // set: the player's selected beam skill at its level, an enemy's Kamehameha Lv1)
                Fx::BeamCharge { owner, off, set, t } => {
                    let who = w.find(*owner);
                    let a = who.and_then(|k| w.actor(k));
                    let name = match (set, who.map(|k| &w.ents[k].body)) {
                        (crate::fx::BeamSet::Other, Some(Body::Player(p))) => {
                            crate::shots::beam_set(w.data.skill_kind(p.skill()), p.skill_level)
                        }
                        _ => "beam_kamehameha_1".to_string(),
                    };
                    if let (Some(a), Some((run, ci))) = (a, w.data.beams.get(&name).and_then(|b| b.charge.clone())) {
                        let r = if *t < 4 { ci } else { ci + 1 + ((t >> 2) & 1) };
                        let (ox, oy) = off[(a.facing & 3) as usize];
                        // the run's record r (12-byte records from the run's address; the reference draws the run's
                        // step r - not frame r: run 0x08721164 shares frames, Lv3's charge records 12..14 are frames
                        // 9..11)
                        match u32::from_str_radix(run.trim_start_matches("fx_"), 16) {
                            Ok(base) if w.data.record_frames.contains_key(&(base + 12 * r.max(0) as u32)) => {
                                record(w, id, base + 12 * r.max(0) as u32, a.px() + ox, a.py() + oy, out)
                            }
                            _ => out.sprites.push(SpriteDraw {
                                entity: id,
                                sheet: format!("effects/{run}"),
                                frame: FrameSel::Frame { index: r.max(0) as u16, flip: false },
                                x: a.px() + ox,
                                y: a.py() + oy,
                                priority: 3,
                                mirror: false,
                                effect: SpriteEffect::None,
                            }),
                        }
                    }
                }
                Fx::Debris { x, y, .. } => out.sprites.push(SpriteDraw {
                    entity: id,
                    sheet: "effects/debris".into(),
                    frame: FrameSel::Frame {
                        index: 0,
                        flip: false,
                    },
                    x: x >> 8,
                    y: y >> 8,
                    priority: 3,
                    mirror: false,
                    effect: SpriteEffect::None,
                }),
                Fx::DragonFist {
                    seg, dir, dying, shifted, ..
                } => {
                    let seg = dragon_drawn(w, seg, *dir, *shifted);
                    // segment 5 is the head (the steered end: record dir * 4 + 1, dying 1 + (t >> 3)), 0 .. 4 the
                    // body back to the root; the head on top (facing up: the root on top) - reference skillfx.ts
                    // DragonFistFx `draw` (later on top there; first OAM entry on top here)
                    // (one segment per draw-order entry: `prop` = 1 + k)
                    let order: Vec<usize> = if prop > 0 {
                        vec![prop - 1]
                    } else if *dir == 1 {
                        vec![0, 1, 2, 3, 4, 5]
                    } else {
                        vec![5, 4, 3, 2, 1, 0]
                    };
                    for k in order {
                        let ix = if k != 5 {
                            0
                        } else if *dying < 0 {
                            1
                        } else {
                            // (the head's tiles are re-uploaded into its OAM entry: the step shows a frame ahead of
                            // the presenter's OAM delay - skill audit sa_f41_skill5 f190 / f198)
                            1 + ((dying + 1) >> 3).min(2)
                        };
                        record(
                            w,
                            id,
                            0x0877_ad10 + 12 * (*dir as u32 * 4 + ix as u32),
                            seg[k][0] >> 8,
                            seg[k][1] >> 8,
                            out,
                        );
                    }
                }
                // Instant Transmission: the cursor ring (group A 0x0872BF04) on the selection point, the target rings
                // (group B 0x0872BF34) on the picked enemies' centres; cursor in front
                Fx::ItMarkers {
                    t,
                    alive: crate::fx::Alive::It { p, serial },
                } => {
                    let sel = w.find(*p).and_then(|k| match &w.ents[k].body {
                        Body::Player(pl) => pl.it_sel.as_ref().filter(|s| s.serial == *serial),
                        _ => None,
                    });
                    if let Some(s) = sel {
                        if s.cursor {
                            it_ring(w, id, 0x0872_bf04, *t, s.x >> 8, s.y >> 8, out);
                        }
                        for e in &s.picks {
                            let c = w.find(*e).and_then(|k| match &w.ents[k].body {
                                Body::Player(q) => Some(q.a.center()),
                                Body::Enemy(n) => Some(n.a.center()),
                                Body::Npc(n) => Some(n.a.center()),
                                _ => None,
                            });
                            if let Some((cx, cy)) = c {
                                it_ring(w, id, 0x0872_bf34, *t, cx, cy, out);
                            }
                        }
                    }
                }
                // record objects (class 0x080507A0): the Flame Kamehameha charge (run 0x08721164 record 12 + f, f = t >> 3,
                // > 3 -> 2 + (f & 1), at owner + FLAME_CHARGE) and the Energy Punch Lv3 aura (0x08762270 + 12 * (3 * dir
                // + (state tick >= 56)) at owner + ENERGY_PUNCH_POS) - reference player.ts RecObj
                Fx::RecObj { what, owner, t, .. } => {
                    if let Some(pk) = w.find(*owner) {
                        if let Body::Player(p) = &w.ents[pk].body {
                            let d = (p.a.facing & 3) as usize;
                            let (addr, (ox, oy)) = match what {
                                crate::fx::RecWhat::FlameCharge => {
                                    let f = t >> 3;
                                    let r = 12 + if f > 3 { 2 + (f & 1) } else { f };
                                    (0x0872_1164 + 12 * r as u32, [(8, -2), (12, -2), (12, -2), (4, -2)][d])
                                }
                                crate::fx::RecWhat::EnergyPunch => (
                                    0x0876_2270 + 12 * (3 * d as u32 + (p.ap.tick >= 56) as u32),
                                    [(13, 8), (0, -19), (-8, -3), (24, -3)][d],
                                ),
                            };
                            record(w, id, addr, p.a.px() + ox, p.a.py() + oy, out);
                        }
                    }
                }
                // Energy Sword (table 0x0876CBD8) at the owner: charging records base + min(4, min(79, t) >> 4) (base 0,
                // facing right 5); slashing k = (slash - 1) & 7: facing right [14,13,12,11,10,17,16,15][k], else 10 + k
                // (reference skillfx.ts EnergySwordFx)
                // (skill audit sa_f39_skill9, the sword object 0x0804B4AC's record +0x18: charging from its counter
                // before this tick's increment, (t - 1) >> 4 - the step a frame after ours; slashing it follows the
                // player's animation frame, k = frame - 2 - not a per-tick count: the slash frames last 1-2 ticks)
                Fx::EnergySword { owner, slash, t, .. } => {
                    if let Some(a) = w.actor_by_id(*owner) {
                        let right = a.facing == 3;
                        let fr = w.find(*owner).and_then(|k| match &w.ents[k].body {
                            Body::Player(pl) => Some(pl.ap.frame),
                            _ => None,
                        });
                        let rec = if *slash >= 0 && fr.is_some_and(|f| f < 2) {
                            // (the slash state's first tick: the player's frame not yet 2, the full sword still drawn)
                            (if right { 5 } else { 0 }) + 4
                        } else if *slash >= 0 {
                            let k = match fr {
                                Some(f) if f >= 2 => ((f - 2) & 7) as usize,
                                _ => ((slash - 1).max(0) & 7) as usize,
                            };
                            if right {
                                [14, 13, 12, 11, 10, 17, 16, 15][k]
                            } else {
                                10 + k as i32
                            }
                        } else {
                            (if right { 5 } else { 0 }) + 4.min(79.min(*t - 1).max(0) >> 4)
                        };
                        record(w, id, 0x0876_cbd8 + 12 * rec as u32, a.px(), a.py(), out);
                    }
                }
                // Big Bang Kamehameha flare (effects/bbk_flare): flare + mirror piece k = (t >> 2) & 3, the ball
                // ((t >> 2) & 1) until the attack ends (reference skillfx.ts BbkFlareFx)
                Fx::BbkFlare { x, y, dir, t, ended, alive: al } => {
                    // (the flare animates by re-uploading its tiles into a fixed OAM entry: the picture changes in the
                    // frame of the step, a frame ahead of the presenter's OAM delay - skill audit sa_f41_skill3)
                    let t = &(t + 1);
                    let k = ((t >> 2) & 3) as u32;
                    let mut ps = vec![("flare", 4 * k), ("mirror", 4 * k)];
                    // (the ball goes with the beam's release in the same frame: sa_f41_skill3 f237)
                    if !*ended && crate::fx::alive(w, *al) {
                        ps.push(("ball", 4 * ((t >> 2) & 1) as u32));
                    }
                    // (OAM order: the flare, its mirror, then the ball - the flare in front)
                    for (anim, tt) in ps {
                        out.sprites.push(SpriteDraw {
                            entity: id,
                            sheet: "effects/bbk_flare".into(),
                            frame: FrameSel::Anim { anim: anim.into(), dir: *dir, t: tt, looping: true },
                            x: *x,
                            y: *y,
                            priority: 3,
                            mirror: false,
                            effect: SpriteEffect::None,
                        });
                    }
                }
                // Flame Kamehameha links (table 0x087568B4 + 0x54 * dir + 12 * frame): non-last links cycle 2,1,2,3 from
                // frame 4, the last 4,5,6,5 from 7; shifted across the facing by the stored half-steps (reference
                // skillfx.ts FlameKameFx)
                Fx::FlameKame { dir, n, links, .. } => {
                    let (dx, _) = crate::geom::DIR_VEC[(*dir & 3) as usize];
                    for (k, l) in links.iter().enumerate().rev() {
                        let mut f = l[2] >> 2;
                        if k as i32 == n - 1 {
                            if f >= 7 {
                                f = (f & 3) + 4;
                                if f == 7 {
                                    f = 5;
                                }
                            }
                        } else if f >= 4 {
                            f &= 3;
                            if f == 0 {
                                f = 2;
                            }
                        }
                        let sh = l[3] / 2;
                        let (lx, ly) = (l[0] + if dx != 0 { 0 } else { sh }, l[1] + if dx != 0 { sh } else { 0 });
                        record(w, id, 0x0875_68b4 + 0x54 * *dir as u32 + 12 * f as u32, lx, ly, out);
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
                                (
                                    0x0871_370c,
                                    if o.dead >= 0 {
                                        16
                                    } else {
                                        d * 4 + if f > 3 { 2 + (f & 1) } else { f }
                                    },
                                )
                            }
                            SName::Gun => {
                                (0x0874_e968, d + 4 * [0, 1, 2, 1][((t >> 2) & 3) as usize])
                            }
                            SName::Rifle => (0x0874_e614, d),
                            SName::Shell => (0x0871_e630, d),
                            SName::Star => (0x0875_4a1c, (t >> 3) & 1),
                            SName::Wrap | SName::WrapDiag => (
                                0x0875_71f4,
                                3 * (d >> 1)
                                    + ((t % 24) >> 3)
                                    + if o.vx != 0 && o.vy != 0 { 6 } else { 0 },
                            ),
                            SName::Spit => (0x0875_79b8, o.sector + ((t >> 2) & 1)),
                            SName::Barrage => (0x0872_d6b8, 5 * d + (t >> 2).min(4)),
                            SName::Burning => (
                                0x0875_01ec,
                                if o.dead >= 0 {
                                    2 + ((t >> 3) & 1)
                                } else {
                                    (t >> 3) & 1
                                },
                            ),
                            SName::Volley => (0x0876_e264, 3 * d + 2),
                        };
                        // a ki blast's burst keeps the box of its last flight cel (0x08019B4E sets the box from the
                        // cel record's offsets, the draw 0x08019C10 puts the OAM at its top-left; the burst sets
                        // none): after a long cel (cels 2 / 3, from t = 16: offsets 4 x 8 across, 8 x 4 along x) the
                        // 16 x 16 burst sits 4 px right / down (emulator, opponent audit oa_t23_ki f64: hit on cel 14)
                        let (x, y) = match o.name {
                            SName::KiBlast if o.dead >= 0 && (t - o.dead as i32) >> 3 >= 2 => {
                                if d < 2 {
                                    (x + 4, y)
                                } else {
                                    (x, y + 4)
                                }
                            }
                            _ => (x, y),
                        };
                        effect(w, id, run, idx, x, y, out);
                    }
                    ShotKind::Flame { .. } => {
                        if (t >> 2) <= 4 {
                            record(
                                w,
                                id,
                                0x0872_d6b8 + 60 * d as u32 + 12 * (t >> 2) as u32,
                                x,
                                y,
                                out,
                            );
                        }
                    }
                    ShotKind::Bomb { .. } => {
                        let mut k = (t >> 2) & 3;
                        if k == 3 {
                            k = 1;
                        }
                        record(w, id, 0x0871_e984 + 12 * k as u32, x, y, out);
                    }
                    // a mummy bandage, flying or stuck on the player (records 0x087571F4 + 12 * its cel: `wrap_update`)
                    ShotKind::Wrap(m) => effect(
                        w,
                        id,
                        0x0875_71f4,
                        m.cel,
                        x,
                        y,
                        out,
                    ),
                    ShotKind::SjWave => record(w, id, 0x0875_a6a8 + 12 * d as u32, x, y, out),
                    // an energy wave (records 0x0875043C + 36 * dir: muzzle k = ((t + 1) >> 2) & 1 at the origin while
                    // growing in mode 0, body record 2 every 8 px) and Majin Buu's candy beam (0x0874ED7C + 60 * dir: below); pieces
                    // at position + origin (reference projectiles.ts EnergyWave / CandyBeam)
                    ShotKind::EnergyWave { len, mode, .. } => {
                        let base = 0x0875_043c + 36 * d as u32;
                        let (dx, dy) = crate::geom::DIR_VEC[(d & 3) as usize];
                        let mut ps = vec![];
                        // (the muzzle's two records swap a frame before t >> 2 would; the OAM in this order - the
                        // muzzle in front of the body: opponent audit oa_t45_left f41 .. f45, the emulator's ring
                        // record 1 from picture 44 and over the body's first piece)
                        if *mode == 0 && *len < 48 {
                            ps.push((base + 12 * (((t + 1) >> 2) & 1) as u32, x, y));
                        }
                        let mut k = 0;
                        while k < len - 1 {
                            ps.push((base + 24, x + dx * k, y + dy * k));
                            k += 8;
                        }
                        for (a, px, py) in ps {
                            record_add(w, id, a, px, py, out);
                        }
                    }
                    ShotKind::CandyBeam { len, .. } => {
                        // (the draw 0x08028AD2: muzzle record k at the origin while len < 64 - retracting too -, head
                        // record 3 + k at dir * len, then body record 2 from the origin out; the OAM in that order,
                        // the head in front of the body; k = the gfx its tiles hold, every 4 frames - emulator OAM /
                        // VRAM, opponent audit oa_t73_idle f22 ..)
                        let base = 0x0874_ed7c + 60 * d as u32;
                        let (dx, dy) = crate::geom::DIR_VEC[(d & 3) as usize];
                        let k = (((t + 1) >> 2) & 1) as u32;
                        let mut ps = vec![];
                        if *len < 64 {
                            ps.push((base + 12 * k, x, y));
                        }
                        ps.push((base + 12 * (3 + k), x + dx * len, y + dy * len));
                        let mut o = 0;
                        while o < len - 1 {
                            ps.push((base + 24, x + dx * o, y + dy * o));
                            o += 8;
                        }
                        // (in OAM order, the first in front)
                        for (a, px, py) in ps {
                            record_add(w, id, a, px, py, out);
                        }
                    }
                    // Buu's stretch arm (records 0x0874EFEC + 24 * dir) and the Bio Mech grapple arm (0x0876DFF8 +
                    // 48 * dir): the arm piece every 8 px while < len - 1, the fist / claw at dir * len on top (the
                    // claw closed on the player while reeling: records 2 (back) and 3 (front)); reference
                    // projectiles.ts StretchArm / GrappleArm `draw`
                    ShotKind::StretchArm { len, .. } | ShotKind::GrappleArm { len, .. } => {
                        let (base, heads) = match &s.kind {
                            ShotKind::GrappleArm { phase: 2, .. } => (0x0876_dff8 + 48 * d as u32, &[24u32, 36][..]),
                            ShotKind::GrappleArm { .. } => (0x0876_dff8 + 48 * d as u32, &[12u32][..]),
                            _ => (0x0874_efec + 24 * d as u32, &[12u32][..]),
                        };
                        let (dx, dy) = crate::geom::DIR_VEC[(d & 3) as usize];
                        let mut ps = vec![];
                        let mut o = 0;
                        while o < len - 1 {
                            ps.push((base, x + dx * o, y + dy * o));
                            o += 8;
                        }
                        for h in heads {
                            ps.push((base + h, x + dx * len, y + dy * len));
                        }
                        for (a, px, py) in ps.into_iter().rev() {
                            record_add(w, id, a, px, py, out);
                        }
                    }
                    // an enemy's beam object in its charge state (Kid Buu's Kamehameha, the class 0x080206B8 state
                    // 0) draws the charge sparkle as the player's does (`Fx::BeamCharge`: owner + its offset, record
                    // ci, then ci + 1 + ((t >> 2) & 1)) - opponent audit oa_t3c_above f26 .., the ball over Kid Buu
                    ShotKind::Kame(k) if k.charging => {
                        const OFF: [(i32, i32); 4] = [(8, -2), (12, -2), (12, -2), (4, -2)];
                        let a = w.actor_by_id(s.owner);
                        if let (Some(a), Some((run, ci))) = (a, w.data.beams.get(&k.set).and_then(|b| b.charge.clone())) {
                            let r = if t < 4 { ci } else { ci + 1 + ((t >> 2) & 1) };
                            let (ox, oy) = OFF[(a.facing & 3) as usize];
                            if let Ok(base) = u32::from_str_radix(run.trim_start_matches("fx_"), 16) {
                                if w.data.record_frames.contains_key(&(base + 12 * r.max(0) as u32)) {
                                    record(w, id, base + 12 * r.max(0) as u32, a.px() + ox, a.py() + oy, out);
                                }
                            }
                        }
                    }
                    ShotKind::Kame(k) if !k.charging => {
                        // (an enemy's beam object (Kid Buu's, class 0x0804AEF8): its counter +0x18 runs a frame ahead of
                        // the shot's t - the head / start cels flip a frame earlier; opponent audit oa_t3c_above f37,
                        // f41 .. (every 4th frame) with t)
                        let by_enemy = w.find(s.owner).is_some_and(|o| matches!(w.ents[o].body, Body::Enemy(_)));
                        let vt = if by_enemy { k.vt + 1 } else { k.vt };
                        beam(w, id, &k.set, d, x, y, k.len, vt, !k.done, k.stopped, out)
                    }
                    // the Final Flash (class 0x08036FA2; tables 0x0875FE2C / 0x08760A8C / 0x087616EC): start record
                    // 2 * dir at the visible origin, body 2 * dir + 1 every 32 px from 16 while < len - 1; growing /
                    // shrinking (scale < 0x100) as affine OBJs: inv = 0x10000 / scale, (pa, pd) down (inv, -0x80), up
                    // (inv, 0x100), left (-0x80, inv), right (0x100, inv) (reference skillfx.ts drawFinalFlash)
                    ShotKind::FullBeam(b) => {
                        if let Some(tb) = b.table {
                            let table = [0x0875_fe2c, 0x0876_0a8c, 0x0876_16ec][(tb as usize).min(2)];
                            let (vx, vy) = b.vis.unwrap_or((x, y));
                            let (dx, dy) = crate::geom::DIR_VEC[(d & 3) as usize];
                            let l = ((x + dx * b.len - vx) * dx + (y + dy * b.len - vy) * dy).max(0);
                            let base = table + 24 * d as u32;
                            let mut ps = vec![(base, vx, vy)];
                            let mut o = 16;
                            while o < l - 1 {
                                ps.push((base + 12, vx + dx * o, vy + dy * o));
                                o += 32;
                            }
                            let eff = if b.scale >= 0x100 {
                                Some(SpriteEffect::None)
                            } else if b.scale > 0 {
                                let inv = (0x10000 / b.scale).min(0x7fff) as i16;
                                let (pa, pd) = [(inv, -0x80), (inv, 0x100), (-0x80, inv), (0x100, inv)][(d & 3) as usize];
                                Some(SpriteEffect::Affine { pa, pd })
                            } else {
                                None
                            };
                            if let Some(effect) = eff {
                                for (addr, px, py) in ps.into_iter().rev() {
                                    if let Some((eff, idx)) = w.data.record_frames.get(&addr) {
                                        out.sprites.push(SpriteDraw {
                                            entity: id,
                                            sheet: format!("effects/{eff}"),
                                            frame: FrameSel::Frame { index: *idx, flip: w.data.record_flip.contains(&addr) },
                                            x: px,
                                            y: py,
                                            priority: 3,
                                            mirror: false,
                                            effect,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // the Big Bang ball (0x08037650): table (level 0 / 1: 0x08764E00, 2: 0x0876AAE0) + dir * 120 + 12 * f
                    ShotKind::BigBang(b) => {
                        let table = if b.lv >= 2 { 0x0876_aae0 } else { 0x0876_4e00 };
                        record(w, id, table + 120 * d as u32 + 12 * b.f.clamp(0, 9) as u32, x, y, out);
                    }
                    // the Ultra Volleyball wave (table 0x0876E264 + 36 * dir, pieces at position + origin): tail at the
                    // position, body every 8 px while < len - 1, head at dir * len (reference drawVolleyPieces)
                    ShotKind::Volley { len, .. } => {
                        let base = 0x0876_e264 + 36 * d as u32;
                        let (dx, dy) = crate::geom::DIR_VEC[(d & 3) as usize];
                        let mut ps = vec![(base, x, y)];
                        // (the pieces span len - 8: the head 8 px short of the box's end - skill audit sa_f31_skill23)
                        let len = &(*len - 8);
                        let mut k = 8;
                        while k < len + 8 - 9 {
                            ps.push((base + 12, x + dx * k, y + dy * k));
                            k += 8;
                        }
                        ps.push((base + 24, x + dx * len, y + dy * len));
                        // (OAM order: the tail first, in front - skill audit sa_f31_skill23, the tail's end over the
                        // head on the wave's first frame)
                        for (a, px, py) in ps.into_iter() {
                            record_add(w, id, a, px, py, out);
                        }
                    }
                    // Masenko: the aiming cursor (record 0x0872509C) until the release; the ball - charging: spark record
                    // L + k (k = (t >> 3) & 3, 3 -> 1), flying: L + 1 + ((t >> 3) & 1) (run 0x08721164, L = 9 / 9 / 12),
                    // its blast: (0x0872A5DC / 0x0872A60C / 0x0872A648) + 12 * min(n - 1, tb >> 2) (reference MasenkoBall)
                    ShotKind::MasenkoCursor(c) => {
                        if c.released < 0 {
                            record(w, id, 0x0872_509c, x, y, out);
                        }
                    }
                    ShotKind::MasenkoBall(m) => {
                        let lv = m.lv.clamp(0, 2) as usize;
                        let spark = [9, 9, 12][lv];
                        let r = match m.phase {
                            crate::shots::MPhase::Charge => {
                                let k = match (t >> 3) & 3 {
                                    3 => 1,
                                    k => k,
                                };
                                0x0872_1164 + 12 * (spark + k) as u32
                            }
                            crate::shots::MPhase::Fly => 0x0872_1164 + 12 * (spark + 1 + ((t >> 3) & 1)) as u32,
                            crate::shots::MPhase::Blast => {
                                let n = [4, 5, 6][lv];
                                [0x0872_a5dc, 0x0872_a60c, 0x0872_a648][lv] + 12 * (n - 1).min(m.tb >> 2) as u32
                            }
                        };
                        record(w, id, r, x, y, out);
                    }
                    // Super Buu's lobbed ball (0x08021280): the spark run 0x08721164 - charging, record 12 + k (k = (t >> 3)
                    // & 3, 3 -> 1); thrown, record 13 + ((tick >> 3) & 1), the object's tick running on from the charge's
                    // 61; drawn where the game's object draws it (`shots::superball_draw`)
                    ShotKind::SuperBall { phase, draw, .. } => {
                        if *phase == 0 {
                            let k = match (t >> 3) & 3 {
                                3 => 1,
                                k => k,
                            };
                            let f = w.actor_by_id(s.owner).map(|a| a.facing).unwrap_or(s.dir);
                            let (dx, dy) = crate::shots::superball_draw(f, None);
                            effect(w, id, 0x0872_1164, 12 + k, x + dx, y + dy, out);
                        } else {
                            let (bx, by) = if *draw != [0; 4] {
                                crate::shots::superball_path(*draw, s.t)
                            } else {
                                let (dx, dy) = crate::shots::superball_draw(s.dir, Some(s.t));
                                (x + dx, y + dy)
                            };
                            effect(w, id, 0x0872_1164, 13 + (((t + 61) >> 3) & 1), bx, by, out);
                        }
                    }
                    // a Kamikaze ghost: the ghost sprite (148) in its own animation state, like an actor
                    ShotKind::Ghost(g) => {
                        let cel =w.data.sprite(crate::shots::GHOST_SPRITE).and_then(|sp| {
                            let an = sp.anim_raw(g.ea.pos.raw).or_else(|| sp.anim_raw(1))?;
                            let st = an.steps(g.facing);
                            st.get((g.ea.pos.frame.max(0) as usize).min(st.len().saturating_sub(1))).map(|s| (sp.id.clone(), s.frame, s.flip))
                        });
                        if let Some((sid, frame, flip)) = cel {
                            out.sprites.push(SpriteDraw {
                                entity: id,
                                sheet: format!("sprites/{sid}"),
                                frame: FrameSel::Frame { index: frame, flip },
                                x,
                                y,
                                priority: 3,
                                mirror: false,
                                effect: SpriteEffect::None,
                            });
                            // its shadow (the sprite header's, after the body like an actor's; skill audit
                            // sa_f31_skill13: every ghost has one in the game)
                            if let Some(sp) = w.data.sprite(crate::shots::GHOST_SPRITE) {
                                if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0" ) {
                                    out.sprites.push(SpriteDraw {
                                        entity: id,
                                        sheet: format!("effects/{sh}"),
                                        frame: FrameSel::Frame { index: 0, flip: false },
                                        x: x + sp.shadow_off.0,
                                        y: y + sp.shadow_off.1,
                                        priority: 3,
                                        mirror: false,
                                        effect: SpriteEffect::None,
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Body::Npc(n) => {
                let step = actor_cel(w, i);
                // a script `tint` (presets of 0x08752EEC: `ui/hit_flash.json` maps[3 + preset])
                let mut a = std::borrow::Cow::Borrowed(&n.a);
                if let Some(k) = n.tint.filter(|_| n.a.flash_map < 0) {
                    a.to_mut().flash_map = (3 + k) as i8;
                }
                actor_draw(w, id, &a, step, out);
            }
            Body::Pickup(p) => crate::fieldview::pickup(w, id, p, out),
            Body::MapSprite(m) => crate::fieldview::map_sprite(w, id, m, out),
            Body::Injector(_) | Body::Breakable(_) => {}
            Body::Mod(m) => crate::modkit::draw(w, id, m, out),
        }
    }
    // cutscene presentation: fade, dialog box
    out.fade_black = crate::scene::fade_black(w);
    script_fade(w, out);
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
                portrait: if text_box {
                    dialog_portrait(w, d.portrait)
                } else {
                    None
                },
                portrait_left: d.portrait_left,
                // a choice's cursor once the box is up
                cursor: (d.options > 0 && d.phase == crate::scene::DialogPhase::Shown)
                    .then_some(d.cursor.max(0) as u8),
            });
        }
    }
    out.dialog_anim = dialog_anim(w);
    for s in 0..w.players.len() {
        let c = w.cameras[s.min(crate::world::MAX_PLAYERS - 1)].shown;
        out.cameras.push(CameraView { x: c.x0, y: c.y0 });
        if let Some(pi) = w.player_idx(s) {
            let p = w.pl_ref(pi);
            let name = w
                .data
                .form(p.form)
                .map(|f| f.name.clone())
                .unwrap_or_default();
            // experience line: steps 0..72 between this level's total and the next (0x0801363E(player, 72))
            let lv = p.rec.level;
            let t = &w.data.xp_table;
            let k = (lv - 50).max(0) as usize;
            let xp72 = match (t.get(k), t.get(k + 1)) {
                (Some(&a), Some(&b)) if b > a => {
                    (((p.rec.xp - a).clamp(0, b - a) * 72) / (b - a)) as u8
                }
                (Some(_), None) => 72,
                _ => 0,
            };
            let skill = p.skill();
            // (the bars: what the HUD's update read in the frame's pass - `World::hud_sample`)
            // (changed since the pass - a script's heal, a menu: the current values)
            let now = (p.hp(), p.max_hp(), p.ep(), p.max_ep());
            let bars = w
                .hud_bars
                .get(s)
                .copied()
                .filter(|_| w.hud_bars_at == w.frame && w.hud_end.get(s) == Some(&now))
                .unwrap_or(now);
            // (the skill icon's grey and the SS / SS3 gauges: read there too - `World::more_values`)
            // the skill icon (grey: not usable) and the EP bar's SS / SS3 gauge parts are tiles the HUD rebuilds from its
            // read (`0x0800B99C`: skill def +0x10's check, then the 12-byte summary): the icon as the frame leaves it
            // (skill audit sa_f26_skill2 f5 / f106: greyed in the press's frame; Masenko Lv3 f5), the gauges as read at
            // the HUD's place in the pass (`World::hud_sample`: the form controller's drain and a punch's gain show a
            // frame later - pixel gate earth_2_8_1 f3630), but a form change in the pass (a revert's end, a transform's)
            // at once (skill audit sa_f02_skill20 f330, sa_f40_skill20 f69)
            let alt = !crate::player::can_use_skill(w, pi, skill) || w.field.as_ref().is_some_and(|f| !f.combat);
            let gauge_now = (p.ss_gauge, (p.ss3_gauge >> 16) as i32);
            let gauge = w
                .hud_more
                .get(s)
                .copied()
                .filter(|_| {
                    w.hud_bars_at == w.frame
                        && w.hud_more_end.get(s).is_some_and(|e| (e.1, e.2) == gauge_now)
                        && w.hud_form.get(s) == Some(&p.form)
                })
                .map(|m| (m.1, m.2))
                .unwrap_or(gauge_now);
            out.hud.push(HudView {
                name,
                level: lv.max(0) as u32,
                hp: bars.0,
                hp_max: bars.1,
                ep: bars.2,
                ep_max: bars.3,
                ss: gauge.0,
                ss3: gauge.1,
                ss_cooldown: p.ss_cooldown,
                xp72,
                skill: (skill > 0).then_some(skill as u8),
                // areas without combat (towns, offices) grey the icon out (emulator HUD)
                skill_alt: alt,
                // (a HUD object a text box's purge suspended is out of the world list: not drawn - real route rkb_168
                // 14-1 f1,622 .. 1,685: show_hud's new HUD, then s-script's text box on the world map)
                // (a hidden HUD still sliding out is drawn: `Scene::hud_y`)
                hidden: (w.scene.hud_hidden && !(w.scene.hud_vel < 0 && w.scene.hud_y >= -16))
                    || crate::vram::owner_suspended(w, crate::vram::Owner::Hud),
                stat_mark: stat_mark_frame(w),
                y_off: w.scene.hud_y - 8,
                // local co-op: the tag over the head (the body box's top) that tells the players apart
                slot: s as u8,
                tag: (w.players.len() > 1 && !p.gone).then(|| {
                    let b = p.a.body_box();
                    ((b.x0 + b.x1) >> 1, b.y0 + p.a.z - 4)
                }),
            });
        }
    }
    obj_shake_baked(w, out);
}

/// the screen shake on the world's OBJs (emulator, sc_janemba f5,130): the camera object 0x030019A4 holds its 22.10
/// position (+0 / +4), its unshaken pixel rect (+0x18 ..) and the shaken pixel position (+0x28 / +0x2C =
/// (pos + shake) >> 10, `0x080097A6` from the shake's update); an actor's draw (`0x08010240`) takes its screen
/// position from the shaken one - so the OBJs move by `((pos + shake) >> 10) - (pos >> 10)`, not `shake >> 10` (the
/// BG layers' whole-pixel offsets). The generic effect objects' draw (`0x0802A912`) reads only the rect: they stay
/// unshaken (an actor's shadow / halo is part of the actor's draw: shaken with it - leaving every `effects/` sheet
/// unshaken cost 300+ exact frames, sb_006 / sb_007). Baked here, at the sprites' build: their OAM is a frame late
/// with them
fn obj_shake_baked(w: &World, out: &mut RenderView) {
    let Some(c) = w.cameras.first() else { return };
    let (ox, oy) = w.g.shake_off;
    let (dx, dy) = (((c.x + ox) >> 10) - (c.x >> 10), ((c.y + oy) >> 10) - (c.y >> 10));
    for s in out.sprites.iter_mut() {
        // (the generic effect objects - Fx::Anim / Loop, class 0x0802A81C - draw from the camera's unshaken rect:
        // the puffs, explosions; board 105 frames fewer px, 17 more - sc_janemba / rj_049 / sb_041 / rkb_168)
        let effect = w.find(s.entity).is_some_and(|k| matches!(&w.ents[k].body, Body::Fx(f) if matches!(f.as_ref(), crate::fx::Fx::Anim { .. } | crate::fx::Fx::Loop { .. })));
        if !effect {
            s.x -= dx;
            s.y -= dy;
        }
    }
    out.obj_shake = Some((0, 0));
}

/// the stat-points mark's animation frame (draw 0x0800B55C: (counter >> 3) & 3), while it exists
fn stat_mark_frame(w: &World) -> Option<u8> {
    let v = w.field.as_ref()?.vram.as_ref()?;
    if !v.owners.iter().any(|o| o.1 == crate::vram::Owner::StatMark) {
        return None;
    }
    // (the draw counts +0x14 up once per draw - every field iteration and the menu fades' camera ticks: the camera
    // clock; measured equal to the game's counter, real route rclose_067 14-2 / shipr_227 7-2 / dr1_001 9-2)
    let it = w.field.as_ref().map(|f| f.cam_iters()).unwrap_or(0);
    Some(((it.wrapping_sub(v.stat_at) >> 3) & 3) as u8)
}

/// the script screen fades as the palette writes they make each update (`t` steps of `len`, l = t * 256 / len):
/// fade_out (`0x080177D2` -> `0x080176D6`) / fade_out_white (`0x0801782C` -> `0x0801774C`) scale the palette as
/// it was toward black / white by l; fade_in (`0x08017806` -> `0x080070CC`) / fade_in_white (`0x0801785C` ->
/// `0x080070DA`) write the fade routine `0x08007074` from the stored colours at 256 - l toward black / white;
/// flash_white (`0x08017888`, len = 2 n) toward white at t * 256 / n, then (2 n - t) * 256 / n. A finished fade to
/// white leaves the palette white (`Screen.white` 16)
fn script_fade(w: &World, out: &mut RenderView) {
    use crate::scene::FadeKind;
    let s = &w.scene.screen;
    const WHITE: u32 = 0xFF_FFFF;
    let pf = |level: i32, target: u32| wf_view::PaletteFade {
        level: level.clamp(0, 256) as u16,
        target,
    };
    match &s.fade {
        Some(f) => {
            let t = f.t.clamp(0, f.len);
            let l = t * 256 / f.len.max(1);
            match (f.kind, f.out) {
                (FadeKind::Black, true) => {
                    // (the palette as it was: all white after a finished fade to white - db5t_004 12-22 f2070, the
                    // white screen scaled toward black, a flat grey 206 at l = 44)
                    if s.white >= 16 {
                        out.palette_fade = Some(pf(256, WHITE));
                    }
                    // (an already darkened palette stays as dark: the screen's `dim` - a second fade_out after a
                    // finished one, rkb_012 19-3 f510: black, not the palette scaled from 0 again)
                    out.palette_scale = Some(pf(l.max(s.dim), 0));
                    out.fade_black = 0;
                }
                (FadeKind::Black, false) => {
                    out.palette_fade = Some(pf(256 - l, 0));
                    out.fade_black = 0;
                }
                (FadeKind::White, true) => out.palette_scale = Some(pf(l, WHITE)),
                (FadeKind::White, false) => {
                    out.palette_fade = Some(pf(256 - l, WHITE));
                    out.fade_black = 0;
                }
                (FadeKind::Flash, _) => {
                    let n = (f.len / 2).max(1);
                    out.palette_fade = Some(pf(
                        if t <= n {
                            t * 256 / n
                        } else {
                            (2 * n - t) * 256 / n
                        },
                        WHITE,
                    ));
                }
                (FadeKind::Mosaic, _) => {}
            }
        }
        None if s.white >= 16 => out.palette_scale = Some(pf(256, WHITE)),
        None => {}
    }
    // screen_flash (0x4C, command vtable 0x0804C800): counter c = len - t, level (256 / (len / 2)) * (c while
    // c <= len / 2, else len - c) toward (r, g, b) through `0x08007074` (update 0x08015A30)
    if let Some(fl) = &s.flash {
        let half = (fl.len / 2).max(1);
        let c = fl.len - fl.t;
        let x = if c <= half { c } else { fl.len - c };
        let (r, g, b) = fl.rgb;
        out.palette_fade = Some(pf(
            (256 / half) * x,
            (r as u32) << 16 | (g as u32) << 8 | b as u32,
        ));
    }
}

/// the frame's events as frontend events
pub fn sim_events(w: &World, out: &mut Vec<SimEvent>) {
    for e in &w.events {
        match e {
            Event::Sfx { id, vol } => out.push(SimEvent::Audio(AudioEvent::PlaySfx {
                id: *id as u8,
                volume: (*vol).clamp(0, 64) as u8,
                pan: 0,
            })),
            Event::SfxStop { id } => out.push(SimEvent::Audio(AudioEvent::StopSfx(*id as u8))),
            Event::Song { id } => out.push(SimEvent::Audio(AudioEvent::PlaySong(*id as u16))),
            Event::SfxStopAll => out.push(SimEvent::Audio(AudioEvent::StopAllSfx)),
            Event::SongStop => out.push(SimEvent::Audio(AudioEvent::StopSong)),
            Event::MasterVolumes { music, sfx } => {
                out.push(SimEvent::Audio(AudioEvent::SetMasterVolumes {
                    music: *music as u8,
                    sfx: *sfx as u8,
                }))
            }
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
    prev: Option<(
        Vec<SpriteDraw>,
        Vec<HudView>,
        Option<wf_view::BannerView>,
        Option<wf_view::DialogView>,
        Option<wf_view::DialogAnim>,
        Vec<wf_view::BossBarView>,
        Option<i32>,
        Vec<wf_view::TitleView>,
        Vec<wf_view::DamageNumberView>,
        Vec<(u8, i32, i32)>,
    )>,
    prev_mode7: Option<wf_view::Mode7View>,
    prev_shake: Option<(i32, i32)>,
}

impl Presenter {
    pub fn before_step(&mut self, w: &World) {
        let mut v = RenderView::default();
        render_view(w, &mut v);
        self.prev_mode7 = v.map.as_ref().and_then(|m| m.mode7.clone());
        self.prev_shake = v.map.as_ref().map(|m| m.shake);
        self.prev = Some((
            v.sprites,
            v.hud,
            v.banner,
            v.dialog,
            v.dialog_anim,
            v.boss_bars,
            v.timer,
            v.title,
            v.damage_numbers,
            v.markers,
        ));
    }
    /// the session started mid-area (harnesses): nothing of the previous build to show
    pub fn reset(&mut self) {
        *self = Presenter::default();
    }
    pub fn apply(&self, out: &mut RenderView) {
        if let Some((sprites, hud, banner, dialog, anim, boss_bars, timer, title, nums, marks)) = &self.prev {
            // (an OBJ: a frame late)
            out.title = title.clone();
            out.markers = marks.clone();
            // (damage numbers too: dr4_009 15-9 f316, made at f315)
            out.damage_numbers = nums.clone();
            // (and their shake offset: the frame before's - sc_janemba f5130, the active shake's (-4, 1) against
            // the OBJs' previous (-4, 0))
            // (the sprites carry their own shake from their build: `obj_shake_baked`)
            out.obj_shake = Some((0, 0));
            out.sprites = sprites.clone();
            // (the cooldown triangle shows the frame's own value: shipr_001 7-1 f240, the record's 0x7FFF that frame
            // and an empty step-0 triangle)
            // (and the HP / EP bars: their tiles are drawn in the HUD's update, from what it read at its place in the
            // pass - `World::hud_sample`; opponent audit oa_t44_left f115: down by the enemy's hit in the hit's frame,
            // the damage number a frame later)
            let cds: Vec<(i32, i32, i32, i32, i32, i32, i32, bool)> = out
                .hud
                .iter()
                .map(|h| (h.ss_cooldown, h.ss, h.ss3, h.hp, h.hp_max, h.ep, h.ep_max, h.skill_alt))
                .collect();
            out.hud = hud.clone();
            for (h, c) in out.hud.iter_mut().zip(cds) {
                h.ss_cooldown = c.0;
                // (the SS / SS3 gauge parts and the skill icon's grey: tiles drawn in the HUD's update from its read
                // - `World::more_values`; skill audit sa_f26_skill2 f5 / f106, sa_f40_skill20 the gauge steps)
                h.ss = c.1;
                h.ss3 = c.2;
                h.skill_alt = c.7;
                h.hp = c.3;
                h.hp_max = c.4;
                h.ep = c.5;
                h.ep_max = c.6;
            }
            out.boss_bars = boss_bars.clone();
            // (its OAM a frame late; the digits are drawn into its tiles in the draw itself: the current value)
            out.timer = timer.and(out.timer);
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
            let form = w.player_idx(0).map(|i| w.pl_ref(i).form).or_else(|| {
                w.story
                    .chars
                    .get(w.story.current.max(0) as usize)
                    .map(|c| c.form)
            })?;
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
    // the choice cursor object (`0x080178C8`): the text object makes it with the first typed character of the page
    // with the options (`0x0800AF8A`, any line - emulator: shipr_227 7-2 row 173, the draw count 1 with the typing
    // pointer at 1; earth_4_27_2 row 2,285, the second page's first character) and its draw counts +1 a frame
    let cursor_draws =
        (d.options > 0 && d.phase == P::Shown).then(|| (d.t - d.page_t + 1).max(0) as u32);
    if scale < 1 {
        return Some(wf_view::DialogAnim {
            scale: 0xFF,
            typed,
            cursor_draws,
        });
    }
    Some(wf_view::DialogAnim {
        scale: scale.min(8) as u8,
        typed,
        cursor_draws,
    })
}

impl Sim for Session {
    fn step(&mut self, inputs: &[u16], events: &mut Vec<SimEvent>) {
        if self.world.frame == 0 {
            let song = self
                .world
                .data
                .areas
                .get(&self.world.arena.map)
                .and_then(|a| a.1);
            events.push(SimEvent::Audio(match song {
                Some(s) => AudioEvent::PlaySong(s),
                None => AudioEvent::StopSong,
            }));
        }
        // (battles too: their arena is a field map drawn by the same hardware - the OBJs, the HUD, the boxes and the
        // numbers show the frame before's, the BGs the frame's own)
        self.presenter.before_step(&self.world);
        Session::step(self, inputs);
        sim_events(&self.world, events);
    }
    fn view(&self, out: &mut RenderView) {
        render_view(&self.world, out);
        self.presenter.apply(out);
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
        let Some(w) = crate::snapshot::from_bytes(&self.data, &s[n + 1..]) else {
            return false;
        };
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
