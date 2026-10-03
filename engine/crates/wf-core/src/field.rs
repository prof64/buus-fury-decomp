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

use crate::battle;
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
    /// a placed enemy was defeated (on-defeat callback; loot position = x, y). `loot`: the field loot block (position,
    /// velocity) this run starts with - the type's drop script runs with its own enemy's (two enemies dying in one
    /// frame drop where each died; Babidi's ship 8-62)
    Defeat {
        entity: EntityId,
        ty: i32,
        x: i32,
        y: i32,
        #[serde(default)]
        loot: Option<(i32, i32, i32, i32)>,
    },
    /// the area's on-enter / the previous area's on-leave bytecode (run during the load, before the player exists)
    AreaEnter,
    AreaLeave,
    /// an enemy AI node queued a script (RunScript: `run` = `header_0xADDR`, the event whose header is ADDR;
    /// InlineScript: `inline_0xADDR`)
    Ai { entity: EntityId },
    /// a level-up (kill XP): the engine's level-up hook, event #3 ("%s has reached level %d!", stat points,
    /// skills learned), run where the game runs it inline
    LevelUp { entity: EntityId },
    /// a closed menu queued it (Options -> Sleep: event s007, menu/options.rs)
    Menu,
    /// a supply used in the Inventory (its use script, supply def +0x30; `menu/inventory.rs`)
    ItemUse { item: i32 },
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
    /// armed by Instant Transmission's arrival (0x080125FA: the test is wrapped by `0x0804CF94` and the rect spans
    /// the world): the next check fires it wherever the player is, then the wrapper restores the test and rect and
    /// runs the original activation
    #[serde(default)]
    pub forced: bool,
}

/// a map prop's live state (header +0x24 list; breakables and scripts change it)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PropRt {
    pub rect: Rect,
    pub visible: bool,
    /// shake offset this frame
    pub dx: i32,
}

/// the rest of an area load after its on-enter script (`load_area_with`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Arrival {
    pub to: AreaId,
    pub x: i32,
    pub y: i32,
    pub facing: Option<Dir>,
    /// the exit's fade-in follows the load (begin_fade_in)
    pub fade_in: bool,
    /// the frame of the load (a script that did not block finishes in it: the frame's camera step is owed)
    pub frame: u32,
    /// a cutscene scene load (`load_scene*` with the players offstage): the players stay offstage after the on-enter
    /// script (no player object, no draws; VERIFIED Babidi's ship switch scripts, e.g. 8-20 -> load_scene 8-26)
    #[serde(default)]
    pub scene: bool,
}

