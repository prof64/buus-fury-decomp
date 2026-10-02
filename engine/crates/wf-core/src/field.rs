//! The field game: one area of the overworld at a time, loaded from content, with its placed objects (enemies,
//! NPCs, triggers, exits, save points, breakables, item pickups, map sprites), area transitions and the field camera.
//! Buu's Fury fights in the field: enemies are the battle system's `Enemy` entities, the players the battle
//! `Player`s; there is no separate battle mode. Battle mode (`World.field == None`) is untouched by this module.
//!
//! Frame (`Session::step_frames` when `World.field` is set, the game's `0x08006E3C`): enemy grid marks → object
//! update (`World::step_entities`, every mode: the world keeps running during fades) → `post_step`: the trigger check
//! (`0x080126D4`, normal mode only), the fade timeline (exit → 32 fade-out frames → load → 32 fade-in frames), the
//! camera (`0x080077EC` with the field clamp), placed enemies' defeats, story sync, play time. Mechanics:
//! re/buu/notes.md §11 ("Field: area loading and transitions") and the following field sections.
//!
//! Scripts stay outside wf-core: everything that runs a script leaves the step as `Event::Field(FieldEvent::Run
//! {..})`; the script host answers through the functions at the end of this file (warp, spawn_map_object, spawn
//! pickups, busy). Spawn conditions are evaluated in the core (`story::eval_condition`).

use crate::battle::{self, spawn_enemy};
use crate::fieldcontent::{Activation, AreaDef, Callback, Cond, Dest, MapDef, ObjDef, ObjKind, ObjRef, TriggerKind};
use crate::geom::{Dir, Rect};
use crate::player::{self, Player, PlayerInit};
use crate::story::{self, Story};
use crate::world::{ArenaRt, Body, Collision, EntityId, Event, World, GBA_H, GBA_W, MAX_PLAYERS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use crate::fieldnpc::Behaviour;
pub use crate::fieldobj::{Breakable, MapSprite, Pickup, PickupKind};

/// the area currently loaded: (world, area, variant 1-based)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AreaId {
    pub world: i32,
    pub area: i32,
    pub variant: i32,
}

/// what made a script run (the host maps `Callback` ids onto its compiled scripts)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// a script / event / save-point trigger fired (object of the area or the map header)
    Trigger { obj: ObjRef },
    /// A facing a talking NPC
    Talk { npc: EntityId },
    /// a map item was collected (its "You found ..." event)
    Pickup { obj: ObjRef },
    /// a breakable prop broke (loot script; loot position = x, y)
    Break { obj: ObjRef, x: i32, y: i32 },
    /// a placed enemy was defeated (on-defeat callback; loot position = x, y)
    Defeat { entity: EntityId, ty: i32, x: i32, y: i32 },
    /// the area's on-enter / the previous area's on-leave bytecode (run during the load, before the player exists)
    AreaEnter,
    AreaLeave,
    /// an enemy AI node queued a script (RunScript: `run` = `header_0xADDR`, the event whose header is ADDR;
    /// InlineScript: `inline_0xADDR`)
    Ai { entity: EntityId },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum FieldEvent {
    /// an area finished loading (objects created)
    AreaEnter { area: AreaId, key: String },
    AreaLeave { area: AreaId },
    /// run a script / event (the host resolves `run` ids: `inline_0xADDR` / `sNNN`); `slot` = the player concerned
    Run { run: Callback, cause: Cause, slot: u8 },
    /// a pickup was collected (already applied to `World.story` unless it is a map item, whose event gives it)
    Collected { kind: PickupKind, slot: u8 },
    /// a placed enemy was defeated (also when it has no callback)
    Defeated { entity: EntityId, ty: i32, x: i32, y: i32 },
    /// a spawn condition the core could not evaluate (treated as false)
    ConditionUnknown { script: String },
    /// an exit / warp started its fade-out toward `to`
    Transition { to: AreaId },
}

// ---------------------------------------------------------------------------------------------------------------
// state

/// one entry of the trigger list (`0x03002A64`); not an object of the object world
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TriggerRt {
    pub obj: ObjRef,
    pub rect: Rect,
    pub act: Activation,
    /// the check pass this trigger was last tested in (the "on entry" test `0x08007791`)
    pub last: u32,
}

/// a map prop's live state (header +0x24 list; breakables and scripts change it)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PropRt {
    pub rect: Rect,
    pub visible: bool,
    /// shake offset this frame
    pub dx: i32,
}

/// the run mode of the field loop
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    /// fade-out (`0x08006D98`, brightness 256 → 0 by 8): k = fade frames done (1..32); the load runs at the end of
    /// the 32nd
    FadeOut { k: i32, to: AreaId, x: i32, y: i32, facing: Option<Dir> },
    /// fade-in (`0x08006DD6`): k = fade frames done; the player goes idle at the end of the 32nd
    FadeIn { k: i32 },
}

