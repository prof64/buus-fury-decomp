//! Which system implements each of the 248 natives (single source of truth; `examples/coverage.rs` prints it
//! as the table in `engine/docs/script-host.md`). Every native is implemented (`natives.rs`); `Partial` marks
//! the ones whose behaviour is modelled but not verified frame-exact against the game (the note says what).

use wf_script::natives;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    /// Handled inside the script VM / runtime (G, P, run_script, run_event).
    Vm,
    /// Existing wf-core APIs (player, records, RNG, sound events, input).
    Core,
    /// `World.story` (flags, words, inventory, zenie, the 9 character records).
    Story,
    /// `World.scene` (command tracks, director, camera control, screen, dialog, menus, HUD, boss bar).
    Scene,
    /// `World.field` (map objects, warps, scene loads, pickups) — battle mode records a `scene::Request`.
    Field,
    /// Implemented, but a model (not frame-checked against the game; see the note).
    Partial,
}

pub struct Binding {
    pub at: Where,
    pub note: &'static str,
}

const fn b(at: Where, note: &'static str) -> Binding {
    Binding { at, note }
}

pub fn binding(n: u8) -> Binding {
    use Where::*;
    match n {
        0x00..=0x07 => b(Vm, "VM globals G0..G3 (mirrored to World.story.g) / param block P0..P3"),
        0x08..=0x0b => b(Story, "World.story.l[i] (0x03001020)"),
        0x0c | 0x0d => b(Core, "leader Player.rec.hp / .max_hp"),
        0x0e => b(Core, "leader Player: SS3 gauge / SS gauge / EP by form"),
        0x0f => b(Core, "leader Player.rec.max_ep"),
        0x10 => b(Story, "World.story.zenie"),
        0x11 => b(Story, "World.story.variant_byte"),
        0x12..=0x14 => b(Field, "World.field.area (world, area, variant)"),
        0x15 => b(Core, "leader's character (form -> char)"),
        0x16 | 0x17 | 0xd3 | 0xde => b(Core, "string handle (text.rs) resolved by formatted text nodes"),
        0x18 => b(Scene, "HostState.menu_result, set when a choice window closes"),
        0x19 | 0x1a => b(Core, "World.rng (game PRNG 0x0803FAAC)"),
        0x1b..=0x1e | 0xba | 0xbb => b(Story, "World.story flags"),
        0x1f..=0x21 => b(Core, "Event::Sfx / SfxStop"),
        0x22 => b(Core, "Event::SfxStopAll"),
        0x23 => b(Core, "Event::Song"),
        0x24 => b(Core, "Event::SongStop"),
        0x25..=0x2a => b(Scene, "scene.screen.layers (BG visibility, priority, blend targets, offset, alpha)"),
        0x2b..=0x2e => b(Scene, "scene::set_area: collision overrides (copy-on-write grids, re-applied on restore)"),
        0x2f | 0x30 => b(Scene, "scene.hud_hidden"),
        0x31 | 0x32 => b(Scene, "scene::start_scroll: modal camera scroll"),
        0x33 | 0x34 => b(Scene, "scene.cam.mode Follow / Fixed"),
        0x35 => b(Scene, "scene::spawn_npc: Body::Npc"),
        0x36 | 0x8a | 0xa9 | 0xaa => b(Field, "field::spawn_map_object (area lists 1 / 0 / 3)"),
        0x37 => b(Scene, "objects with that script sprite id"),
        0x38 | 0x39 => b(Scene, "director batch / wait_for_actor group"),
        0x3a => b(Scene, "director commit: block until the batch (group) is idle"),
        0x46 | 0x5c | 0x5e => b(Scene, "Cmd::Beam: charge effect + the game's beam object (shots::KameBeam: grows on screen, retracts, its sounds); lockstep-exact"),
        0x3b => b(Partial, "Cmd::Beam (wave): timing and actor animation checked; the wavy beam 0x080286D4 is a scene Visual"),
        0x3e | 0x41 | 0x42 => b(Partial, "Cmd::Shoot: timing and animation checked; the projectile is a scene Visual"),
        0x3c | 0x3d | 0x51 | 0x59 => b(Scene, "Cmd::PlayAnim / SetAnim / SetPose / PlayAnimFrames on the actor state animation"),
        0x3f | 0x40 | 0x48 | 0x65 => b(Scene, "Cmd::Puff: fx::puff"),
        0x43 => b(Scene, "Cmd::HitFlash"),
        0x44 | 0x50 | 0x5f | 0x60 | 0x61 => b(Scene, "Cmd::MoveTo (step math 0x080192CE)"),
        0x45 => b(Scene, "Cmd::JumpTo"),
        0x47 | 0x53 => b(Scene, "Cmd::FaceActor (holds) / Face"),
        0x49 => b(Scene, "Cmd::GlideTo"),
        0x4a | 0x68 => b(Scene, "Cmd::Wait"),
        0x4b => b(Scene, "Cmd::Sfx"),
        0x4c => b(Scene, "Cmd::ScreenFlash: scene.screen.flash"),
        0x4d => b(Scene, "Cmd::Remove"),
        0x4e => b(Scene, "Cmd::ChangeSprite: new Npc, the queue moves to it"),
        0x4f => b(Field, "Cmd::BecomeMapObject: field::spawn_map_object(actor list) at the stand-in's place"),
        0x52 => b(Scene, "Cmd::Tint: the nearest of the 20 presets of table 0x08752EEC (Npc.tint = entry; no palette = restore)"),
        0x54..=0x56 => b(Scene, "Cmd::SetSpeed / SetAttachment / SetByte8 (Npc fields)"),
        0x57 => b(Scene, "Cmd::Shake: World::shake + track wait"),
        0x58 | 0x80 | 0x93 => b(Scene, "Cmd::ItemGet (60 frames, SFX 45, hold-up pose) + World.story inventory"),
        0x5a | 0x5b => b(Scene, "Cmd::Emote (balloon Visual)"),
        0x5d => b(Scene, "Cmd::SetPos"),
        0x63 | 0x64 | 0x66 | 0x67 | 0x69 => b(Scene, "prop track commands on the area's map props (scene.props -> Field.props: visibility, rect position; jump arc in 1/256 px)"),
        0x62 => b(Partial, "prop track command on the area's map prop: timing and the vanish exact; the dust frames (0x087581FC) are not drawn"),
        0x6a..=0x70 => b(Scene, "camera track commands (follow / pan / release / set)"),
        0x71 | 0x75 => b(Field, "saved field + return (area reload, camera)"),
        0x72..=0x74 => b(Field, "scene loads: fades + field::load_area + fixed camera"),
        0x76 | 0x77 => b(Field, "field::warp (transition with fades for 0x76)"),
        0x78..=0x7e => b(Scene, "scene::start_fade (modal)"),
        0x7f | 0x81..=0x83 => b(Story, "World.story.supplies (u8 counters)"),
        0x84..=0x87 => b(Field, "field::spawn_pickup at the loot position (battle mode: HostState.drops)"),
        0x88 | 0xef => b(Scene, "modal wait (Pending::Frames)"),
        0x89 => b(Scene, "modal: leader's A press"),
        0x8b => b(Field, "field::refresh_triggers (area triggers by condition + exits)"),
        0xad => b(Field, "field::respawn_breakables (missing breakables whose condition holds)"),
        0x8c | 0x8d => b(Partial, "scene::Menu Save / Character (deterministic model, not frame-checked)"),
        0x8e | 0xd4 => b(Scene, "scene::Request ReturnToTitle / Sleep (session level)"),
        0x8f..=0x91 => b(Scene, "scene.boss_bar"),
        0x92 | 0x94 => b(Story, "World.story.equipment"),
        0x95 => b(Story, "switch_character: story record -> leader Player"),
        0x96..=0x9a => b(Story, "World.story.chars[c].flags"),
        0x9b => b(Core, "leader form SS / SS3"),
        0x9c | 0x9d => b(Story, "character's current form gfx / set by sprite"),
        0x9e..=0xa0 | 0xec | 0xf4 => b(Scene, "scene.visuals (markers, box, map marker, energy sphere)"),
        0xa1..=0xa3 => b(Core, "resolved actor position / facing"),
        0xa4..=0xa6 | 0xe9 | 0xea => b(Story, "World.story.chars[c].skills / skill_levels (+ the live fighter)"),
        0xa7 | 0xc6 | 0xc7 => b(Partial, "scene::Menu mini-games (Watch This!, power struggle, punch): deterministic models, not the game's"),
        0xa8 => b(Story, "fusion record (60% of the summed stats), member flags"),
        0xab | 0xac => b(Scene, "scene.radar"),
        0xae..=0xb1 | 0xbc..=0xc1 => b(Scene, "scene.timers + scene.timer_hud"),
        0xb2 => b(Scene, "scene.title"),
        0xb3 | 0xb4 => b(Core, "scene::SIN_TABLE (0x087B6EEC)"),
        0xb5..=0xb9 | 0xd6..=0xd8 => b(Vm, "run_event / run_script"),
        0xc2 => b(Core, "player::gain_xp_levels; a level-up runs hook #3 inline (NativeResult::RunEvent); kill XP in the field queues #3 (Cause::LevelUp)"),
        0xc3 | 0xc8 => b(Core, "leader Player.rec.xp / level"),
        0xc4 | 0xc5 | 0xeb | 0xdb => b(Story, "World.story.game_bits"),
        0xc9 | 0xdc => b(Partial, "engine broadcast with an unknown receiver: scene::Request::Broadcast"),
        0xca => b(Story, "every story record and live fighter: HP / EP full"),
        0xcb | 0xcc => b(Scene, "scene::screen_rect_at (camera-clamped)"),
        0xcd => b(Scene, "scene.shop"),
        0xce | 0xcf => b(Scene, "scene::Menu ShopBuy / ShopSell + purchases on World.story"),
        0xd0 => b(Core, "leader Player.apply_form"),
        0xd1 => b(Partial, "scene::Menu Credits (fixed length) + Request::Credits"),
        0xd2 => b(Core, "scene.keys[0] (the leader's raw input)"),
        0xd5 => b(Core, "leader Player.equip"),
        0xd9 | 0xda => b(Scene, "scene.dest_label"),
        0xdd => b(Story, "summed XP of characters 0..4"),
        0xdf..=0xe8 | 0xf5 => b(Story, "HostState.eq_accum (equipment effect accumulator)"),
        0xed | 0xee => b(Story, "stat points of the character (fighter or record)"),
        0xf0 => b(Field, "actor+0x1A8 = height << 10, positive up (fieldpatrol::set_z for map enemies)"),
        0xf1 => b(Story, "unequip to the default item, back to the owned list"),
        0xf2 => b(Core, "seconds 0: base stats += (bytes); else the timed buff (Player.holy_left / boost, progression::recalc_stats_with)"),
        0xf7 => b(Core, "the timed buff with no stat bonus; its special flag (record +0x67, effect unknown) is kept in HostState.boost"),
        0xf3 => b(Story, "progression::level_up on the fighter / record"),
        0xf6 => b(Core, "always 0 (no tamper check)"),
        _ => b(Partial, "out of table"),
    }
}

pub fn name(n: u8) -> String {
    natives::name(n)
}