/// the run mode of the field loop
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    /// fade-out (`0x08006D98`, brightness 256 → 0 by 8): k = fade frames done (1..32); the load runs at the end of
    /// the 32nd
    FadeOut {
        k: i32,
        to: AreaId,
        x: i32,
        y: i32,
        facing: Option<Dir>,
        /// brightness step per frame: 0 = the exits' 8 (32 frames); 4 = a script's `warp_fade` (64 frames)
        #[serde(default)]
        step: i32,
    },
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
    /// the hold is a map trigger's blocking inline script (the VM's nested loop, `hold_nested`): the objects do not
    /// think at all (a walking player keeps walking in place: its control does not run, its animation ticks)
    #[serde(default)]
    pub nested: bool,
    /// a script host is attached: blocking events (event triggers, save points, talk) set `busy` until it clears it
    pub host: bool,
    /// trigger check passes (list +0xC4)
    pub checks: u32,
    /// the frame the field became busy (an event started: NPC animations do not tick that frame)
    pub busy_since: u32,
    /// a broken breakable's callback ran at the start of this frame number (`fieldobj::break_done`, before the object
    /// pass): an event it starts holds the frame, the player is not updated in that pass (segment sb 21-5 / 21-11:
    /// the pods)
    #[serde(default)]
    pub busy_pre_pass: u32,
    /// the event ended this frame (NPC animations tick once more)
    pub event_ended: bool,
    /// drop scripts waiting for the death effect (SelfDestruct: the effect object runs the type's drop script after
    /// `death_param` frames): (due frame, enemy, type, x, y)
    #[serde(default)]
    /// (due frame, enemy, type, loot x, loot y, loot vx, loot vy)
    pub pending_drops: Vec<(u32, EntityId, i32, i32, i32, i32, i32)>,
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
    /// (an enemy created at run time through the placed-enemy constructor - a destroyed tank's crew - has the
    /// reference `ObjRef::List(0xff, enemy type)`)
    pub placed: Vec<(EntityId, ObjRef, i32, i32)>,
    /// per placed enemy, its loot block while alive (footprint centre, 1 px per frame along the last hit): what its
    /// death's deferred drop object stores (the entity can be gone when the defeat is noticed)
    pub placed_loot: Vec<(EntityId, i32, i32, i32, i32)>,
    /// kill XP waiting for the next frame's update (player entity, enemy XP)
    pub xp_due: Vec<(EntityId, i64)>,
    /// on-defeat callbacks of placed enemies, run at the start of the next frame's update (the dying enemy's death
    /// handler runs there; story playthrough: the first boss's defeat scene starts the frame after the KO)
    pub defeat_runs: Vec<(Callback, Cause)>,
    /// on-defeat callbacks of enemies whose death effect runs them after its delay (SelfDestruct: with the drop
    /// script; 3-1's robbers: s126 8 frames after the KO): (due frame, callback, cause)
    #[serde(default)]
    pub defeat_later: Vec<(u32, Callback, Cause)>,
    /// death effects waiting behind one whose callback event started in its update (they run the frame after)
    #[serde(default)]
    pub defeat_wait: Vec<EntityId>,
    #[serde(skip)]
    pub drop_held: bool,
    /// this frame's held pass moved the coins (fieldobj.rs held_moves)
    #[serde(skip)]
    pub coins_moved: bool,
    /// event objects an object queued on the world's event list (+0x610: RunScript, Goten's rock hit): (due frame,
    /// callback, cause); the next frame's list pass starts them, the script's first command runs the frame after
    /// (2-90: s121 starts the frame after Goten's RunScript)
    #[serde(default)]
    pub queued_runs: Vec<(u32, Callback, Cause)>,
    /// the running event came from the world's event list: the game runs it before the objects update, so in the
    /// frame it ends the enemies still run their update (2-90: Super Saiyan Goten's AI starts in s121's last frame)
    #[serde(default)]
    pub queued_event: bool,
    /// a level-up hook started at the start of this frame's update: it runs inline (a nested loop), nothing else
    /// updates this frame
    pub held: bool,
    /// the running event holds an interrupted frame (`held`): when it ends, that frame's remaining updates run in
    /// the frame it ends (story playthrough: the fighters move in the frame the level-up popup closes)
    pub held_event: bool,
    pub resume_held: bool,
    /// the frame's input as it was before an event's hold cleared it (`pre_step`): the pass an inline event
    /// interrupted completes with it (`post_step`, resumed)
    #[serde(default)]
    pub raw_input: Option<[crate::input::InputFrame; crate::world::MAX_PLAYERS]>,
    /// between the frame's start (deferred calls) and the objects' update: objects created now get their first
    /// update in this frame's world update
    pub pre_update: bool,
    /// an area load waiting for its on-enter script (the game runs it inside EnterArea, before the player objects
    /// exist: a blocking command in it suspends the load; story playthrough, Grand Kai's planet 1-15 `prop_show`).
    /// The host calls `finish_arrival` when the script is over.
    pub arrival: Option<Arrival>,
    /// the last trigger check fired a trigger (or collected a coin): it was the frame's one activation
    pub check_fired: bool,
    /// a script trigger (A button or touch) fired in this frame's check: the game runs its script from the player's
    /// update (the trigger test), before the cameras. A script that starts with a camera scroll takes the camera
    /// where it was - `cam_before` = player 0's camera before this frame's follow ease (Diablo Desert 11-13: the jump
    /// onto the train while the camera trails the walk); other scripts leave the follow camera as it is
    #[serde(default)]
    pub button_fired: bool,
    #[serde(default)]
    pub cam_before: Option<(i32, i32)>,
    /// pickups in the world when this frame's deferred calls started (coins they drop are checked before the update)
    pub pre_pickups: u32,
    /// frame of the last area load
    pub entered: u32,
    /// number of area loads (touch_once triggers survive only within one)
    pub loads: u32,
    /// the trigger list's enable flag (+0xD0) is clear: Instant Transmission's run turns the checks off while Goku is
    /// away (player.rs `ctl_it_field`)
    #[serde(default)]
    pub trig_off: bool,
    /// each player's 24.8 position when this frame's update began (objects updated before the player read it: the
    /// field list updates last to first; Janemba's arm parts create the strike hand there)
    #[serde(default)]
    pub frame_start_pos: Vec<(i32, i32)>,
    /// a script removed the leader's object (`remove_actor(0)`: s511 on the Other World map); the next load creates
    /// it again from the record (EnterArea always builds the player)
    #[serde(default)]
    pub player_removed: bool,
    /// the last load queued its area's on-enter script (`FieldEvent::Run` with `Cause::AreaEnter`)
    #[serde(default)]
    pub on_enter_queued: bool,
    /// set by a nesting script host around a script's immediate warp (`warp` 0x77 inside an event): the load defers
    /// its arrival to the on-enter script like an exit's load (the game nests the script: 3-1's on-enter prop_hide
    /// holds the player's creation 2 frames after s124 / s129's warps)
    #[serde(default)]
    pub nest_load: bool,
    /// a script's `warp_fade` whose host nests the load's on-enter script (`nest_load` at the fade's load)
    #[serde(default)]
    pub nest_fade: bool,
    /// the running fade-in is a script's `warp_fade` arrival (0x76): brightness by 4 (64 frames), the player idle
    /// and under control, no autowalk (6-1 -> 6-13, the tournament grounds' gate)
    #[serde(default)]
    pub script_fade_in: bool,
    /// the object being updated destroyed one before it in the world list (Majin Buu's eat removes the player
    /// object): the list compacts and it is updated again in this pass (`World::step_entities_list`)
    #[serde(default)]
    pub list_redo: bool,
    /// a new player object was created in this pass (the eat's respawn): it sits at the end of the list from the
    /// pass's end on, and is not updated in this one
    #[serde(default)]
    pub player_new: bool,
    /// the object tile allocator (vram.rs)
    #[serde(default)]
    pub vram: Option<crate::vram::VramState>,
    /// the frame the 3D map's fade-in ended (the flyer runs the next frame although the warp's event still holds)
    #[serde(default)]
    pub fade_end: u32,
    /// set by a nesting script host around `load_scene` (0x73): the area's objects are spawned after its on-enter
    /// script, as the game's load (0x08009030 runs the map's load scripts, then the object loop): a script that blocks
    /// there delays them (8-60's prop_show(1): the fighters appear 3 frames after the load, Babidi's ship 8-63 buttons)
    #[serde(default)]
    pub defer_objects: bool,
    /// the deferred objects' area (`defer_objects`): spawned when the on-enter script ends (`finish_arrival`)
    #[serde(default)]
    pub pending_objects: Option<AreaId>,
    /// entity ids an area load made while an event held the field (lo..hi): never told to stop by the event, they
    /// run while it holds (the world map's patrolling encounters during s515's warp_fade fade-in)
    #[serde(default)]
    pub load_ids: (u32, u32),
    /// the area is a cutscene's backdrop (`load_scene`): its objects are held by the event like any other
    #[serde(default)]
    pub scene_load: bool,
    /// the HUD object's low-HP beep stamp (hud +0x10: logic frame of the last beep, the load's at creation)
    #[serde(default)]
    pub hud_beep: u32,
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
    p.boost = old.boost;
    p.regen_timer = old.regen_timer;
    p.hp_regen_acc = old.hp_regen_acc;
    // (the hit-damage remainder `hp_acc` is the player object's: a load's new player starts it at 0 - app soak of
    // the real route: carried across loads, Goku's HP ran 1 below the game's from 1-13 on)
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
        nested: false,
        busy_since: 0,
        busy_pre_pass: 0,
        checks: 0,
        event_ended: false,
        pending_drops: Vec::new(),
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
        placed_loot: vec![],
        xp_due: vec![],
        defeat_runs: vec![],
        defeat_later: vec![],
        defeat_wait: vec![],
        drop_held: false,
        coins_moved: false,
        queued_runs: vec![],
        queued_event: false,
        held: false,
        held_event: false,
        resume_held: false,
        raw_input: None,
        pre_update: false,
        arrival: None,
        check_fired: false,
        button_fired: false,
        cam_before: None,
        pre_pickups: 0,
        entered: 0,
        loads: 0,
        trig_off: false,
        frame_start_pos: vec![],
        player_removed: false,
        on_enter_queued: false,
        nest_load: false,
        nest_fade: false,
        script_fade_in: false,
        list_redo: false,
        player_new: false,
        vram: Some(crate::vram::VramState::default()),
        fade_end: 0,
        defer_objects: false,
        pending_objects: None,
        load_ids: (0, 0),
        scene_load: false,
        hud_beep: 0,
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
                f.triggers.push(TriggerRt { obj: o.at, rect: *rect, act: *act, last: u32::MAX, forced: false });
            }
            None
        }
        ObjKind::Enemy { ty, x, y, .. } => {
            // the constructor first (spawn_enemy: its SetAnim(0) idle enter draws rand(255); the patrol runner from def
            // +0x18), the AI's first state after it
            let i = crate::battle::spawn_enemy_at(w, *ty, *x, *y, 0, Some(-1), true)?;
            // the placement's own graphics (ctor 0x0800FD55 takes the def's sprite: the same enemy type shows
            // different fighters, story playthrough Grand Kai's planet 1-12)
            if let ObjKind::Enemy { sprite, .. } = &o.kind {
                let content = *sprite - crate::scene::SPRITE_BASE;
                if *sprite > 0 && w.data.sprite(content).is_some() && w.en_ref(i).a.sprite != Some(content) {
                    let d = w.data.clone();
                    // (the ctor allocates the placement's sprite: the type's tiles are given back first)
                    crate::vram::entity_gone(w, i);
                    w.en(i).a.set_sprite(&d, Some(content));
                    crate::vram::actor_created(w, i);
                }
            }
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
            let mut i = i;
            if let Some(st) = w.data.enemy(*ty).map(|t| t.ai) {
                let f = w.g.clock(w.frame);
                let e = w.en(i);
                e.mark1 = f;
                e.mark2 = f;
                i = crate::battle::push_initial(w, i, st);
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
    // a load inside a running event (script warp / scene load) keeps the field held until the event ends
    // also a script that holds an interrupted frame (a defeat callback's script: Pui Pui's warps to 8-28): its end still
    // completes that frame's update (Babidi's ship 8-27 -> 8-28: the new player's idle ticks twice when it ends)
    let keep_busy = (w.scene.depth > 0 || w.field.as_ref().is_some_and(|f| f.held_event)) && w.field.as_ref().is_some_and(|f| f.busy);
    let load_lo = w.next_id;
    let now = w.frame;
    if let Some(f) = w.field.as_mut() {
        f.hud_beep = now;
    }
    // the label entity of a travel menu is an object of the old area
    w.scene.dest_label = None;
    // every object goes; the players are re-created below (same entity ids)
    let players = w.players.clone();
    // a re-created player is a new object: not under the script's control until a command takes it again (6-1's
    // gate script walks the player, warp_fades to 6-13: the new player walks on the pad during the fade-in)
    w.scene.puppets.retain(|p| !players.contains(&p.id));
    // their object tiles too (the players' objects are re-created at the arrival), the HUD's and the banner's
    // first; nothing left: re-initialised
    // (an imported state's objects of classes the model does not know go with the area too: ship 7-2 -> 7-4)
    crate::vram::release(w, crate::vram::Owner::Foreign);
    crate::vram::release(w, crate::vram::Owner::Hud);
    crate::vram::release(w, crate::vram::Owner::Map3d);
    crate::vram::release(w, crate::vram::Owner::HudOut);
    if let Some(v) = w.field.as_mut().and_then(|f| f.vram.as_mut()) {
        v.hud_out.clear();
        v.popups.clear();
        v.delayed.clear();
        v.late.clear();
        v.shadow_set.clear();
        let keys: Vec<u32> = v.keyed.drain(..).map(|k| k.0).collect();
        for k in keys {
            v.owners.retain(|o| o.1 != crate::vram::Owner::Keyed(k) || {
                v.a.free(o.0);
                false
            });
        }
    }
    crate::vram::release(w, crate::vram::Owner::Popup);
    crate::vram::visuals_gone(w);
    // the BG sphere effect is an object of the old area too (segment sb 20-8 -> 20-9: its redraws stop)
    w.scene.visuals.retain(|v| v.kind != crate::scene::VisualKind::EnergySphere);
    crate::vram::release(w, crate::vram::Owner::Ending);
    crate::vram::release(w, crate::vram::Owner::BossBar);
    crate::vram::release(w, crate::vram::Owner::Banner);
    for i in 0..w.ents.len() {
        crate::vram::entity_gone(w, i);
    }
    // the players a scene load sent offstage are destroyed with the objects too (4-21 -> s190's 4-24 scene)
    crate::vram::offstage_gone(w);
    crate::vram::release(w, crate::vram::Owner::Title);
    crate::vram::release(w, crate::vram::Owner::Props);
    crate::vram::load_reset(w);
    w.ents.retain(|e| players.contains(&e.id));
    w.enemies.clear();
    w.modal = None;
    w.g.shakes.clear();
    w.g.shake_off = (0, 0);
    // the follow camera's remembered target belongs to the old area
    w.scene.cam.follow_last = None;
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
        f.placed_loot.clear();
        f.triggers.clear();
        f.deferred.clear();
        f.debris.clear();
        f.label = None;
        f.flight = None;
        f.patrols.clear();
        f.item_get = None;
        f.mode = Mode::Normal;
        f.busy = keep_busy;
        f.entered = w.frame;
        f.loads += 1;
        f.on_enter_queued = false;
    }
    // the props object (`0x0800D15A`, before the area's objects)
    if m.prop_tile_bytes > 0 {
        crate::vram::acquire(w, crate::vram::Owner::Props, m.prop_tile_bytes);
    }
    for s in 0..w.players.len().max(1) {
        snap_camera(w, s, x, y);
    }
    // 4. map sprites, breakables, items (forward, conditions), then the trigger list (map header triggers, area
    // triggers; scanned area first), the song, the actors. A nested scene load spawns the objects after the on-enter
    // script (`defer_objects`)
    let defer = objects && a.on_enter.is_some() && w.field.as_ref().is_some_and(|f| f.defer_objects);
    if let Some(f) = w.field.as_mut() {
        f.pending_objects = defer.then_some(to);
    }
    let objects = objects && !defer;
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
    if let Some(f) = w.field.as_mut() {
        f.load_ids = if keep_busy { (load_lo, w.next_id) } else { (0, 0) };
        f.scene_load = false;
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
        if let Some(f) = w.field.as_mut() {
            f.on_enter_queued = true;
        }
        // with a script host the rest of the load waits for the script (the players stay out of the world). Not
        // while an event holds the field: its host runs one script at a time, and the event waits for this load
        // (the game nests the on-enter script; scripts that block on load are not known inside events yet)
        if w.field.as_ref().is_some_and(|f| f.host && (!f.busy && w.scene.depth == 0 || f.nest_load)) && objects {
            // the players are already offstage: a cutscene scene load
            let scene = !w.scene.offstage.is_empty() && !w.players.iter().any(|&id| w.find(id).is_some());
            crate::scene::send_offstage(w);
            let three_d = m.three_d;
            if let Some(f) = w.field.as_mut() {
                f.arrival = Some(Arrival { to, x, y, facing, fade_in: false, frame: w.frame, scene });
                // the 3D map object's camera exists from the load, at the arrival point (segment sb 21-25)
                if three_d {
                    let mut fl = crate::fieldworld::Flight::arrive();
                    fl.cam[0] = x << 10;
                    fl.cam[1] = y << 10;
                    f.flight = Some(fl);
                }
            }
            return;
        }
    }
    arrive(w, to, x, y, facing);
}