/// fade length in logic frames (brightness 256 by 8)
pub const FADE: i32 = 32;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Field {
    pub area: AreaId,
    pub key: String,
    /// map id (`area_08_003`)
    pub map: String,
    pub camera_max: (i32, i32),
    pub props: Vec<PropRt>,
    /// the trigger list in scan order (area triggers in exported order, then the map header's)
    pub triggers: Vec<TriggerRt>,
    pub mode: Mode,
    /// a script / event holds the field (set by the host): no trigger check, no talk
    pub busy: bool,
    /// a script host is attached: blocking events (event triggers, save points, talk) set `busy` until it clears it
    pub host: bool,
    /// trigger check passes (list +0xC4)
    pub checks: u32,
    /// the frame the field became busy (an event started: NPC animations do not tick that frame)
    pub busy_since: u32,
    /// the event ended this frame (NPC animations tick once more)
    pub event_ended: bool,
    /// the area allows combat (area record +8): 0 = A only talks / interacts
    pub combat: bool,
    /// calls queued for a later frame (breakables' break callbacks)
    pub deferred: Vec<crate::fieldobj::Deferred>,
    /// the loot position and velocity (`0x03001010` / `0x03001018`, 24.8 velocity)
    pub loot: (i32, i32, i32, i32),
    /// breakable debris chips (display)
    pub debris: Vec<crate::fieldobj::Debris>,
    /// the bottom label of the world map (message trigger 0x17 string id)
    pub label: Option<i64>,
    /// the world map's flying player (3D map only)
    pub flight: Option<crate::fieldworld::Flight>,
    /// the blocking item-get presentation
    pub item_get: Option<crate::fieldobj::ItemGet>,
    /// field enemies' patrol runners (def +0x18 steps; run by the AI node RunMotionController)
    pub patrols: Vec<crate::fieldpatrol::Patrol>,
    /// placed enemies alive: (entity, object, last position px)
    pub placed: Vec<(EntityId, ObjRef, i32, i32)>,
    /// frame of the last area load
    pub entered: u32,
    /// number of area loads (touch_once triggers survive only within one)
    pub loads: u32,
}

