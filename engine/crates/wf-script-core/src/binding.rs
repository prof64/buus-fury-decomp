//! Which system implements each of the 248 natives (single source of truth; `examples/coverage.rs` prints it
//! as the table in `engine/docs/script-host.md`).

use wf_script::natives;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    /// Handled inside the script VM / runtime (G, P, run_script, run_event).
    Vm,
    /// Implemented here through an existing wf-core API (named in the note).
    Core,
    /// Implemented here on [`crate::StoryState`] — game state wf-core does not have yet (see the requests).
    Story,
    /// Not implementable yet: the note names the missing core system and a proposed API. The adapter returns
    /// an error naming it.
    Missing,
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
        0x00..=0x07 => b(Vm, "VM globals G0..G3 / param block P0..P3"),
        0x08..=0x0b => b(Story, "StoryState.l[i] (game words 0x03001020)"),
        0x0c | 0x0d => b(Core, "leader Player.rec.hp / .max_hp"),
        0x0e => b(Core, "leader Player: form.ss3 -> ss3_gauge, form.ss -> ss_gauge, else rec.ep"),
        0x0f => b(Core, "leader Player.rec.max_ep"),
        0x10 => b(Story, "StoryState.zenie"),
        0x11 => b(Story, "StoryState.variant_byte"),
        0x12..=0x14 => b(Story, "StoryState.area (world, map, variant); core has only World.arena.map"),
        0x15 => b(Core, "leader Player.form -> GameData.form(id).char_"),
        0x16 | 0x17 => b(Missing, "string handles: script values that name UI strings (StringRef(i32) table in core/ui)"),
        0x18 => b(Story, "StoryState.menu_result (set by choice nodes / menus)"),
        0x19 => b(Core, "World.rng.rand(n)"),
        0x1a => b(Core, "World.rng.chance(pct * 0x28F5D >> 8)"),
        0x1b..=0x1e => b(Story, "StoryState.flags"),
        0x1f | 0x20 => b(Core, "World::sfx(id, vol) -> Event::Sfx"),
        0x21 => b(Core, "World::sfx_stop(id) -> Event::SfxStop"),
        0x22 => b(Missing, "Event::SfxStopAll"),
        0x23 => b(Core, "Event::Song { id }"),
        0x24 => b(Missing, "Event::SongStop"),
        0x25..=0x2a => b(Missing, "presentation state: World.screen { bg_visible[4], bg_priority, blend } (or Event::Screen*)"),
        0x2b..=0x2e => b(Missing, "Collision overrides: Collision::set_rect(layer, rect, passable) on a per-World copy-on-write grid"),
        0x2f | 0x30 => b(Missing, "World.hud_visible: bool"),
        0x31 | 0x32 => b(Missing, "camera script mode: Camera { target: CamTarget::{Follow(id), MoveTo{x,y,speed}} } + done()"),
        0x33 | 0x34 => b(Missing, "Camera.follow: Option<EntityId> (core cameras always follow the player)"),
        0x35 => b(Missing, "scripted actors: Body::Npc(Npc { a: Actor, track: Track }) + World::spawn_npc(sprite, x, y)"),
        0x36 | 0x8a | 0xa9 | 0xaa | 0xad => b(Missing, "map objects of the current area (wf-world objects.json) instantiated in World: World::spawn_map_object(index, force)"),
        0x37 => b(Core, "count live entities whose Actor.sprite == id (players by character)"),
        0x38 => b(Story, "StoryState.director.batch = true"),
        0x39 => b(Story, "StoryState.director.group.push(actor)"),
        0x3a => b(Missing, "commit: block until every Track (or the group) is idle — needs per-actor Tracks"),
        0x3b..=0x61 => b(Missing, "per-actor command Track { queue: Vec<Cmd> } on Npc/Player/Enemy + Cmd::{Walk, Face, Anim, Shoot, Wait, ...}::update(&mut World) -> done"),
        0x62..=0x69 => b(Missing, "map props (scripted OBJ sprites): World.props + Track"),
        0x6a..=0x70 => b(Missing, "camera Track (queued camera commands)"),
        0x71..=0x77 => b(Missing, "area loading in World: World::warp(world, map, variant, x, y) / scene stack (save field, load scene, return)"),
        0x78..=0x7e => b(Missing, "screen effect timer: World.screen_fx { kind: Fade|White|Mosaic|Flash, t, len } (modal, done when t == len)"),
        0x7f | 0x81..=0x83 => b(Story, "StoryState.supplies (u8 counts)"),
        0x80 | 0x93 => b(Story, "StoryState inventory + present(ItemGet) (60-frame popup action)"),
        0x84..=0x87 => b(Story, "StoryState.drops (pickup spawn requests at the loot position) — needs Body::Pickup in core to appear"),
        0x88 => b(Story, "modal wait: Pending::Frames counted by ScriptLayer::poll"),
        0x89 => b(Core, "modal: Pending::Button, done on World.input[0].pressed & A"),
        0x8c | 0x8d | 0xce | 0xcf | 0xd9 | 0xda => b(Missing, "UI menus (wf-ui): save, character switch, shop buy/sell, destination labels — Pending::Menu resolved by the frontend input"),
        0x8e | 0xd1 | 0xd4 => b(Missing, "session control: return to title / credits / sleep (Session-level, not World)"),
        0x8f..=0x91 => b(Missing, "World.boss_bar: Option<(EntityId, a, b)>"),
        0x92 | 0x94 => b(Story, "StoryState.equipment (owned list)"),
        0x95 => b(Core, "leader Player.apply_form(GameData.chars[c].form) (party roster itself: StoryState)"),
        0x96..=0x9a => b(Story, "StoryState.party / char_flags"),
        0x9b => b(Core, "GameData.form(leader.form).ss || .ss3"),
        0x9c => b(Core, "GameData.form(chars[c].form).gfx - 9"),
        0x9d => b(Story, "StoryState.char_form[c] (per-character current form; party records not in core)"),
        0x9e..=0xa0 | 0xab | 0xac | 0xec | 0xf4 => b(Missing, "map markers / radar / energy sphere objects (presentation entities)"),
        0xa1..=0xa3 => b(Core, "resolved entity Actor.x>>8 / y>>8 / facing"),
        0xa4..=0xa6 | 0xe9 | 0xea => b(Missing, "per-character skill lists + levels for all 9 characters (core keeps only active fighters' skills)"),
        0xa7 | 0xc6 | 0xc7 => b(Missing, "mini-games (Watch This!, power struggle, punch): modal sub-sessions"),
        0xa8 => b(Missing, "fusion of party records: progression::fuse exists; needs party records + timed fusion in World"),
        0xb2 | 0xef => b(Missing, "title text overlay: World.title_text"),
        0xb3 | 0xb4 => b(Missing, "the game's sine table (wf-core::fixed::sin_q(angle))"),
        0xb5..=0xb9 | 0xd6..=0xd8 => b(Vm, "run_event / run_script"),
        0xba | 0xbb => b(Story, "StoryState.flags (bit range)"),
        0xae..=0xc1 => b(Story, "StoryState.timers (countdown / stopwatch) ticked by poll(); timer HUD visibility is presentation"),
        0xc2 => b(Core, "player::gain_xp(w, leader, n) (needs World.opts.xp)"),
        0xc3 | 0xc8 => b(Core, "leader Player.rec.xp / .level"),
        0xc4 | 0xc5 | 0xeb | 0xdb => b(Story, "StoryState.game_bits"),
        0xc9 | 0xdc | 0xf0 => b(Missing, "engine message broadcast to entities (World::broadcast_msg(kind, value))"),
        0xca => b(Core, "every Player in World.players: rec.hp = max_hp, rec.ep = max_ep (+ StoryState party records)"),
        0xcb | 0xcc => b(Missing, "screen-space queries need the presentation camera contract (World::camera(0) + actor box)"),
        0xcd => b(Story, "StoryState.shop (list for the shop UI)"),
        0xd0 => b(Core, "leader Player.apply_form(form)"),
        0xd2 => b(Core, "World.input[0].pressed & A (leader's input)"),
        0xd3 | 0xde => b(Missing, "string handles (see 0x16)"),
        0xd5 => b(Core, "leader Player.equip contains item"),
        0xdd => b(Core, "sum of rec.xp of active players (party records: StoryState)"),
        0xdf..=0xe8 | 0xf5 => b(Story, "StoryState.eq_accum (equipment effect scripts; core folds these from content in recalc_stats)"),
        0xed | 0xee => b(Core, "Player.rec.points of the fighter playing that character"),
        0xf1 => b(Core, "Player.equip[slot] = 0 + refresh_stats; item back to StoryState.equipment"),
        0xf2 => b(Core, "seconds > 0: Player.holy_left = seconds*60 (+ boost values: Missing in EffStats); 0: rec stats += values"),
        0xf3 => b(Core, "progression::level_up(rec, growth, old, new, World.rng)"),
        0xf6 => b(Core, "always 0 (no tamper check in the remake)"),
        0xf7 => b(Missing, "timed special buff (record byte +0x67)"),
        _ => b(Missing, "out of table"),
    }
}

pub fn name(n: u8) -> String {
    natives::name(n)
}