/// the objects a nested scene load deferred to its on-enter script (`defer_objects`), in the load's list order; made
/// while an event holds the field, they run like the load's own (`load_ids`)
fn spawn_pending_objects(w: &mut World) {
    let Some(to) = w.field.as_mut().and_then(|f| f.pending_objects.take()) else { return };
    let fd = field_data(w);
    let Some(a) = fd.area(to.world, to.area) else { return };
    let lo = w.next_id;
    for list in [4, 2, 3, 0, 1] {
        for o in a.objects.iter().filter(|o| list_of(o) == list) {
            if cond_holds(w, &o.cond) {
                spawn_object(w, o);
            }
        }
    }
    let hi = w.next_id;
    if let Some(f) = w.field.as_mut() {
        if f.scene_load {
            // held with the scene (its event)
        } else if f.load_ids.1 > f.load_ids.0 {
            f.load_ids.1 = hi;
        } else if f.busy {
            f.load_ids = (lo, hi);
        }
    }
}

/// a mode that keeps the object world running over the field (the power struggle, `menu::minigame::world_runs`):
/// the cameras ease as in a field frame (the follow camera is part of the world update), nothing else of the field
/// frame runs
pub fn world_mode_cameras(w: &mut World) {
    let scripted = crate::scene::camera(w);
    for s in 0..w.players.len() {
        if s == 0 && scripted {
            continue;
        }
        camera(w, s);
    }
}

/// the area's on-enter script is over: the load's remaining part (the players, the world map flyer, AreaEnter); an
/// exit's fade-in starts. No-op without a pending arrival.
pub fn finish_arrival(w: &mut World) {
    spawn_pending_objects(w);
    let Some(ar) = w.field.as_mut().and_then(|f| f.arrival.take()) else { return };
    if !ar.scene {
        crate::scene::bring_back(w);
    }
    arrive(w, ar.to, ar.x, ar.y, ar.facing);
    if ar.fade_in {
        begin_fade_in(w);
        // a script that blocked finishes as its host resumes it, before this frame's field step: the fade-in's
        // first step is the next frame's (story playthrough 1-15: black for two frames after the player appears)
        if ar.frame != w.frame {
            if let Some(f) = w.field.as_mut() {
                f.mode = Mode::FadeIn { k: -1 };
            }
        }
    }
    // finished in the load's own frame (the script did not block): the frame's player cameras run now, as they did
    // with the players in the world
    if ar.frame == w.frame {
        let scripted = crate::scene::camera_owned(w);
        if scripted {
            // a follow camera found no target in the frame's camera step
            crate::scene::follow_owed(w);
        }
        // the 3D map's camera too (a warp_fade's nested load: Earth 2-8 #15 / 3-1 / 4-1 world-map arrivals)
        if crate::fieldworld::flying(w) {
            crate::fieldworld::camera(w);
        }
        // (not under a script that warped: its next command owns the camera this frame - Earth 3-1 / 3-11: a world
        // map circle's warp, then camera_pan_to from the snap)
        let held = w.field.as_ref().is_some_and(|f| f.busy);
        for s in 0..w.players.len() {
            if s == 0 && (scripted || crate::fieldworld::flying(w) || held) {
                continue;
            }
            camera(w, s);
        }
    }
}