/// how a field session starts
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldConfig {
    pub world: i32,
    pub area: i32,
    /// 1-based; 0 = the area's variant selector
    #[serde(default)]
    pub variant: i32,
    /// player position (footprint top-left px); None = the map's start position
    #[serde(default)]
    pub pos: Option<(i32, i32)>,
    #[serde(default)]
    pub facing: Dir,
    /// character per player slot (0 Goku .. 8); empty = the story's current character
    #[serde(default)]
    pub chars: Vec<i32>,
    /// the persistent state to start from (None = a new game)
    #[serde(default)]
    pub story: Option<Story>,
    pub rng: battle::RngInit,
    #[serde(default = "yes")]
    pub xp: bool,
    /// start like an exit arrival: the load's RNG draws, then the 32-frame fade-in with the player autowalking
    #[serde(default)]
    pub arrive: bool,
}
fn yes() -> bool {
    true
}
impl Default for FieldConfig {
    fn default() -> Self {
        FieldConfig { world: 8, area: 3, variant: 0, pos: None, facing: 0, chars: vec![], story: None, rng: battle::RngInit::Seed(1), xp: true, arrive: false }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// setup

fn field_data(w: &World) -> Arc<crate::fieldcontent::FieldData> {
    Arc::clone(w.data.field.as_ref().expect("content pack without world/ (field data)"))
}

/// a player of a character from the story records
pub fn make_player(w: &World, slot: u8, ch: i32) -> Player {
    let cs = &w.story.chars[(ch.max(0) as usize).min(w.story.chars.len() - 1)];
    let c = w.data.chars.iter().find(|c| c.index == ch);
    let forms = c.map(|c| c.forms.clone()).unwrap_or_default();
    let form = if w.data.form(cs.form).is_some() { cs.form } else { c.map(|c| c.form).unwrap_or(2) };
    let skills = if cs.skills.is_empty() { vec![7] } else { cs.skills.clone() };
    let sel = (cs.sel.max(0) as usize).min(skills.len() - 1);
    let lv = cs.skill_level(skills[sel]);
    let mut p = Player::new(w, PlayerInit { slot, form, rec: cs.rec.clone(), equip: cs.equip, skills, skill_level: lv, sel, holy_water: false, forms_allowed: forms });
    p.ss_cooldown = cs.ss_cooldown;
    p
}

/// the player object the load creates (`0x080109AA`): a fresh object from the character's record; what the game
/// keeps in the record (HP, EP, gauges, cooldown, skills, equipment) carries over
fn renew_player(w: &World, old: &Player) -> Player {
    let mut p = Player::new(
        w,
        PlayerInit { slot: old.slot, form: old.form, rec: old.rec.clone(), equip: old.equip, skills: old.skills.clone(), skill_level: old.skill_level, sel: old.sel, holy_water: false, forms_allowed: old.forms_allowed.clone() },
    );
    p.ss_gauge = old.ss_gauge;
    p.ss_acc = old.ss_acc;
    p.ss3_gauge = old.ss3_gauge;
    p.ss_cooldown = old.ss_cooldown;
    p.holy_left = old.holy_left;
    p.regen_timer = old.regen_timer;
    p.hp_regen_acc = old.hp_regen_acc;
    p.hp_acc = old.hp_acc.clone();
    p.ep_acc = old.ep_acc;
    p.story_cnt = old.story_cnt;
    p.fus_cnt = old.fus_cnt;
    p.partner = old.partner.clone();
    p.refresh_stats(w);
    p
}

fn new_field() -> Field {
    Field {
        area: AreaId::default(),
        key: String::new(),
        map: String::new(),
        camera_max: (0, 0),
        props: vec![],
        triggers: vec![],
        mode: Mode::Normal,
        busy: false,
        busy_since: 0,
        checks: 0,
        event_ended: false,
        combat: true,
        host: false,
        deferred: vec![],
        patrols: vec![],
        loot: (0, 0, 0, 0),
        item_get: None,
        debris: vec![],
        label: None,
        flight: None,
        placed: vec![],
        entered: 0,
        loads: 0,
    }
}

/// build a field world (a new game or a saved story) and load the start area
pub fn create(data: Arc<crate::content::GameData>, cfg: &FieldConfig) -> World {
    let rng = match cfg.rng {
        battle::RngInit::Seed(s) => crate::rng::Rng::from_seed(s),
        battle::RngInit::State(a, b) => crate::rng::Rng::new(a, b),
    };
    let arena = ArenaRt { id: String::new(), map: String::new(), bounds: Rect::new(0, 0, 2048, 2048), region: None, view: None };
    let mut w = World::new(Arc::clone(&data), rng, arena);
    w.opts.xp = cfg.xp;
    w.story = cfg.story.clone().unwrap_or_else(|| Story::new_game(&data));
    let chars = if cfg.chars.is_empty() { vec![w.story.current] } else { cfg.chars.clone() };
    for (slot, &ch) in chars.iter().enumerate().take(MAX_PLAYERS) {
        let mut p = make_player(&w, slot as u8, ch);
        p.a.facing = cfg.facing;
        let id = w.add(Body::Player(Box::new(p)));
        w.players.push(id);
    }
    if let Some(&c) = chars.first() {
        w.story.current = c;
    }
    w.field = Some(Box::new(new_field()));
    let fd = field_data(&w);
    let pos = cfg.pos.or_else(|| fd.area(cfg.world, cfg.area).and_then(|a| fd.map_of(a, cfg.variant.max(1))).map(|m| m.start_pos)).unwrap_or((120, 120));
    let to = AreaId { world: cfg.world, area: cfg.area, variant: cfg.variant };
    load_area(&mut w, to, pos.0, pos.1, Some(cfg.facing));
    if cfg.arrive {
        // the load frame's camera update (the game eases once before the first fade-in frame)
        for s in 0..w.players.len() {
            camera(&mut w, s);
        }
        begin_fade_in(&mut w);
    } else {
        for s in 0..w.players.len() {
            if let Some(i) = w.player_idx(s) {
                player::set_idle(&mut w, i);
            }
        }
    }
    w
}

/// the variant an entry uses (`0x08008B30`): the given one if != 0, else 1 without a selector, else the selector
/// bytecode's result (no area of the ROM has a selector: 1)
pub fn resolve_variant(a: &AreaDef, v: i32) -> i32 {
    if v > 0 {
        return v.min(a.maps.len().max(1) as i32);
    }
    1
}

fn cond_holds(w: &mut World, c: &Cond) -> bool {
    match c {
        Cond::Always => true,
        Cond::Never(_) => false,
        Cond::FlagSet(f) => w.story.flag(*f),
        Cond::FlagClear(f) => !w.story.flag(*f),
        Cond::FlagSetConsume(f) => {
            if w.story.flag(*f) {
                w.story.set_flag(*f, false);
                true
            } else {
                false
            }
        }
        Cond::Script(id) => {
            let fd = field_data(w);
            let cur = leader_char(w);
            match fd.cond_code.get(id).and_then(|code| story::eval_condition(code, &w.story, cur)) {
                Some(b) => b,
                None => {
                    w.events.push(Event::Field(FieldEvent::ConditionUnknown { script: id.clone() }));
                    false
                }
            }
        }
    }
}

/// the leader's character index
pub fn leader_char(w: &World) -> i32 {
    w.player_idx(0).and_then(|i| story::char_of_form(&w.data, w.pl_ref(i).form)).unwrap_or(w.story.current)
}

/// create one placed object of the object world (not triggers: see `TriggerRt`); returns its entity
pub fn spawn_object(w: &mut World, o: &ObjDef) -> Option<EntityId> {
    let nplayers = w.players.len().max(1);
    match &o.kind {
        ObjKind::Trigger { rect, act, .. } => {
            if let Some(f) = w.field.as_mut() {
                f.triggers.push(TriggerRt { obj: o.at, rect: *rect, act: *act, last: u32::MAX });
            }
            None
        }
        ObjKind::Enemy { ty, x, y, .. } => {
            // the constructor first (spawn_enemy: its SetAnim(0) idle enter draws rand(255); the patrol runner from def
            // +0x18), the AI's first state after it
            let i = spawn_enemy(w, *ty, *x, *y, 0, Some(-1))?;
            // the world map (3584 x 2560, beyond the 2048-px grids): placed exactly at the def position
            if w.field.as_ref().is_some_and(|f| f.camera_max.0 > 2048 || f.camera_max.1 > 2048) || w.data.field.as_ref().and_then(|d| w.field.as_ref().and_then(|f| d.maps.get(&f.map))).is_some_and(|m| m.three_d) {
                let e = w.en(i);
                e.a.x = *x << 8;
                e.a.y = *y << 8;
            }
            let id = w.ents[i].id;
            if let ObjKind::Enemy { behaviour, .. } = &o.kind {
                crate::fieldpatrol::attach(w, id, behaviour);
            }
            if let Some(st) = w.data.enemy(*ty).map(|t| t.ai) {
                let f = w.frame as i64;
                let e = w.en(i);
                e.mark1 = f;
                e.mark2 = f;
                crate::nodes::push(w, i, st);
            }
            let k = w.field.as_ref().map(|f| f.placed.len()).unwrap_or(0);
            w.en(i).target = (k % nplayers) as u8;
            let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
            if let Some(f) = w.field.as_mut() {
                f.placed.push((id, o.at, px, py));
            }
            Some(id)
        }
        ObjKind::Npc { .. } => crate::fieldnpc::spawn(w, o),
        ObjKind::Breakable { .. } => crate::fieldobj::spawn_breakable(w, o),
        ObjKind::Item { .. } => crate::fieldobj::spawn_map_item(w, o),
        ObjKind::MapSprite { .. } => crate::fieldobj::spawn_map_sprite(w, o),
        ObjKind::Unknown => None,
    }
}

fn list_of(o: &ObjDef) -> i32 {
    match o.at {
        ObjRef::List(l, _) => l as i32,
        ObjRef::Header(_) => -1,
    }
}

/// unload the current area and load `to` with the leader at (x, y) px (footprint top-left): EnterArea `0x08008D44` +
/// LoadArea `0x08008B30`
pub fn load_area(w: &mut World, to: AreaId, x: i32, y: i32, facing: Option<Dir>) {
    // players offstage during a cutscene scene come back for any real area load (warp, exit, return_to_field)
    crate::scene::bring_back(w);
    load_area_with(w, to, x, y, facing, true);
}

/// the area load with `objects` = false: the map's conditional objects are not spawned (script load_scene
/// 0x74 `load_scene_fade_noobj`, 0x08009030 first flag 0). A cutscene scene load calls this after
/// `scene::send_offstage` (no player object: no player re-creation, no draws).
pub fn load_area_with(w: &mut World, to: AreaId, x: i32, y: i32, facing: Option<Dir>, objects: bool) {
    let fd = field_data(w);
    let Some(a) = fd.area(to.world, to.area) else { return };
    let variant = resolve_variant(a, to.variant);
    let Some(m) = fd.map_of(a, variant) else { return };
    let to = AreaId { variant, ..to };
    let from = w.field.as_ref().map(|f| f.area).unwrap_or_default();
    let had_area = w.field.as_ref().is_some_and(|f| f.loads > 0);
    // every object goes; the players are re-created below (same entity ids)
    let players = w.players.clone();
    w.ents.retain(|e| players.contains(&e.id));
    w.enemies.clear();
    w.modal = None;
    w.g.shakes.clear();
    w.g.shake_off = (0, 0);
    // 2. map-screen visited bits: story flag 0x1C0 + bit (table 0x0874CA24) — TODO: the table is not exported yet
    // 2. the map-screen visited bit (story flag 0x1C0 + bit)
    crate::fieldworld::mark_visited(w, to.world, to.area);
    // 3. the map + the camera snap
    let col = fd.collision(m);
    w.arena = arena_of(a, m, &col);
    w.col = Some(Collision::new(col));
    {
        let f = w.field.as_mut().expect("field");
        f.area = to;
        f.key = a.key.clone();
        f.map = m.id.clone();
        f.camera_max = m.camera_max;
        f.combat = a.combat;
        f.props = m.props.iter().map(|p| PropRt { rect: p.rect, visible: p.visible, dx: 0 }).collect();
        f.placed.clear();
        f.triggers.clear();
        f.deferred.clear();
        f.debris.clear();
        f.label = None;
        f.flight = None;
        f.patrols.clear();
        f.item_get = None;
        f.mode = Mode::Normal;
        f.busy = false;
        f.entered = w.frame;
        f.loads += 1;
    }
    for s in 0..w.players.len().max(1) {
        snap_camera(w, s, x, y);
    }
    // 4. map sprites, breakables, items (forward, conditions), then the trigger list (map header triggers, area
    // triggers; scanned area first), the song, the actors
    for list in [4, 2, 3] {
        for o in a.objects.iter().filter(|o| objects && list_of(o) == list) {
            if cond_holds(w, &o.cond) {
                spawn_object(w, o);
            }
        }
    }
    for o in a.objects.iter().filter(|o| objects && list_of(o) == 0).chain(m.exits.iter()) {
        if cond_holds(w, &o.cond) {
            spawn_object(w, o);
        }
    }
    if let Some(s) = song_of(a) {
        w.events.push(Event::Song { id: s });
    }
    for o in a.objects.iter().filter(|o| objects && list_of(o) == 1) {
        if cond_holds(w, &o.cond) {
            spawn_object(w, o);
        }
    }
    // 5. the old area's on-leave, the new area's on-enter (both before the player exists)
    if had_area {
        w.events.push(Event::Field(FieldEvent::AreaLeave { area: from }));
        if let Some(sc) = fd.area(from.world, from.area).and_then(|a| a.on_leave.clone()) {
            w.events.push(Event::Field(FieldEvent::Run { run: Callback::Script(sc), cause: Cause::AreaLeave, slot: 0 }));
        }
    }
    if let Some(sc) = &a.on_enter {
        w.events.push(Event::Field(FieldEvent::Run { run: Callback::Script(sc.clone()), cause: Cause::AreaEnter, slot: 0 }));
    }
    // the player objects at the destination (leader first; partners next to it), after the actors in the list: they
    // update before them. Each new player draws 2 x rand(255).
    for (slot, &pid) in players.iter().enumerate() {
        let Some(k) = w.ents.iter().position(|e| e.id == pid) else { continue };
        let ent = w.ents.remove(k);
        let Body::Player(old) = ent.body else { continue };
        let mut p = renew_player(w, &old);
        let (fw, fh) = (p.a.foot_w, p.a.foot_h);
        let (sx, sy) = if slot == 0 { (x, y) } else { w.col.as_ref().unwrap().free_spot(crate::world::Layer::Walk, x + 24 * slot as i32, y, fw, fh, None, None) };
        p.a.x = sx << 8;
        p.a.y = sy << 8;
        p.a.facing = facing.unwrap_or(old.a.facing);
        w.ents.push(crate::world::Entity { id: pid, removed: false, body: Body::Player(Box::new(p)) });
        w.rng.rand(255);
        w.rng.rand(255);
    }
    // the 3D world map: the flying player (z 60, heading north, speed 0)
    if m.three_d {
        if let Some(f) = w.field.as_mut() {
            f.flight = Some(crate::fieldworld::Flight::arrive());
        }
        if let Some(i) = w.player_idx(0) {
            let p = w.pl(i);
            p.a.facing = 1;
            p.ap.raw = 16;
            p.ap.frame = 0;
        }
    }
    w.story.variant_byte = variant;
    w.events.push(Event::Field(FieldEvent::AreaEnter { area: to, key: a.key.clone() }));
}

fn song_of(a: &AreaDef) -> Option<i32> {
    match a.song {
        crate::fieldcontent::Song::Id(s) => Some(s),
        _ => None,
    }
}

/// the arena runtime + collision of a field state (byte snapshots re-attach the static map)
pub fn rebuild_arena(data: &crate::content::GameData, f: &Field) -> Option<(ArenaRt, Arc<crate::content::MapCollision>)> {
    let fd = data.field.as_ref()?;
    let a = fd.area(f.area.world, f.area.area)?;
    let m = fd.maps.get(&f.map)?;
    let col = fd.collision(m);
    Some((arena_of(a, m, &col), col))
}

fn arena_of(a: &AreaDef, m: &MapDef, col: &crate::content::MapCollision) -> ArenaRt {
    // the game's walk test only bounds the box to 0..2048 (0x08009324); the camera clamps to the map
    let side = if m.three_d { m.world_w.max(m.world_h).max(2048) } else { 2048 };
    ArenaRt { id: a.key.clone(), map: m.id.clone(), bounds: Rect::new(0, 0, side, side), region: None, view: Some(Rect::new(0, 0, col.world_w.max(GBA_W), col.world_h.max(GBA_H))) }
}

// ---------------------------------------------------------------------------------------------------------------
// camera

/// the field camera clamp (`0x08009744`): negative -> 0; the integer part above the max -> max (a camera up to
/// 1023/1024 px past the max is kept)
fn clamp_cam(v: i32, max: i32) -> i32 {
    if v < 0 {
        0
    } else if (v >> 10) > max {
        max << 10
    } else {
        v
    }
}

fn set_rect(w: &mut World, slot: usize) {
    let c = &mut w.cameras[slot];
    let (cx, cy) = (c.x >> 10, c.y >> 10);
    c.rect = Rect::new(cx, cy, cx + GBA_W, cy + GBA_H);
}

/// EnterArea's snap: (dest.x - 112, dest.y - 76) << 10, clamped
pub fn snap_camera(w: &mut World, slot: usize, x: i32, y: i32) {
    let max = w.field.as_ref().map(|f| f.camera_max).unwrap_or((0, 0));
    let c = &mut w.cameras[slot.min(MAX_PLAYERS - 1)];
    c.x = clamp_cam((x - 112) << 10, max.0);
    c.y = clamp_cam((y - 76) << 10, max.1);
    c.init = true;
    set_rect(w, slot.min(MAX_PLAYERS - 1));
    let c = &mut w.cameras[slot.min(MAX_PLAYERS - 1)];
    c.shown = c.rect;
}

/// the camera object (0x080077EC): eases toward the footprint centre (3/64, 9/128 per frame, 22.10), field clamp
pub fn camera(w: &mut World, slot: usize) {
    // a script owns the camera (scene.rs: follow / scroll / pan / set)
    if slot == 0 && crate::scene::camera(w) {
        return;
    }
    let Some(pi) = w.player_idx(slot) else { return };
    let a = &w.pl_ref(pi).a;
    let tx = ((a.px() * 2 + a.foot_w) >> 1) - 120;
    let ty = ((a.py() * 2 + a.foot_h) >> 1) - 84;
    let max = w.field.as_ref().map(|f| f.camera_max).unwrap_or((0, 0));
    let c = &mut w.cameras[slot];
    c.shown = c.rect;
    if !c.init {
        c.x = tx << 10;
        c.y = ty << 10;
        c.init = true;
    } else {
        c.x += (((tx << 10) - c.x) * 3) >> 6;
        c.y += (((ty << 10) - c.y) * 9) >> 7;
    }
    c.x = clamp_cam(c.x, max.0);
    c.y = clamp_cam(c.y, max.1);
    set_rect(w, slot);
}

// ---------------------------------------------------------------------------------------------------------------
// per frame

/// before the entity update: the deferred calls due (start of the world update); the item-get presentation holds the
/// players' input
pub fn pre_step(w: &mut World) {
    // an event (nested loop inside the player update) or the item-get holds the players; the raw keys stay in scene.keys
    if w.field.as_ref().is_some_and(|f| f.item_get.is_some() || f.busy) {
        w.input = Default::default();
    }
    crate::fieldobj::run_deferred(w);
}

/// a map NPC's update (scene.rs dispatches `Body::Npc` with a behaviour here)
pub fn npc_update(w: &mut World, i: usize) {
    crate::fieldnpc::update(w, i)
}

/// a map placement by its script actor tag (actor id -(tag + 1) = list 1 index `tag`): placed enemies and NPCs
pub fn actor_by_tag(w: &World, tag: i32) -> Option<usize> {
    let f = w.field.as_ref()?;
    if let Some(&(id, ..)) = f.placed.iter().find(|p| p.1 == ObjRef::List(1, tag as u16)) {
        return w.find(id);
    }
    (0..w.ents.len()).find(|&k| matches!(&w.ents[k].body, Body::Npc(n) if n.tag == Some(tag)))
}

/// an object def of the current area (area lists, then the map header's exits)
pub fn area_obj(w: &World, obj: ObjRef) -> Option<ObjDef> {
    trigger_def(w, obj)
}

/// the world never freezes in the field: fades update every object
pub fn frozen(_w: &World) -> bool {
    false
}

fn begin_fade_in(w: &mut World) {
    if let Some(f) = w.field.as_mut() {
        f.mode = Mode::FadeIn { k: 0 };
    }
    // the world map's fade-in has no autowalk: the flyer is under control
    if crate::fieldworld::flying(w) {
        return;
    }
    for s in 0..w.players.len() {
        if let Some(i) = w.player_idx(s) {
            player::start_autowalk(w, i);
        }
    }
}

/// after the entity update: the trigger check, the fade timeline, cameras, placed enemies' defeats, story sync,
/// play time
pub fn post_step(w: &mut World) {
    let mode = w.field.as_ref().map(|f| f.mode.clone()).unwrap_or(Mode::Normal);
    crate::fieldobj::item_get_step(w);
    crate::fieldobj::debris_step(w);
    match mode {
        Mode::Normal => trigger_check(w),
        Mode::FadeOut { k, to, x, y, facing } => {
            let k = k + 1;
            if k >= FADE {
                // the mode's destructor: the old player goes idle (1 x rand(255)), then EnterArea in the same frame
                for s in 0..w.players.len() {
                    if let Some(i) = w.player_idx(s) {
                        player::set_idle(w, i);
                    }
                }
                load_area(w, to, x, y, facing);
                begin_fade_in(w);
            } else if let Some(f) = w.field.as_mut() {
                f.mode = Mode::FadeOut { k, to, x, y, facing };
            }
        }
        Mode::FadeIn { k } => {
            let k = k + 1;
            let flying = crate::fieldworld::flying(w);
            if flying && k >= crate::fieldworld::FADE_IN_3D {
                if let Some(f) = w.field.as_mut() {
                    f.mode = Mode::Normal;
                }
                crate::fieldworld::reset_state(w);
            } else if flying {
                if let Some(f) = w.field.as_mut() {
                    f.mode = Mode::FadeIn { k };
                }
            } else if k >= FADE {
                if let Some(f) = w.field.as_mut() {
                    f.mode = Mode::Normal;
                }
                for s in 0..w.players.len() {
                    if let Some(i) = w.player_idx(s) {
                        player::set_idle(w, i);
                    }
                }
            } else if let Some(f) = w.field.as_mut() {
                f.mode = Mode::FadeIn { k };
            }
        }
    }
    // the script camera (scene.rs) owns player 0's camera while a script drives it
    let scripted = crate::scene::camera(w);
    if crate::fieldworld::flying(w) {
        crate::fieldworld::camera(w);
    }
    for s in 0..w.players.len() {
        if s == 0 && (scripted || crate::fieldworld::flying(w)) {
            continue;
        }
        camera(w, s);
    }
    defeats(w);
    if let Some(f) = w.field.as_mut() {
        f.event_ended = false;
    }
    story::sync_from_players(w);
    w.story.play_frames += 1;
}

/// `vfunc+0x74(1, 0)` (TriggerSelf, `0x0800F866`): the defeated handler without loot; the object stays (the event's
/// warp reloads the area). Emits `Defeated` and runs the on-defeat callback.
pub fn trigger_self(w: &mut World, i: usize) {
    let id = w.ents[i].id;
    let (x, y, ty) = (w.en_ref(i).a.px(), w.en_ref(i).a.py(), w.en_ref(i).ty);
    let obj = w.field.as_ref().and_then(|f| f.placed.iter().find(|p| p.0 == id).map(|p| p.1));
    let cb = obj.and_then(|o| area_obj(w, o)).and_then(|d| match d.kind {
        ObjKind::Enemy { on_defeat, .. } => on_defeat,
        _ => None,
    });
    w.events.push(Event::Field(FieldEvent::Defeated { entity: id, ty, x, y }));
    if let Some(run) = cb {
        w.events.push(Event::Field(FieldEvent::Run { run, cause: Cause::Defeat { entity: id, ty, x, y }, slot: 0 }));
    }
}

/// placed enemies that are gone: defeated (their on-defeat callback runs with the loot position)
fn defeats(w: &mut World) {
    let placed = w.field.as_ref().map(|f| f.placed.clone()).unwrap_or_default();
    let mut keep = Vec::with_capacity(placed.len());
    let fd = field_data(w);
    let area = w.field.as_ref().map(|f| (f.area.world, f.area.area)).unwrap_or_default();
    for (id, obj, lx, ly) in placed {
        match w.enemy_by_id(id) {
            Some((_, e)) if !e.defeated => {
                let (x, y) = (e.a.px(), e.a.py());
                keep.push((id, obj, x, y));
            }
            other => {
                let (x, y) = other.map(|(_, e)| (e.a.px(), e.a.py())).unwrap_or((lx, ly));
                let def = fd.area(area.0, area.1).and_then(|a| a.objects.iter().find(|o| o.at == obj));
                let (ty, cb) = match def.map(|d| &d.kind) {
                    Some(ObjKind::Enemy { ty, on_defeat, .. }) => (*ty, on_defeat.clone()),
                    _ => (-1, None),
                };
                // the dying enemy sets the loot position (0x03001010) for its drop script; INFERRED: its position, no velocity
                crate::fieldobj::set_loot(w, x, y, 0, 0);
                w.events.push(Event::Field(FieldEvent::Defeated { entity: id, ty, x, y }));
                if let Some(run) = cb {
                    w.events.push(Event::Field(FieldEvent::Run { run, cause: Cause::Defeat { entity: id, ty, x, y }, slot: 0 }));
                }
            }
        }
    }
    if let Some(f) = w.field.as_mut() {
        f.placed = keep;
    }
    let alive: Vec<EntityId> = w.enemies.iter().copied().filter(|&id| w.find(id).is_some()).collect();
    w.enemies = alive;
}

/// input mask of a push direction (`0x080707D8`): down, up, left, right
const PUSH_KEY: [u16; 4] = [crate::input::DOWN, crate::input::UP, crate::input::LEFT, crate::input::RIGHT];

fn trigger_def(w: &World, obj: ObjRef) -> Option<ObjDef> {
    let fd = field_data(w);
    let f = w.field.as_ref()?;
    let a = fd.area(f.area.world, f.area.area)?;
    match obj {
        ObjRef::Header(_) => fd.maps.get(&f.map).and_then(|m| m.exits.iter().find(|o| o.at == obj)).cloned(),
        _ => a.objects.iter().find(|o| o.at == obj).cloned(),
    }
}

/// the trigger check (`0x080126D4`): the player's footprint top-left (integer px) against each trigger's rect
/// (x0 <= px < x1, y0 <= py < y1) in list order; the first whose activation passes fires and the scan stops. Only
/// with a live player (HP != 0). Co-op: the players are tested in slot order (slot 0 = the game's player).
pub fn trigger_check(w: &mut World) {
    let Some(f) = w.field.as_mut() else { return };
    if f.busy || f.item_get.is_some() {
        return;
    }
    // the list's check counter (+0xC4), frozen while an event runs
    f.checks = f.checks.wrapping_add(1);
    let checks = f.checks;
    let trigs = f.triggers.clone();
    for s in 0..w.players.len() {
        let Some(pi) = w.player_idx(s) else { continue };
        let pl = w.pl_ref(pi);
        if pl.rec.hp == 0 || pl.dead {
            continue;
        }
        let (px, py, facing, z) = (pl.a.px(), pl.a.py(), pl.a.facing, -pl.a.z);
        let inp = w.input[s];
        // coins' touch triggers were appended last: scanned first
        if crate::fieldobj::coin_check(w, px, py, s as u8) {
            return;
        }
        for (k, t) in trigs.iter().enumerate() {
            let r = t.rect;
            if !(r.x0 <= px && px < r.x1 && r.y0 <= py && py < r.y1) {
                continue;
            }
            let ok = match t.act {
                Activation::Touch | Activation::TouchOnce => true,
                Activation::Button(d) => inp.pressed & crate::input::A != 0 && d.is_none_or(|d| d == facing),
                // the world-map flyer's altitude (22.10, exclusive bounds); no other player has one
                Activation::Altitude(lo, hi) => w.field.as_ref().and_then(|f| f.flight.as_ref()).is_some_and(|f| (lo << 10) < f.z && f.z < (hi << 10)) || (z > lo && z < hi && z != 0),
                // on entry: not tested in the previous pass
                Activation::Unknown(0x0800_7791) => {
                    let fresh = t.last.wrapping_add(1) != checks;
                    if let Some(x) = w.field.as_mut().and_then(|f| f.triggers.get_mut(k)) {
                        x.last = checks;
                    }
                    fresh
                }
                // UP held (then removed)
                Activation::Unknown(0x0800_71A7) => inp.held & crate::input::UP != 0,
                // A pressed (then removed)
                Activation::Unknown(0x0802_266F) => inp.pressed & crate::input::A != 0,
                Activation::Unknown(_) => false,
            };
            if !ok {
                continue;
            }
            fire(w, s, k, t);
            return;
        }
    }
}

fn fire(w: &mut World, slot: usize, k: usize, t: &TriggerRt) {
    let loads = w.field.as_ref().map(|f| f.loads).unwrap_or(0);
    let Some(def) = trigger_def(w, t.obj) else { return };
    let ObjKind::Trigger { kind, .. } = def.kind else { return };
    let host = w.field.as_ref().is_some_and(|f| f.host);
    let obj = t.obj;
    let s8 = slot as u8;
    match kind {
        TriggerKind::Exit { to, push } | TriggerKind::EdgeExit { to, push } => {
            if w.input[slot].held & PUSH_KEY[push as usize & 3] == 0 {
                return;
            }
            let edge = matches!(kind, TriggerKind::EdgeExit { .. });
            exit_to(w, slot, to, push, t.rect, edge);
        }
        TriggerKind::Script(s) => w.events.push(Event::Field(FieldEvent::Run { run: Callback::Script(s), cause: Cause::Trigger { obj }, slot: s8 })),
        TriggerKind::Event(s) | TriggerKind::SavePoint(s) => {
            let _ = host;
            crate::fieldnpc::start_event(w);
            w.events.push(Event::Field(FieldEvent::Run { run: Callback::Event(s), cause: Cause::Trigger { obj }, slot: s8 }));
        }
        // message 0x17: the world map's bottom label (string id)
        TriggerKind::Message { value, .. } => {
            if let Some(f) = w.field.as_mut() {
                f.label = Some(value);
            }
        }
        TriggerKind::Region | TriggerKind::Unknown(_) => {}
    }
    // touch_once: removed after firing unless an area load happened meanwhile
    if matches!(t.act, Activation::TouchOnce | Activation::Unknown(0x0800_71A7) | Activation::Unknown(0x0802_266F)) {
        if let Some(f) = w.field.as_mut() {
            if f.loads == loads && k < f.triggers.len() && f.triggers[k].obj == t.obj {
                f.triggers.remove(k);
            }
        }
    }
}

/// an exit fired (`0x080075AC` door / `0x080071F8` edge): facing = push, autowalk, fade-out toward the destination
/// (door: def (x, y), facing = hi nibble - 1 or the push; edge: player px - (x0, y0) + (x, y), facing = push)
fn exit_to(w: &mut World, slot: usize, to: Dest, push: Dir, rect: Rect, edge: bool) {
    let Some(pi) = w.player_idx(slot) else { return };
    let (px, py) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    let (x, y) = if edge || to.relative { (px - rect.x0 + to.x, py - rect.y0 + to.y) } else { (to.x, to.y) };
    let facing = if edge { push } else { to.facing.unwrap_or(push) };
    for s in 0..w.players.len() {
        if let Some(i) = w.player_idx(s) {
            w.pl(i).a.facing = push;
            player::start_autowalk(w, i);
        }
    }
    let dest = AreaId { world: to.world, area: to.area, variant: to.variant };
    begin_fade_out(w, dest, x, y, Some(facing));
}

fn begin_fade_out(w: &mut World, to: AreaId, x: i32, y: i32, facing: Option<Dir>) {
    if let Some(f) = w.field.as_mut() {
        f.mode = Mode::FadeOut { k: 0, to, x, y, facing };
    }
    w.events.push(Event::Field(FieldEvent::Transition { to }));
}

// ---------------------------------------------------------------------------------------------------------------
// script host API (the natives that change the field)

/// `warp(world, area, variant, x, y)` (immediate load) / `warp_fade` (fade-out first)
pub fn warp(w: &mut World, world: i32, area: i32, variant: i32, x: i32, y: i32, fade: bool) {
    let to = AreaId { world, area, variant };
    if w.field.is_none() {
        return;
    }
    if fade {
        begin_fade_out(w, to, x, y, None);
    } else {
        load_area(w, to, x, y, None);
    }
}

/// `spawn_map_object(index)` / `spawn_map_object_force`: create the current area's object `at` (force = ignore its
/// condition)
pub fn spawn_map_object(w: &mut World, at: ObjRef, force: bool) -> Option<EntityId> {
    let fd = field_data(w);
    let f = w.field.as_ref()?;
    let a = fd.area(f.area.world, f.area.area)?;
    let o = a.objects.iter().find(|o| o.at == at)?.clone();
    if !force && !cond_holds(w, &o.cond) {
        return None;
    }
    spawn_object(w, &o)
}

/// a script / event holds the field (no trigger or talk fires while set; map NPCs freeze, animations tick)
pub fn set_busy(w: &mut World, busy: bool) {
    let frame = w.frame;
    if let Some(f) = w.field.as_mut() {
        if busy && !f.busy {
            f.busy_since = frame;
        }
        if !busy && f.busy {
            f.event_ended = true;
        }
        f.busy = busy;
    }
}

/// a script host drives this field: blocking events set `busy` until the host clears it (`set_busy(false)`)
pub fn attach_host(w: &mut World, on: bool) {
    if let Some(f) = w.field.as_mut() {
        f.host = on;
    }
}