fn arrive(w: &mut World, to: AreaId, x: i32, y: i32, facing: Option<Dir>) {
    let fd = field_data(w);
    let Some(a) = fd.area(to.world, to.area) else { return };
    let variant = to.variant;
    let Some(m) = fd.map_of(a, variant) else { return };
    let players = w.players.clone();
    // the player objects at the destination (leader first; partners next to it), after the actors in the list: they
    // update before them. Each new player draws 2 x rand(255).
    for (slot, &pid) in players.iter().enumerate() {
        // a player a script removed (`remove_actor(0)`: s511, the Other World map) is created from its record
        let (mut p, old_facing) = match w.ents.iter().position(|e| e.id == pid) {
            Some(k) => {
                let ent = w.ents.remove(k);
                let Body::Player(old) = ent.body else { continue };
                (renew_player(w, &old), old.a.facing)
            }
            None if slot == 0 && w.field.as_ref().is_some_and(|f| f.player_removed) => {
                if let Some(f) = w.field.as_mut() {
                    f.player_removed = false;
                }
                let p = make_player(w, 0, w.story.current);
                let f = p.a.facing;
                (p, f)
            }
            None => continue,
        };
        let (fw, fh) = (p.a.foot_w, p.a.foot_h);
        let (sx, sy) = if slot == 0 { (x, y) } else { w.col.as_ref().unwrap().free_spot(crate::world::Layer::Walk, x + 24 * slot as i32, y, fw, fh, None, None) };
        p.a.x = sx << 8;
        p.a.y = sy << 8;
        p.a.facing = facing.unwrap_or(old_facing);
        w.ents.push(crate::world::Entity { id: pid, removed: false, body: Body::Player(Box::new(p)) });
        let pi = w.ents.len() - 1;
        crate::vram::actor_created(w, pi);
        // the idle state is entered twice; the second draw is the idle tick that stays (intro lockstep: the
        // player's blink after the warp)
        w.rng.rand(255);
        let tick = w.rng.rand(255);
        if let Some(Body::Player(p)) = w.ents.last_mut().map(|e| &mut e.body) {
            p.ap.tick = tick;
        }
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
        // the 3D map object's camera exists from the load, behind the flyer (3-1: the load inside s130's warp)
        crate::fieldworld::camera(w);
    }
    // the HUD object after the players (`0x0800B854`), then the banner object (segment sb 4-21); a cutscene's
    // backdrop (the players offstage, `0x08009030`) has neither (segment sb 4-24)
    let scene = !w.scene.offstage.is_empty();
    if !scene {
        crate::vram::hud_created(w);
    }
    crate::scene::banner_load(w, a, m.three_d, scene);
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
    // a screen shake replaces the camera controller (the shake object 0x0804A720 takes the mode's camera slot):
    // the follow does not run while it lasts (story playthrough: the camera holds for a strong hit's 15-frame shake)
    let shaking = w.camera_held_by_shake();
    let c = &mut w.cameras[slot];
    c.shown = c.rect;
    if !c.init {
        c.x = tx << 10;
        c.y = ty << 10;
        c.init = true;
    } else if shaking {
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

/// the player a script's `warp_fade` built into a 2D area, during its fade-in: a free object that reads the keys
/// (the event that warped does not hold it; the script's next command on it takes it)
pub fn free_arrival(w: &World) -> bool {
    w.field.as_ref().is_some_and(|f| f.script_fade_in && matches!(f.mode, Mode::FadeIn { .. }) && f.item_get.is_none()) && !crate::fieldworld::flying(w)
}

/// before the entity update: the deferred calls due (start of the world update); the item-get presentation holds the
/// players' input
pub fn pre_step(w: &mut World) {
    if let Some(f) = w.field.as_mut() {
        f.coins_moved = false;
    }
    let pos: Vec<(i32, i32)> = (0..w.players.len()).filter_map(|s| w.player_idx(s)).map(|i| (w.pl_ref(i).a.x, w.pl_ref(i).a.y)).collect();
    if let Some(f) = w.field.as_mut() {
        f.frame_start_pos = pos;
    }
    // a script's warp_fade arrival in a 2D area ends as its 64th frame starts: the players go idle (1 x rand(255)
    // each), the script goes on in this frame (6-1 -> 6-13: the walk to the pad starts on the fully lit frame)
    if w.field.as_ref().is_some_and(|f| f.script_fade_in && matches!(f.mode, Mode::FadeIn { k } if k >= crate::fieldworld::FADE_IN_3D - 1)) && !crate::fieldworld::flying(w) {
        if let Some(f) = w.field.as_mut() {
            f.mode = Mode::Normal;
            f.script_fade_in = false;
        }
        for s in 0..w.players.len() {
            if let Some(i) = w.player_idx(s) {
                player::set_idle(w, i);
            }
        }
    }
    // an event (nested loop inside the player update) or the item-get holds the players; the raw keys stay in scene.keys
    // (not the player a script's `warp_fade` just built: until the fade-in ends and the script takes it again it is a
    // free object that reads the keys, event or not - 0-21, s026: A punches during the 64-frame fade-in)
    if w.field.as_ref().is_some_and(|f| f.item_get.is_some() || f.busy) && !free_arrival(w) {
        let raw = w.input;
        if let Some(f) = w.field.as_mut() {
            f.raw_input = Some(raw);
        }
        // the world map's flyer reads the keys itself, event or not (s515's warp_fade: it answers A / B during the
        // arrival fade-in)
        let flying = crate::fieldworld::flying(w) && w.field.as_ref().is_some_and(|f| f.item_get.is_none());
        for s in 0..w.input.len() {
            if !(flying && s == 0) {
                w.input[s] = Default::default();
            }
        }
    }
    if let Some(f) = w.field.as_mut() {
        f.pre_update = true;
        f.held = false;
    }
    let pickups_before = w.ents.iter().filter(|e| !e.removed && matches!(e.body, Body::Pickup(_))).count();
    if let Some(f) = w.field.as_mut() {
        f.pre_pickups = pickups_before as u32;
    }
    crate::fieldobj::run_deferred(w);
    // the death effects' kill XP (their first update: new objects update first), before the defeat callbacks (story
    // playthrough: Pikkon's KO levels Goku up; s003 runs, then the defeat scene s103); a level-up runs hook #3 inline,
    // holding the rest of the frame (the event starts now; story playthrough: the other objects neither move nor
    // animate in that frame)
    // (an event already holding the field when the frame starts: see the on-defeat countdown below)
    let event_held = w.field.as_ref().is_some_and(|f| f.busy);
    let due = w.field.as_mut().map(|f| std::mem::take(&mut f.xp_due)).unwrap_or_default();
    for (pid, exp) in due {
        if let Some(pi) = w.find(pid) {
            let n = w.events.len();
            crate::player::gain_xp(w, pi, exp);
            let leveled = w.events[n..].iter().any(|e| matches!(e, Event::Field(FieldEvent::Run { cause: Cause::LevelUp { .. }, .. })));
            if leveled {
                crate::fieldnpc::start_event(w);
                if let Some(f) = w.field.as_mut() {
                    f.held = true;
                    f.held_event = true;
                }
            }
        }
    }
    let runs = w.field.as_mut().map(|f| std::mem::take(&mut f.defeat_runs)).unwrap_or_default();
    let any = !runs.is_empty();
    for (run, cause) in runs {
        w.events.push(Event::Field(FieldEvent::Run { run, cause, slot: 0 }));
    }
    // the defeat callback's scene runs inside the death handler (a nested loop): the rest of the frame is held, like
    // the level-up hook below (story playthrough: the player's punch does not animate in the frame the boss's
    // defeat scene starts)
    if any && w.field.as_ref().is_some_and(|f| f.host) {
        crate::fieldnpc::start_event(w);
        if let Some(f) = w.field.as_mut() {
            f.held = true;
            f.held_event = true;
        }
    }
    death_effects(w, event_held);
}

/// the death effects' drop scripts and on-defeat callbacks that are due (pre step; again in the post step for the
/// effects that waited behind another one's event)
fn death_effects(w: &mut World, event_held: bool) {
    let next = w.frame + 1;
    // an event holding the field holds the on-defeat callback's countdown (the drop script still runs on time):
    // Diablo Desert Express 11-24 - a robber's event s373, 8 frames after the KO, comes 7 frames after the level-up
    // popup that started the frame after the KO has closed
    if let Some(f) = w.field.as_mut().filter(|_| event_held) {
        for d in f.defeat_later.iter_mut() {
            d.0 = d.0.saturating_add(1);
        }
    }
    // the death effects run one after the other (drop script, then the callback): one whose callback is an event
    // starts it inside its update - the effects after it wait for the next frame (11-24, the real route: two robbers
    // down in one frame - the second one's drop script draws a frame after the first one's, under s373)
    if let Some(f) = w.field.as_mut() {
        let first = f.defeat_later.iter().position(|d| d.0 <= next && matches!(d.1, Callback::Event(_)));
        if let Some(k) = first {
            let ent = match &f.defeat_later[k].2 {
                Cause::Defeat { entity, .. } => Some(*entity),
                _ => None,
            };
            // (the hold flag of this frame keeps them out; from the next frame's start on they are due whatever the
            // pass counter says - the event's frame runs no object pass)
            let hold = f.defeat_wait.is_empty();
            if hold {
                for d in f.defeat_later.iter().skip(k + 1).filter(|d| d.0 <= next) {
                    if let Cause::Defeat { entity, .. } = &d.2 {
                        f.defeat_wait.push(*entity);
                    }
                }
                if let Some(at) = ent.and_then(|e| f.pending_drops.iter().position(|d| d.1 == e)) {
                    let later: Vec<EntityId> = f.pending_drops.iter().skip(at + 1).filter(|d| d.0 <= next).map(|d| d.1).collect();
                    for e in later {
                        if !f.defeat_wait.contains(&e) {
                            f.defeat_wait.push(e);
                        }
                    }
                }
                let wait = f.defeat_wait.clone();
                for d in f.defeat_later.iter_mut().filter(|d| matches!(&d.2, Cause::Defeat { entity, .. } if wait.contains(entity))) {
                    d.0 = u32::MAX;
                }
                for d in f.pending_drops.iter_mut().filter(|d| wait.contains(&d.1)) {
                    d.0 = u32::MAX;
                }
            }
        }
    }
    run_due_drops(w);
    // death effects whose delay is over run their enemy's on-defeat callback after its drop script (frame numbering
    // as run_due_drops; 3-1's robbers: the loot draws, then s126), holding the rest of the frame like defeat_runs
    let later: Vec<(Callback, Cause)> = match w.field.as_mut() {
        Some(f) if f.defeat_later.iter().any(|d| d.0 <= next) => {
            let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut f.defeat_later).into_iter().partition(|d| d.0 <= next);
            f.defeat_later = keep;
            due.into_iter().map(|d| (d.1, d.2)).collect()
        }
        _ => vec![],
    };
    if !later.is_empty() {
        for (run, cause) in later {
            w.events.push(Event::Field(FieldEvent::Run { run, cause, slot: 0 }));
        }
        // the hold without start_event's busy: the drop script queued before it ends first and must not release
        // the held update (set_busy(false)); the event's scene start holds the field
        if w.field.as_ref().is_some_and(|f| f.host) {
            if let Some(f) = w.field.as_mut() {
                f.held = true;
                f.held_event = true;
            }
        }
    }
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

/// the world never freezes in the field (fades update every object), except for the frame an inline level-up hook
/// took over
pub fn frozen(w: &World) -> bool {
    w.field.as_ref().is_some_and(|f| f.held)
}

fn begin_fade_in(w: &mut World) {
    if let Some(f) = w.field.as_mut() {
        f.mode = Mode::FadeIn { k: 0 };
    }
    // the world map's fade-in has no autowalk: the flyer is under control; a script's warp_fade arrival neither
    // (the player the load created stays idle, under control)
    if crate::fieldworld::flying(w) || w.field.as_ref().is_some_and(|f| f.script_fade_in) {
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
    let now = w.frame;
    let due: Vec<(u32, Callback, Cause)> = match w.field.as_mut() {
        Some(f) if f.queued_runs.iter().any(|q| q.0 <= now) => {
            let (due, keep) = std::mem::take(&mut f.queued_runs).into_iter().partition(|q| q.0 <= now);
            f.queued_runs = keep;
            due
        }
        _ => vec![],
    };
    if !due.is_empty() {
        if let Some(f) = w.field.as_mut() {
            f.queued_event = true;
        }
    }
    for (_, run, cause) in due {
        w.events.push(Event::Field(FieldEvent::Run { run, cause, slot: 0 }));
    }
    // the death effect whose update runs an inline hook is suspended with it: its drop countdown waits (story
    // playthrough: the coins drop 7 frames after the level-up popup closes, 8 updates after the KO)
    if let Some(f) = w.field.as_mut().filter(|f| f.held_event) {
        for d in f.pending_drops.iter_mut() {
            d.0 = d.0.saturating_add(1);
        }
    }
    // an inline hook's event ended this frame: the update it interrupted completes now
    // the next queued event of the held frame started at once (story playthrough: s003's level-up ends, Pikkon's
    // defeat scene s103 starts in the same frame): the frame stays held until that one ends too
    // the same for the death effects that waited behind this one's event (`death_effects`): the interrupted pass
    // goes on with them, and the next one's event holds it again (11-24: the second robber's drop script and s373
    // come a frame after the first one's; the rest of the pass runs when the last event ends)
    if w.field.as_ref().is_some_and(|f| f.resume_held && !f.busy && !f.defeat_wait.is_empty()) {
        if let Some(f) = w.field.as_mut() {
            let wait = std::mem::take(&mut f.defeat_wait);
            for d in f.defeat_later.iter_mut().filter(|d| matches!(&d.2, Cause::Defeat { entity, .. } if wait.contains(entity))) {
                d.0 = 0;
            }
            for d in f.pending_drops.iter_mut().filter(|d| wait.contains(&d.1)) {
                d.0 = 0;
            }
        }
        // (their coins make the frame's move, their age starts with the pass that completes later)
        if let Some(f) = w.field.as_mut() {
            f.drop_held = true;
        }
        death_effects(w, false);
        if let Some(f) = w.field.as_mut() {
            f.drop_held = false;
            if f.held_event {
                f.resume_held = false;
            }
        }
    }
    if w.field.as_ref().is_some_and(|f| f.resume_held && f.busy) {
        if let Some(f) = w.field.as_mut() {
            f.resume_held = false;
            f.held_event = true;
        }
    }
    let resumed = w.field.as_ref().is_some_and(|f| f.resume_held);
    if resumed {
        // the held update is the rest of the frame the event interrupted: NPCs run their normal update in it, without
        // the extra tick of an event's last frame (story playthrough: King Kai's idle tick after Pikkon's mid-fight
        // dialog advances by 2, not 3)
        let ended = w.field.as_ref().is_some_and(|f| f.event_ended);
        if let Some(f) = w.field.as_mut() {
            f.resume_held = false;
            f.event_ended = false;
        }
        // the interrupted pass goes on with the frame's own input: the nested loop returned inside the object that
        // started it, the objects after it (the players included, when a death effect at the head of the list ran
        // the hook) update normally (12-28: RIGHT held through the Warlord's defeat script - the walk starts in
        // the script's last frame)
        if let Some(raw) = w.field.as_mut().and_then(|f| f.raw_input.take()) {
            w.input = raw;
        }
        // the task list (screen shakes) already ran in this frame's held pass (session.rs): the completed pass is
        // only the objects after the hook (13-45: a crit shake draws once in the frame s393 ends)
        let shakes = std::mem::take(&mut w.g.shakes);
        w.step_entities();
        let new = std::mem::replace(&mut w.g.shakes, shakes);
        w.g.shakes.extend(new);
        if let Some(f) = w.field.as_mut() {
            f.event_ended = ended;
        }
    }
    // an event that ended in this frame's script phase was still running when the game's trigger check ran (deferred
    // calls, player update, trigger check, the other objects, then the scripts): no check this frame (story
    // playthrough: s100, armed during s099, fires only once Instant Transmission's return has turned the checks
    // back on). A held update (defeat scene) instead completes after its event, check included (s090 -> s091).
    let ended_before = w.field.as_ref().is_some_and(|f| f.event_ended) && !resumed;
    let queued = ended_before && w.field.as_ref().is_some_and(|f| f.queued_event);
    if ended_before {
        if let Some(f) = w.field.as_mut() {
            f.queued_event = false;
        }
    }
    if ended_before && (queued || std::env::var("EN_REL").is_ok()) {
        // the enemies' update of the exit frame runs past the busy check (enemy::release_update)
        let n = w.ents.len();
        for k in (0..n).rev() {
            if !w.ents[k].removed && matches!(w.ents[k].body, Body::Enemy(_)) {
                crate::enemy::release_update(w, k);
            }
        }
        // the players' form controllers run too (2-90: the Super Saiyan drain's phase after s121)
        if queued {
            for s in 0..w.players.len() {
                if let Some(pi) = w.player_idx(s) {
                    crate::player::release_passive(w, pi);
                }
            }
        }
    }
    let mode = w.field.as_ref().map(|f| f.mode.clone()).unwrap_or(Mode::Normal);
    crate::fieldobj::item_get_step(w);
    crate::fieldobj::debris_step(w);
    // Instant Transmission's pick (a modal loop of the game, player.rs `start_it`) replaces the frame: no trigger
    // check, the cameras hold (story playthrough: the camera stops on the frame B is pressed)
    let modal = w.modal.is_some() && crate::player::it_picking(w);
    match mode {
        Mode::Normal if modal || ended_before => {}
        Mode::Normal => trigger_check(w),
        // the load is waiting for its on-enter script: the screen stays black
        Mode::FadeOut { .. } if w.field.as_ref().is_some_and(|f| f.arrival.is_some()) => {}
        Mode::FadeOut { k, to, x, y, facing, step } => {
            let k = k + 1;
            if k >= fade_len(step) {
                // a script's fading warp rebuilds the player and its camera at the load: the script camera ends
                if step != 0 {
                    w.scene.cam.mode = crate::scene::CamMode::Player;
                    w.scene.cam.scroll = None;
                }
                // the mode's destructor: the old player goes idle (1 x rand(255)), then EnterArea in the same frame
                // (a script's warp_fade set it idle at its call; its own fade has no such destructor)
                for s in 0..if step != 0 { 0 } else { w.players.len() } {
                    if let Some(i) = w.player_idx(s) {
                        player::set_idle(w, i);
                    }
                }
                // a script's warp_fade inside an event: the game runs the area's on-enter script inside the load, the
                // players arrive when it ends (segment sb 21-25: s479 -> the world map 0-50, its prop_show steps)
                let nest = step != 0 && w.field.as_ref().is_some_and(|f| f.nest_fade);
                if let Some(f) = w.field.as_mut() {
                    f.nest_load = nest;
                    f.nest_fade = false;
                }
                load_area(w, to, x, y, facing);
                if let Some(f) = w.field.as_mut() {
                    f.nest_load = false;
                    f.script_fade_in = step != 0;
                }
                // a load waiting for its on-enter script fades in when it completes (finish_arrival)
                match w.field.as_mut().and_then(|f| f.arrival.as_mut()) {
                    Some(ar) => {
                        ar.fade_in = true;
                        if let Some(f) = w.field.as_mut() {
                            f.mode = Mode::FadeOut { k, to, x, y, facing, step };
                        }
                    }
                    None => begin_fade_in(w),
                }
            } else if let Some(f) = w.field.as_mut() {
                f.mode = Mode::FadeOut { k, to, x, y, facing, step };
            }
        }
        Mode::FadeIn { k } => {
            let k = k + 1;
            let flying = crate::fieldworld::flying(w);
            let scripted = !flying && w.field.as_ref().is_some_and(|f| f.script_fade_in);
            if scripted {
                // a script's warp_fade into a 2D area: brightness by 4 like the 3D arrival, the player under control;
                // it ends at the start of the 64th frame (`pre_step`)
                if let Some(f) = w.field.as_mut() {
                    f.mode = Mode::FadeIn { k };
                }
            } else if flying && k >= crate::fieldworld::FADE_IN_3D {
                let frame = w.frame;
                if let Some(f) = w.field.as_mut() {
                    f.mode = Mode::Normal;
                    f.fade_end = frame;
                }
                crate::fieldworld::reset_state(w);
                // the event whose `warp_fade` ends here exits in this frame (its wait was the fade): a pass it has
                // held since it started (a defeat callback's event: s027 -> s218 -> Gotenks on 0-50) completes now -
                // the map's objects and the flyer update a second time in the fade's last frame
                if w.field.as_ref().is_some_and(|f| f.held_event && f.busy) {
                    if let Some(f) = w.field.as_mut() {
                        f.held_event = false;
                    }
                    let shakes = std::mem::take(&mut w.g.shakes);
                    w.step_entities();
                    let new = std::mem::replace(&mut w.g.shakes, shakes);
                    w.g.shakes.extend(new);
                    let frame = w.frame;
                    if let Some(f) = w.field.as_mut() {
                        f.fade_end = frame;
                    }
                }
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
    let scripted = if modal { crate::scene::camera_owned(w) } else { crate::scene::camera(w) };
    if crate::fieldworld::flying(w) {
        crate::fieldworld::camera(w);
    }
    let button = w.field.as_mut().is_some_and(|f| std::mem::take(&mut f.button_fired));
    let before = w.cameras.first().map(|c| (c.x, c.y));
    if let Some(f) = w.field.as_mut() {
        f.cam_before = if button { before } else { None };
    }
    for s in 0..w.players.len() {
        if s == 0 && (scripted || crate::fieldworld::flying(w)) || modal {
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
    match cb {
        // callback 0x0800424F (run event): the event object joins the world's list and runs in the next frame's pass,
        // which it holds from its start (0-50's airship marker -> s017: the flyer's animation skips that frame's
        // tick, the event's first command runs a frame later)
        Some(run @ Callback::Event(_)) if w.field.as_ref().is_some_and(|f| f.host) => {
            if let Some(f) = w.field.as_mut() {
                f.defeat_runs.push((run, Cause::Defeat { entity: id, ty, x, y, loot: None }));
            }
        }
        // callback 0x0800421F (run bytecode): inline, at once
        Some(run) => w.events.push(Event::Field(FieldEvent::Run { run, cause: Cause::Defeat { entity: id, ty, x, y, loot: None }, slot: 0 })),
        None => {}
    }
}

/// placed enemies that are gone: defeated (their on-defeat callback runs with the loot position)
fn defeats(w: &mut World) {
    let placed = w.field.as_ref().map(|f| f.placed.clone()).unwrap_or_default();
    let mut keep = Vec::with_capacity(placed.len());
    let mut loot_keep = Vec::with_capacity(placed.len());
    let fd = field_data(w);
    let area = w.field.as_ref().map(|f| (f.area.world, f.area.area)).unwrap_or_default();
    for (id, obj, lx, ly) in placed {
        match w.enemy_by_id(id) {
            Some((_, e)) if !e.defeated => {
                let (x, y) = (e.a.px(), e.a.py());
                keep.push((id, obj, x, y));
                let (cx, cy) = (e.a.px() + (e.a.foot_w >> 1), e.a.py() + e.a.foot_h);
                let v = crate::geom::DIR_VEC[e.last_hit_dir as usize & 3];
                loot_keep.push((id, cx, cy, v.0 << 8, v.1 << 8));
            }
            other => {
                let (x, y) = other.map(|(_, e)| (e.a.px(), e.a.py())).unwrap_or((lx, ly));
                // the death's deferred drop object (0x0800F62C copies it to the loot block 0x03001010): the footprint's
                // centre x and bottom y (position + 8, 8) and 1 px per frame along the killing hit (story playthrough: (188, 210)
                // and (201, 230) for fighters last at (179.8, 202.8) / (193.99, 222.7), velocity (256, 0) punched
                // rightwards)
                let (fx, fy, fvx, fvy) = match other {
                    Some((_, e)) => {
                        let (cx, cy) = (e.a.px() + (e.a.foot_w >> 1), e.a.py() + e.a.foot_h);
                        let v = crate::geom::DIR_VEC[e.last_hit_dir as usize & 3];
                        (cx, cy, v.0 << 8, v.1 << 8)
                    }
                    None => w.field.as_ref().and_then(|f| f.placed_loot.iter().find(|l| l.0 == id)).map(|l| (l.1, l.2, l.3, l.4)).unwrap_or((lx, ly, 0, 0)),
                };
                let def = fd.area(area.0, area.1).and_then(|a| a.objects.iter().find(|o| o.at == obj));
                let (ty, cb) = match def.map(|d| &d.kind) {
                    Some(ObjKind::Enemy { ty, on_defeat, .. }) => (*ty, on_defeat.clone()),
                    // no area def: a run-time enemy (a tank's crew, `ObjRef::List(0xff, type)`) keeps its type's
                    // drop script
                    _ => (if let ObjRef::List(0xff, t) = obj { t as i32 } else { -1 }, None),
                };
                // the dying enemy sets the loot block (0x03001010 position, 0x03001018 velocity) for its drop script and
                // its on-defeat callback: the footprint's centre x / bottom y and 1 px per frame along the killing hit
                // (VERIFIED 12-28: the Warlord last at (141, 88), punched rightwards -> (157, 104), (256, 0), read by
                // his callback's spawn_map_item)
                crate::fieldobj::set_loot(w, fx, fy, fvx, fvy);
                w.events.push(Event::Field(FieldEvent::Defeated { entity: id, ty, x, y }));
                // (a tank's death node 0x080333C4 passes its def +0x10 = 8 to the same handler)
                // the type's drop script (def +0x18), run by the death effect after its delay (SelfDestruct param:
                // story playthrough, the fighters on Grand Kai's planet drop 4 coins 8 frames after the KO)
                let (ds, delay) = w.data.enemy(ty).map(|t| (t.drop_script, if t.death_name == "SelfDestruct" { t.death_param as u32 } else if crate::enemy::TANKS.contains(&ty) { 8 } else { 0 })).unwrap_or((0, 0));
                if ds != 0 {
                    let due = w.frame + delay;
                    if let Some(f) = w.field.as_mut() {
                        f.pending_drops.push((due, id, ty, fx, fy, fvx, fvy));
                    }
                }
                if let Some(run) = cb {
                    let due = w.frame + delay;
                    if let Some(f) = w.field.as_mut() {
                        if delay > 0 {
                            f.defeat_later.push((due, run, Cause::Defeat { entity: id, ty, x, y, loot: Some((fx, fy, fvx, fvy)) }));
                        } else {
                            f.defeat_runs.push((run, Cause::Defeat { entity: id, ty, x, y, loot: Some((fx, fy, fvx, fvy)) }));
                        }
                    }
                }
            }
        }
    }
    if let Some(f) = w.field.as_mut() {
        f.placed = keep;
        f.placed_loot = loot_keep;
    }
    let alive: Vec<EntityId> = w.enemies.iter().copied().filter(|&id| w.find(id).is_some()).collect();
    w.enemies = alive;
}

/// death effects whose delay is over run the type's drop script (loot position = where the enemy died); at the start
/// of the world update (the effect objects' update; story playthrough: the drop script's draws come before the
/// enemies' AI)
fn run_due_drops(w: &mut World) {
    // the frame number this update will have (the entities' update advances it)
    let frame = w.frame + 1;
    let due: Vec<(u32, EntityId, i32, i32, i32, i32, i32)> = w.field.as_ref().map(|f| f.pending_drops.iter().copied().filter(|d| d.0 <= frame).collect()).unwrap_or_default();
    if !due.is_empty() {
        if let Some(f) = w.field.as_mut() {
            f.pending_drops.retain(|d| d.0 > frame);
        }
        for (_, id, ty, x, y, vx, vy) in due {
            let ds = w.data.enemy(ty).map(|t| t.drop_script).unwrap_or(0);
            crate::fieldobj::set_loot(w, x, y, vx, vy);
            let run = Callback::Script(format!("inline_0x{ds:08X}"));
            w.events.push(Event::Field(FieldEvent::Run { run, cause: Cause::Defeat { entity: id, ty, x, y, loot: Some((x, y, vx, vy)) }, slot: 0 }));
        }
    }
}

/// a host starts a queued run: the loot block it carries becomes the field's (drop scripts queued in one frame each
/// drop at their own enemy)
pub fn start_cause(w: &mut World, c: &Cause) {
    if let Cause::Defeat { loot: Some((x, y, vx, vy)), .. } = c {
        crate::fieldobj::set_loot(w, *x, *y, *vx, *vy);
    }
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
    // the list's enable flag (+0xD0) is tested first
    if f.trig_off {
        f.check_fired = false;
        return;
    }
    f.check_fired = false;
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
            if let Some(f) = w.field.as_mut() {
                f.check_fired = true;
            }
            return;
        }
        for (k, t) in trigs.iter().enumerate() {
            let r = t.rect;
            if t.forced {
                // the wrapper's rect spans the world and its test passes: restore, then the original activation
                if let Some(x) = w.field.as_mut().and_then(|f| f.triggers.get_mut(k)) {
                    x.forced = false;
                }
                let mut t = t.clone();
                t.forced = false;
                fire(w, s, k, &t);
                return;
            }
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

/// the travel menu (`show_destination_label` 0xD9: label entity vt 0x0804BAF4, update 0x0803669E): the selection is
/// one of the area's triggers. Pressed LEFT / RIGHT / UP / DOWN (checked in that order) move it (0x0803656C) to the
/// trigger whose centre ((x0 + x1 + 15) >> 1, (y0 + y1 + 7) >> 1) lies in that direction: along >= |across|, the
/// smallest along^2 + 9 across^2; A runs the selected trigger's activation (the label index = the trigger index).
pub fn dest_menu(w: &mut World) {
    let pressed = w.input[0].pressed;
    let Some(f) = w.field.as_ref() else { return };
    let trigs: Vec<TriggerRt> = f.triggers.clone();
    let centre = |r: &Rect| ((r.x0 + r.x1 + 15) >> 1, (r.y0 + r.y1 + 7) >> 1);
    // (key, direction vector 0x0807268C)
    for (key, (dx, dy)) in [(crate::input::LEFT, (-1, 0)), (crate::input::RIGHT, (1, 0)), (crate::input::UP, (0, -1)), (crate::input::DOWN, (0, 1))] {
        if pressed & key == 0 {
            continue;
        }
        let cur = w.scene.dest_sel;
        let Some(c) = trigs.get(cur as usize) else { continue };
        let (cx, cy) = centre(&c.rect);
        let mut best = i64::from(i32::MAX);
        for (k, t) in trigs.iter().enumerate() {
            if k as i32 == cur {
                continue;
            }
            let (tx, ty) = centre(&t.rect);
            let (ex, ey) = ((tx - cx) as i64, (ty - cy) as i64);
            let along = ex * dx + ey * dy;
            let across = (ex * dy + ey * dx).abs();
            if along < across {
                continue;
            }
            let score = along * along + 9 * across * across;
            if score < best {
                best = score;
                w.scene.dest_sel = k as i32;
            }
        }
    }
    if pressed & crate::input::A != 0 {
        let k = w.scene.dest_sel as usize;
        if let Some(t) = trigs.get(k) {
            fire(w, 0, k, t);
        }
    }
}

fn fire(w: &mut World, slot: usize, k: usize, t: &TriggerRt) {
    if let Some(f) = w.field.as_mut() {
        f.check_fired = true;
    }
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
        TriggerKind::Script(s) => {
            // (touch triggers too: Diablo Desert Express 11-21's ladder scroll starts while the camera trails)
            if let Some(f) = w.field.as_mut() {
                f.button_fired = true;
            }
            w.events.push(Event::Field(FieldEvent::Run { run: Callback::Script(s), cause: Cause::Trigger { obj }, slot: s8 }))
        }
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

/// the HUD object's update (0x0800BA34; after the actors in the frame's update): HP at or below a fifth of
/// the maximum (and not 0) beeps sound 41 at volume 32 once more than 60 logic frames passed since the last beep
/// (2-9 / 2-8: Gohan low on HP after the circus)
pub fn hud_tick(w: &mut World) {
    crate::vram::hud_tick(w);
    if w.scene.hud_hidden {
        return;
    }
    let Some(pi) = w.player_idx(0) else { return };
    let (hp, max) = (w.pl_ref(pi).rec.hp, w.pl_ref(pi).rec.max_hp);
    if hp == 0 || hp * 5 > max {
        return;
    }
    let now = w.frame;
    let due = w.field.as_ref().is_some_and(|f| now.wrapping_sub(f.hud_beep) as i32 > 60);
    if due {
        if let Some(f) = w.field.as_mut() {
            f.hud_beep = now;
        }
        w.sfx(41, 32);
    }
}

/// brightness level (0..256 = black) of the fade-in after k frames: the 3D world map's arrival fades by 4 (64
/// frames), the 2D areas' by 8
pub fn fade_in_level(w: &World, k: i32) -> i32 {
    let slow = crate::fieldworld::flying(w) || w.field.as_ref().is_some_and(|f| f.script_fade_in);
    let step = if slow { 256 / crate::fieldworld::FADE_IN_3D } else { 8 };
    (256 - step * k).clamp(0, 256)
}

/// frames of a fade-out with brightness step `step` (0 = 8)
pub fn fade_len(step: i32) -> i32 {
    256 / if step > 0 { step } else { 8 }
}

/// brightness level (0..256) of a fade-out after k frames
pub fn fade_out_level(k: i32, step: i32) -> i32 {
    ((if step > 0 { step } else { 8 }) * k).clamp(0, 256)
}

/// `warp_fade` (0x76, handler 0x080022E7): the player goes idle (vt+0x28(0)), a 64-frame fade-out (brightness by 4),
/// then the load (story: s515's world-map take-off holds the idle player and the camera still for 64 frames)
pub fn warp_fade_slow(w: &mut World, world: i32, area: i32, variant: i32, x: i32, y: i32) {
    if w.field.is_none() {
        return;
    }
    // the 3D map's flyer takes the call in its flight state: no idle draw; it turns north and its animation restarts
    // (0-50 -> 0-21, s026: facing 3 -> 1, tick 0, the banking raw kept), then it is held through the fade
    // (`fieldworld::flyer_update`)
    let flying = crate::fieldworld::flying(w);
    for s in 0..w.players.len() {
        if let Some(i) = w.player_idx(s) {
            if flying {
                if s == 0 {
                    if let Some(fl) = w.field.as_mut().and_then(|f| f.flight.as_mut()) {
                        fl.tick = 0;
                        fl.frame = 0;
                    }
                    let p = w.pl(i);
                    p.a.facing = 1;
                    p.ap.frame = 0;
                }
            } else {
                player::set_idle(w, i);
            }
        }
    }
    // the player the load builds faces down, whatever the old one faced (a flyer: 0-21's arrival; a walker: Diablo
    // Desert Express 11-17 -> 11-18, facing left at the call)
    let _ = flying;
    begin_fade_out(w, AreaId { world, area, variant }, x, y, Some(0));
    // the fade object's brightness starts moving two frames after the call (s515 at 3-1: idle at the call, black 63
    // frames later)
    if let Some(Mode::FadeOut { step, k, .. }) = w.field.as_mut().map(|f| &mut f.mode) {
        *step = 4;
        *k = -1;
    }
}

fn begin_fade_out(w: &mut World, to: AreaId, x: i32, y: i32, facing: Option<Dir>) {
    if let Some(f) = w.field.as_mut() {
        f.mode = Mode::FadeOut { k: 0, to, x, y, facing, step: 0 };
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

/// script `spawn_map_item` (0xAA): the area's item object `at`, thrown from the loot block
pub fn throw_map_item(w: &mut World, at: ObjRef) -> Option<EntityId> {
    let fd = field_data(w);
    let f = w.field.as_ref()?;
    let a = fd.area(f.area.world, f.area.area)?;
    let o = a.objects.iter().find(|o| o.at == at)?.clone();
    crate::fieldobj::throw_map_item(w, &o)
}

/// script `refresh_map_triggers` (0x8B, 0x08008AA0(area, 0)): clear the trigger list and re-create the area's
/// triggers whose condition holds now, then the map's exits (the load's order). Used after story flags change.
/// (The game also re-registers the map layout's props; their script-set visibility is kept here.)
pub fn refresh_triggers(w: &mut World) {
    let fd = field_data(w);
    let Some(f) = w.field.as_mut() else { return };
    let (world, area, map) = (f.area.world, f.area.area, f.map.clone());
    f.triggers.clear();
    let Some(a) = fd.area(world, area) else { return };
    let exits = fd.maps.get(&map).map(|m| m.exits.clone()).unwrap_or_default();
    let list: Vec<ObjDef> = a.objects.iter().filter(|o| list_of(o) == 0).cloned().chain(exits).collect();
    for o in &list {
        if cond_holds(w, &o.cond) {
            spawn_object(w, o);
        }
    }
}

/// script `respawn_breakables` (0xAD, 0x08008AFE): every breakable of the area whose condition holds now and that does
/// not exist is created
pub fn respawn_breakables(w: &mut World) {
    let fd = field_data(w);
    let Some(f) = w.field.as_ref() else { return };
    let Some(a) = fd.area(f.area.world, f.area.area) else { return };
    for o in a.objects.iter().filter(|o| matches!(o.kind, ObjKind::Breakable { .. })) {
        let exists = w.ents.iter().any(|e| !e.removed && matches!(&e.body, Body::Breakable(b) if b.obj == o.at));
        if !exists && cond_holds(w, &o.cond) {
            // its map prop shows again (a break hid it)
            if let ObjKind::Breakable { prop, .. } = &o.kind {
                let vis = fd.maps.get(&w.field.as_ref().unwrap().map).and_then(|m| m.props.get(*prop as usize)).map(|p| p.visible).unwrap_or(true);
                if let Some(p) = w.field.as_mut().and_then(|f| f.props.get_mut(*prop as usize)) {
                    p.visible = vis;
                }
            }
            spawn_object(w, o);
        }
    }
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
            if f.held_event {
                f.held_event = false;
                f.resume_held = true;
            }
        }
        f.busy = busy;
        if !busy {
            f.nested = false;
            // the load's own objects ran through the event that loaded them; a later event holds them like the
            // others (segment sb 21-25: the world map's patrols stop at the next circle's event)
            f.load_ids = (0, 0);
        }
    }
    if !busy {
        // the event's scene object is gone: later scenes carry no talk anchor
        w.scene.talk_anchor = crate::scene::NO_ANCHOR;
    }
}

/// a map trigger's inline script blocked (prop_hide / prop_show: 4-20's doors): the game's VM runs it in a nested
/// loop that animates the objects without their think until it ends (the host clears it with `set_busy(false)`)
/// an object queued an event object on the world's event list: it runs at the start of the next frame's update
/// (outside the field: at once)
pub fn queue_run(w: &mut World, run: Callback, cause: Cause) {
    match w.field.as_mut() {
        Some(f) => f.queued_runs.push((w.frame + 1, run, cause)),
        None => w.events.push(Event::Field(FieldEvent::Run { run, cause, slot: 0 })),
    }
}

/// a held frame (`held_event` set for a death effect's on-defeat callback, field.rs `defeat_later`) whose scripts all
/// ended without starting an event: the frame's update runs after all (the game ran the callback inline, nothing
/// blocked)
pub fn release_unstarted_hold(w: &mut World) {
    if let Some(f) = w.field.as_mut().filter(|f| f.held && f.held_event && !f.busy) {
        f.held_event = false;
        f.resume_held = true;
    }
}

pub fn hold_nested(w: &mut World) {
    set_busy(w, true);
    if let Some(f) = w.field.as_mut() {
        f.nested = true;
    }
}

/// a script host drives this field: blocking events set `busy` until the host clears it (`set_busy(false)`)
pub fn attach_host(w: &mut World, on: bool) {
    if let Some(f) = w.field.as_mut() {
        f.host = on;
    }
}

/// `save_field_on_next_scene` + the next `load_scene*` (0x08009196): the field as it is (world descriptor, camera,
/// map objects, entities) kept for `return_to_field` (0x75), which restores it without a load (no object or player
/// re-creation, no RNG draws; VERIFIED Babidi's ship switch scripts 8-20 / 8-23 / 8-25)
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct SavedWorld {
    pub ents: Vec<crate::world::Entity>,
    pub enemies: Vec<crate::world::EntityId>,
    pub field: Option<Box<Field>>,
    pub cameras: Vec<crate::world::Camera>,
    pub enemy_layer: Option<Vec<u8>>,
    pub walk_layer: Option<Vec<u8>>,
    /// the camera control (the scene load fixes it; the restore gives the follow camera back)
    pub cam: crate::scene::CamCtl,
    /// the actors under script control (a map NPC the event walked keeps its scripted animation after the return:
    /// 8-16 s319's Kibito)
    #[serde(default)]
    pub puppets: Vec<crate::scene::Puppet>,
}

pub fn save_world(w: &World) -> SavedWorld {
    let mut ents = w.ents.clone();
    // players a scene already sent offstage belong to the saved field
    ents.extend(w.scene.offstage.iter().cloned());
    SavedWorld {
        ents,
        enemies: w.enemies.clone(),
        field: w.field.clone(),
        cameras: w.cameras.to_vec(),
        enemy_layer: w.col.as_ref().map(|c| c.enemy.clone()),
        walk_layer: w.col.as_ref().and_then(|c| c.walk.clone()),
        cam: w.scene.cam.clone(),
        puppets: w.scene.puppets.clone(),
    }
}

/// `return_to_field` with a saved field: everything back as it was (the players included)
pub fn restore_world(w: &mut World, s: SavedWorld) {
    w.scene.offstage.clear();
    w.scene.cam = s.cam;
    w.ents = s.ents;
    w.enemies = s.enemies;
    w.field = s.field;
    // the save was taken inside the `load_scene` that replaced the field: that call's transient load flags are not
    // part of the field it keeps (s215 saves the Lookout in a nested load_scene; after return_to_field, s216's
    // warp spawned 6-1's NPCs after the player instead of before)
    if let Some(f) = w.field.as_mut() {
        f.defer_objects = false;
        f.pending_objects = None;
        f.nest_load = false;
        f.scene_load = false;
    }
    for p in s.puppets {
        if w.find(p.id).is_some() && !crate::scene::is_puppet(w, p.id) {
            w.scene.puppets.push(p);
        }
    }
    for (k, c) in s.cameras.into_iter().enumerate().take(w.cameras.len()) {
        w.cameras[k] = c;
    }
    let data = w.data.clone();
    if let Some((arena, m)) = w.field.as_ref().and_then(|f| rebuild_arena(&data, f)) {
        w.arena = arena;
        let mut c = crate::world::Collision::new(m);
        if let Some(l) = s.enemy_layer {
            c.enemy = l;
        }
        c.walk = s.walk_layer;
        w.col = Some(c);
    }
}
