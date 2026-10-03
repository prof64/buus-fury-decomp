//! The player: the game's state objects and their animation ticks (panim), movement (walk 5*SPD+102, dash, the
//! walking try-move 0x08012960), melee, block + energy shield, hurt / bump / KO, forms (SS / SS3 gauges,
//! transformations, Saiyaman), the skills, Instant Transmission's modal pick, the hold states enemies put the player
//! in, and every sound call as an event. Verified frame by frame against the emulator (lockstep traces,
//! tools/battle/LOCKSTEP.md). Reference: site/src/battle/rulesets/buu/player.ts + panim.ts.

use crate::actor::Actor;
use crate::content::{AnimRef, Equip, SkillEngine};
use crate::damage::{atk, compute_damage, CombatStats, HpAcc};
use crate::fx::{self, Alive, Fx};
use crate::geom::{overlaps, Dir, Rect, DIR_VEC};
use crate::input::{self as btn, InputFrame, DPAD};
use crate::progression::{level_of, level_up, xp_gain, EffStats, Record};
use crate::shots::{self, ShotKind, StraightOpts};
use crate::world::{broadcast, Body, EntityId, Event, HitMsg, World};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const PUNCH_RECT: [[i32; 4]; 4] = [[0, 0, 16, 16], [0, -24, 16, 0], [-8, -24, 0, 8], [16, -24, 24, 8]];
const FACE_TBL: [[u8; 4]; 9] = [[0, 1, 2, 3], [2, 2, 2, 2], [3, 3, 3, 3], [1, 1, 1, 1], [1, 1, 2, 2], [1, 1, 3, 3], [0, 0, 0, 0], [0, 0, 2, 2], [0, 0, 3, 3]];
fn skill_raw(id: i32) -> Option<i32> {
    Some(match id {
        1 | 2 | 3 | 6 | 11 | 15 | 16 => 19,
        4 | 7 | 23 => 4,
        5 | 8 | 9 | 10 | 13 => 20,
        17 => 13,
        _ => return None,
    })
}
fn skill_check(id: i32) -> Option<&'static [i32]> {
    Some(match id {
        1 => &[200],
        2 => &[10, 33, 100],
        3 => &[12],
        4 => &[35, 105, 420],
        5 => &[300],
        6 => &[15, 50, 150],
        8 => &[25, 70, 290],
        9 => &[350],
        10 => &[15, 45, 140],
        11 => &[15, 44, 145],
        13 => &[160],
        14 => &[40],
        15 => &[15, 45, 150],
        16 => &[15, 45, 150],
        17 => &[50, 200, 800],
        23 => &[350],
        _ => return None,
    })
}
fn skill_mult(id: i32, lv: i32) -> i32 {
    let t: &[i32] = match id {
        1 => &[1640],
        3 => &[164],
        4 => &[410, 820, 1649],
        5 => &[328],
        6 | 10 | 11 | 15 | 16 => &[41, 164, 328],
        8 => &[62, 144, 226],
        9 => &[10240],
        17 => &[3687, 11060, 16589],
        _ => &[1024],
    };
    t[(lv as usize).min(t.len() - 1)]
}
fn skill_sub(id: i32) -> u8 {
    match id {
        1 => 14,
        3 => 17,
        4 => 10,
        5 => 18,
        6 => 6,
        7 => 4,
        8 => 9,
        9 => 15,
        10 => 11,
        11 => 12,
        15 => 5,
        16 => 8,
        17 => 7,
        23 => 16,
        _ => 1,
    }
}
/// a skill's damage multiplier: the content's `engine.damage` (mods), else the built-in table of its behaviour
fn skill_mult_of(w: &World, real: i32, lv: i32) -> i32 {
    w.data.skill_engine(real).and_then(|e| SkillEngine::pick(&e.damage, lv)).unwrap_or_else(|| skill_mult(w.data.skill_kind(real), lv))
}
/// a skill's attack sub-type: the content's `engine.attack_sub` (mods), else the built-in one of its behaviour
fn skill_sub_of(w: &World, real: i32) -> u8 {
    w.data.skill_engine(real).and_then(|e| e.attack_sub).unwrap_or_else(|| skill_sub(w.data.skill_kind(real)))
}
const SWORD_BOX: [[i32; 4]; 8] = [[39, 10, 32, 8], [29, 2, 32, 32], [-4, 0, 8, 32], [-13, 2, 32, 32], [-23, 10, 32, 8], [-12, 46, 32, 32], [-4, 46, 8, 32], [28, 46, 32, 32]];
const VOLLEY_POS: [(i32, i32); 4] = [(10, 8), (10, -20), (-8, -8), (24, -8)];
const DASH_VEC: [(i32, i32); 4] = [(0, 1), (0, -1), (-1, 0), (1, 0)];
const KNOCK_VEC: [(i32, i32); 4] = [(0, 192), (0, -192), (-192, 0), (192, 0)];
const BLOCK_PUSH: [(i32, i32); 4] = [(0, -512), (0, 512), (512, 0), (-512, 0)];
const ENERGY_PUNCH_RECT_L3: [[i32; 4]; 4] = [[0, 0, 24, 28], [-4, -36, 20, 0], [-20, -24, 0, 8], [16, -24, 36, 8]];
const BURNING_HOLD: [(i32, i32); 4] = [(8, -2), (12, -2), (19, -6), (-3, -6)];
const BEAM_CHARGE_KAME: [(i32, i32); 4] = [(8, -2), (12, -2), (12, -2), (4, -2)];
const BEAM_CHARGE_BANSHEE: [(i32, i32); 4] = [(2, 6), (13, -25), (-8, -6), (24, -6)];
const BBK_FLARE_POS: [(i32, i32); 4] = [(8, 16), (8, -32), (-16, -6), (32, -6)];
const FLAME_FIRST: [(i32, i32); 4] = [(8, 16), (8, -32), (0, -6), (16, -6)];
/// SetAnim id -> raw animation (0x08072670 + 1)
const ANIM_RAW: [i32; 25] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 11, 12];
const ANIM_STATE: [Gs; 25] = [
    Gs::Idle, Gs::Walk, Gs::Dash, Gs::Once3, Gs::Punch, Gs::Punch, Gs::Punch, Gs::Hold0, Gs::Bored, Gs::Once3, Gs::LoopC, Gs::LoopC, Gs::Once4, Gs::LoopC, Gs::LoopC,
    Gs::LoopB, Gs::OnceHold, Gs::Once4, Gs::Once4, Gs::Once4, Gs::Once4, Gs::Once4, Gs::Once4, Gs::SsUp, Gs::SsRevert,
];

/// the game's state object at obj+0x14 (lockstep names)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gs {
    Idle,
    Idle2,
    Walk,
    Dash,
    Bored,
    Punch,
    Once3,
    Hurt,
    Once4,
    Ko,
    OnceHold,
    Hold,
    Hold0,
    Block,
    LoopB,
    LoopC,
    SsUp,
    SsRevert,
    Skill(u8),
    Skill18r,
    /// field transitions: the autowalk state (0x08010D7C, state 0x03002B54)
    AutoWalk,
    /// Dabura's stone state (vtable 0x0804AB34)
    Stone,
    /// Majin Buu's candy: idle (0x0804BE60), hop (0x0804B7AC), being eaten (0x0804C2CC)
    Candy,
    CandyWalk,
    CandyEat,
}
impl Gs {
    pub fn name(self) -> String {
        match self {
            Gs::Idle | Gs::Idle2 => "idle".into(),
            Gs::Walk => "walk".into(),
            Gs::AutoWalk => "autowalk".into(),
            Gs::Stone => "vt_0804AB34".into(),
            Gs::Candy => "candy".into(),
            Gs::CandyWalk => "candy_walk".into(),
            Gs::CandyEat => "candy_eat".into(),
            Gs::Dash => "dash".into(),
            Gs::Bored => "bored".into(),
            Gs::Punch => "punch".into(),
            Gs::Once3 => "once3".into(),
            Gs::Hurt => "hurt".into(),
            Gs::Once4 => "once4".into(),
            Gs::Ko => "ko".into(),
            Gs::OnceHold => "once_hold".into(),
            Gs::Hold => "hold".into(),
            Gs::Hold0 => "hold0".into(),
            Gs::Block => "block".into(),
            Gs::LoopB => "loop_b".into(),
            Gs::LoopC => "loop_c".into(),
            Gs::SsUp => "ss_up".into(),
            Gs::SsRevert => "ss_revert".into(),
            Gs::Skill(n) => format!("skill{n}"),
            Gs::Skill18r => "skill18r".into(),
        }
    }
    fn is_skill(self) -> bool {
        matches!(self, Gs::Skill(_) | Gs::Skill18r)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PState {
    Idle,
    Walk,
    Dash,
    Punch,
    Block,
    Hurt,
    Dying,
    Skill,
    Transform,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HoldKind {
    Vampire,
    Hug,
    Latch,
    Eat,
    Freeze,
    Stone,
    Grapple,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnimPos {
    pub raw: i32,
    pub frame: i32,
    pub tick: i32,
}

#[derive(Clone, Debug, Default, Hash, Serialize, Deserialize)]
pub struct SkillInst {
    pub id: i32,
    pub serial: u32,
    pub t: i32,
    pub held: bool,
    pub release: i32,
    pub proj: EntityId,
    pub fx: EntityId,
    pub acc: i32,
    pub refire: bool,
    pub ghosts: Vec<EntityId>,
    pub target: bool,
    pub charged: bool,
    pub charge: i32,
    pub hits: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct ItSel {
    pub serial: u32,
    pub n: i32,
    pub cand: Vec<(EntityId, bool)>,
    pub picks: Vec<EntityId>,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub cursor: bool,
    /// field: the trigger candidates after the enemies (a trigger whose test is `0x0804AA08`, made by activation
    /// `0x0802266F`, answers the pick: rect (x0 + 15, y0 + 7, x1, y1)), picked flag
    #[serde(default)]
    pub tcand: Vec<(crate::fieldcontent::ObjRef, Rect, bool)>,
    /// field: every pick in order (enemies and triggers)
    #[serde(default)]
    pub order: Vec<ItPick>,
}
/// one Instant Transmission pick in the field (the selector's out list {kind byte, object}: kind 1 = trigger)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItPick {
    Ent(EntityId),
    Trig(crate::fieldcontent::ObjRef),
}
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct ItRun {
    pub serial: u32,
    pub dir: Dir,
    pub idx: usize,
    pub targets: Vec<EntityId>,
    pub origin: (i32, i32),
    pub t: i32,
    /// field: the picks in order (the field's run, `ctl_it_field`)
    #[serde(default)]
    pub picks: Vec<ItPick>,
}

/// the partner record that takes over at a cutscene fusion's defuse
#[derive(Clone, Debug, Default, Hash, Serialize, Deserialize)]
pub struct Partner {
    pub form: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub ep: i32,
    pub max_ep: i32,
    pub base: [i32; 4],
    pub skills: Vec<i32>,
    pub slot: usize,
    pub level0: i32,
    pub ss_gauge: i32,
    pub ss3_gauge: i64,
    pub cooldown: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Player {
    pub a: Actor,
    pub slot: u8,
    pub form: i32,
    pub rec: Record,
    pub equip: [i32; 4],
    pub eff: EffStats,
    pub skills: Vec<i32>,
    pub skill_level: i32,
    pub sel: usize,
    pub state: PState,
    pub t: i32,
    pub sk: Option<SkillInst>,
    pub hp_acc: HpAcc,
    pub ep_acc: i32,
    pub regen_timer: i32,
    pub hp_regen_acc: i32,
    pub dead: bool,
    pub held_by: EntityId,
    pub hold_kind: Option<HoldKind>,
    pub hold_left: i32,
    pub hold_count: i32,
    pub hold_last: i64,
    pub blast: EntityId,
    /// stuck mummy bandages (their stat reapply 0x0802D8E6 counts them)
    pub wraps: i32,
    /// the stone state's counter (150 at the enter; key presses -5, damage -dmg >> 10)
    pub stone_hp: i32,
    /// candy: the shown sprite def is 0x36 (sprite 45) instead of the form's
    pub candy_def: bool,
    /// candy hop: the move code kept in the walk state (1 / 2 left / right + 3 / 6 up / down)
    pub candy_code: i32,
    /// the player object is out of the world (Majin Buu's eat, tick 8 .. 119): no update, no hits, not drawn
    pub gone: bool,
    pub ss_gauge: i32,
    pub ss_acc: i32,
    pub ss3_gauge: i64,
    pub ss_cooldown: i32,
    pub holy_left: i32,
    /// the timed buff's STR / POW / END / SPD while `holy_left` > 0 (Holy Water: +25 each; script `boost_stats`)
    pub boost: [i32; 4],
    pub last_dir_key: u16,
    pub last_dir_frame: i64,
    pub walk: i32,
    pub dash_spd: i32,
    pub transform_to: i32,
    pub forms_allowed: Vec<i32>,
    pub punch_anim: i32,
    pub gs: Gs,
    pub ap: AnimPos,
    pub s_cnt: i32,
    pub shield: Option<(i32, bool)>,
    pub resist: i32,
    pub hurt_dir: Dir,
    pub transform_done: bool,
    pub big_bang: EntityId,
    /// fired beams still out: their destructor sounds (stop = true / play) once gone
    pub beam_end: Vec<(EntityId, Vec<(bool, i32)>)>,
    pub ghost_rng: Vec<(u32, u32)>,
    pub it_sel: Option<ItSel>,
    pub it: Option<ItRun>,
    pub ko_at: i64,
    pub dealt_before: i32,
    /// story Trunks forms 24 / 25: the form object's countdown (+0xC)
    pub story_cnt: i32,
    /// cutscene fusions (form object 0x03001B64): countdown (+0x10, 300 at the construction)
    pub fus_cnt: i32,
    /// the defuse script: (frames since, start x px)
    pub defuse: Option<(i32, i32)>,
    pub partner: Option<Partner>,
}

pub struct PlayerInit {
    pub slot: u8,
    pub form: i32,
    pub rec: Record,
    pub equip: [i32; 4],
    pub skills: Vec<i32>,
    pub skill_level: i32,
    pub sel: usize,
    pub holy_water: bool,
    pub forms_allowed: Vec<i32>,
}

impl Player {
    pub fn new(w: &World, init: PlayerInit) -> Player {
        let mut p = Player {
            a: Actor::default(),
            slot: init.slot,
            form: init.form,
            rec: init.rec,
            equip: init.equip,
            eff: EffStats::default(),
            sel: init.sel.min(init.skills.len().saturating_sub(1)),
            skills: init.skills,
            skill_level: init.skill_level,
            state: PState::Idle,
            t: 0,
            sk: None,
            hp_acc: HpAcc::default(),
            ep_acc: 0,
            regen_timer: 0,
            hp_regen_acc: 0,
            dead: false,
            held_by: 0,
            hold_kind: None,
            hold_left: 0,
            hold_count: 0,
            hold_last: 0,
            blast: 0,
            wraps: 0,
            stone_hp: 0,
            candy_def: false,
            candy_code: 0,
            gone: false,
            ss_gauge: 0,
            ss_acc: 0,
            ss3_gauge: 0,
            ss_cooldown: 0,
            holy_left: if init.holy_water { 1800 } else { 0 },
            boost: crate::progression::HOLY_WATER,
            last_dir_key: 0,
            last_dir_frame: -1000,
            walk: 0,
            dash_spd: 0,
            transform_to: -1,
            forms_allowed: init.forms_allowed,
            punch_anim: 0,
            gs: Gs::Idle,
            ap: AnimPos { raw: 1, frame: 0, tick: 0 },
            s_cnt: 0,
            shield: None,
            resist: 128,
            hurt_dir: 0,
            transform_done: false,
            big_bang: 0,
            beam_end: vec![],
            ghost_rng: vec![],
            it_sel: None,
            it: None,
            ko_at: -1,
            dealt_before: 0,
            story_cnt: 0,
            fus_cnt: 300,
            defuse: None,
            partner: None,
        };
        p.apply_form(w, init.form);
        p
    }
    pub fn hp(&self) -> i32 {
        self.rec.hp
    }
    pub fn max_hp(&self) -> i32 {
        self.rec.max_hp
    }
    pub fn ep(&self) -> i32 {
        self.rec.ep
    }
    pub fn max_ep(&self) -> i32 {
        self.rec.max_ep
    }
    pub fn skill(&self) -> i32 {
        self.skills.get(self.sel).copied().unwrap_or(7)
    }
    pub fn attacking(&self) -> bool {
        matches!(self.state, PState::Punch | PState::Skill | PState::Dash)
    }
    /// the state's vfunc +0x1C (IfInPlayerBeamLine 0x08029B30): Ki Blast (0x0804FA30) = the refire request or anim
    /// tick < 16 (until the blast leaves the hand); other skills: INFERRED any skill state
    pub fn charging(&self) -> bool {
        if self.gs == Gs::Skill(7) {
            return self.sk.as_ref().is_some_and(|s| s.refire) || self.ap.tick < 16;
        }
        self.state == PState::Skill
    }
    /// IfPlayerPetrified 0x0803AE65: the stone pose
    pub fn is_stone(&self) -> bool {
        self.gs == Gs::Stone || self.hold_kind == Some(HoldKind::Stone)
    }
    pub fn swinging(&self) -> bool {
        self.state == PState::Punch
    }
    /// story Trunks: form 24 (0x03001B88) / 25 (0x03001B8C)
    pub fn story_form(&self, w: &World) -> bool {
        let c = self.form_ref(w).controller;
        c == 0x3001b88 || c == 0x3001b8c
    }
    /// 0x0800FF44: the shot origin on the body box: down (cx, position y), up (cx, cy), left (x0, cy), right (x1, cy)
    pub fn hand_point(&self) -> (i32, i32) {
        let b = self.a.body_box();
        let (cx, cy) = b.center();
        match self.a.facing {
            0 => (cx, self.a.py()),
            1 => (cx, cy),
            2 => (b.x0, cy),
            _ => (b.x1, cy),
        }
    }
    fn form_ref<'a>(&self, w: &'a World) -> &'a crate::content::Form {
        w.data.form(self.form).expect("form")
    }
    pub fn apply_form(&mut self, w: &World, id: i32) {
        self.form = id;
        let f = w.data.form(id).expect("form");
        let mut spr = None;
        for k in 0..=3 {
            if w.data.sprite(f.gfx - 9 - k).is_some() {
                spr = Some(f.gfx - 9 - k);
                break;
            }
        }
        self.a.set_sprite(&w.data, spr);
        self.refresh_stats(w);
    }
    fn equips<'a>(&self, w: &'a World) -> Vec<Option<&'a Equip>> {
        self.equip.iter().map(|&id| w.data.equipment.iter().find(|e| e.id == id)).collect()
    }
    /// RecalcStats 0x08013C66 + the form bonuses
    pub fn refresh_stats(&mut self, w: &World) {
        let eq = self.equips(w);
        self.eff = crate::progression::recalc_stats_with(&self.rec, &eq, (self.holy_left > 0).then_some(self.boost));
        let b = w.data.form(self.form).expect("form");
        let e = self.eff;
        self.a.stats = CombatStats { str_: e.str_ + b.b_str, pow: e.pow + b.b_pow, end: e.end + b.b_end, end2: e.end2 + b.b_end2, res_m: self.resist, res_e: self.resist, crit_bonus: e.crit_pct * 656, block_factor: e.block_factor };
        let wv = 5 * e.spd + 102;
        if b.b_walk < 0 && b.id == 24 {
            self.walk = wv >> 2;
            self.dash_spd = (wv + 320) >> 2;
        } else {
            self.walk = wv + b.b_walk;
            self.dash_spd = wv + 320 + b.b_dash;
        }
    }
    fn walk_speed(&self) -> i32 {
        0.max(self.walk - 48 * self.wraps)
    }
    fn dash_speed(&self) -> i32 {
        0.max(self.dash_spd - 48 * self.wraps)
    }
    fn scaled(&self, cost: i32) -> i32 {
        (cost * self.eff.ep_cost_factor) >> 4
    }
    fn can_afford(&self, w: &World, cost: i32) -> bool {
        let c = self.scaled(cost) as i64;
        let f = self.form_ref(w);
        if f.ss3 {
            return (self.ss3_gauge >> 16) >= c;
        }
        if f.ss {
            return self.ss_gauge as i64 >= c;
        }
        self.rec.ep as i64 >= c
    }
    fn energy_left(&self, w: &World) -> i64 {
        let f = self.form_ref(w);
        if f.ss3 {
            self.ss3_gauge >> 16
        } else if f.ss {
            self.ss_gauge as i64
        } else {
            self.rec.ep as i64
        }
    }
    fn anim_n(&self, w: &World) -> i32 {
        match self.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|s| s.anim_raw(self.ap.raw)) {
            Some(a) => a.nframes(),
            None => 0,
        }
    }
    fn has_anim(&self, w: &World, raw: i32) -> bool {
        self.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|s| s.anim_raw(raw)).is_some()
    }
    fn idle_state(&self, w: &World) -> Gs {
        if self.form_ref(w).controller == 0x3001b64 {
            Gs::Idle2
        } else {
            Gs::Idle
        }
    }
    /// the sprite shown = obj+0x50 / +0x11
    fn sync_anim(&mut self, w: &World) {
        let Some(sp) = self.a.sprite.and_then(|s| w.data.sprite(s)) else { return };
        self.a.anim = sp.by_raw.get(&self.ap.raw).or_else(|| sp.by_raw.get(&1)).copied();
    }
    fn skill_lv(&self, w: &World, id: i32) -> i32 {
        match w.data.skills.get(&id) {
            Some(s) if s.max_level == 2 => self.skill_level.min(2),
            _ => 0,
        }
    }
    fn skill_cost(&self, w: &World, id: i32) -> i32 {
        let Some(s) = w.data.skills.get(&id) else { return 0 };
        let lv = self.skill_lv(w, id);
        if let Some(c) = s.engine.as_ref().and_then(|e| SkillEngine::pick(&e.cost, lv)) {
            return c;
        }
        let real = id;
        let id = w.data.skill_kind(real);
        if id == 7 {
            return ((self.rec.max_ep as i64 * 0xccd + 65535) >> 16) as i32;
        }
        if matches!(id, 20 | 22 | 18 | 12) {
            return if id == 20 { 10 } else { 0 };
        }
        s.costs.get((lv as usize).min(s.costs.len().saturating_sub(1))).copied().unwrap_or(s.min_ep)
    }
}

// ---- free functions on (world, entity index) ---------------------------------------------------------------------------

fn p(w: &mut World, i: usize) -> &mut Player {
    w.pl(i)
}
fn pr(w: &World, i: usize) -> &Player {
    w.pl_ref(i)
}
fn inp(w: &World, i: usize) -> InputFrame {
    w.input[pr(w, i).slot as usize]
}
fn sfx(w: &mut World, id: i32, vol: i32) {
    w.sfx(id, vol);
}
fn sfx_stop(w: &mut World, id: i32) {
    w.sfx_stop(id);
}

fn spend(w: &mut World, i: usize, cost: i32, scaled_already: bool) {
    let c = if scaled_already { cost } else { pr(w, i).scaled(cost) };
    if c <= 0 {
        return;
    }
    let (ss, ss3) = {
        let f = pr(w, i).form_ref(w);
        (f.ss, f.ss3)
    };
    let pl = p(w, i);
    if ss3 {
        pl.ss3_gauge = 0.max(pl.ss3_gauge - c as i64 * 65536);
    } else if ss {
        pl.ss_gauge = 0.max(pl.ss_gauge - c);
    } else {
        let before = pl.rec.ep;
        pl.rec.ep = 0.max(pl.rec.ep - c);
        let d = before - pl.rec.ep;
        w.stats.ep_spent += d as i64;
    }
}
fn drain(w: &mut World, i: usize, per256: i32) {
    let pl = p(w, i);
    pl.ep_acc += (per256 * pl.eff.ep_cost_factor) >> 4;
    let n = pl.ep_acc >> 8;
    pl.ep_acc &= 0xff;
    if n != 0 {
        spend(w, i, n, true);
    }
}
fn set_resist(w: &mut World, i: usize, v: i32) {
    if pr(w, i).resist == v {
        return;
    }
    p(w, i).resist = v;
    let d = Arc::clone(&w.data);
    let _ = d;
    let mut pl = pr(w, i).clone();
    pl.refresh_stats(w);
    *p(w, i) = pl;
}
fn refresh(w: &mut World, i: usize) {
    let mut pl = pr(w, i).clone();
    pl.refresh_stats(w);
    *p(w, i) = pl;
}
fn apply_form(w: &mut World, i: usize, id: i32) {
    let mut pl = pr(w, i).clone();
    pl.apply_form(w, id);
    *p(w, i) = pl;
}

/// Actor.setAnim for the skill code: the named animation (n -> raw n + 1 / name) becomes obj+0x50
fn set_anim_named(w: &mut World, i: usize, refs: &[AnimRef], restart: bool) {
    let Some(sp) = pr(w, i).a.sprite.and_then(|s| w.data.sprite(s)) else { return };
    let mut found = None;
    for r in refs {
        let a = match r {
            AnimRef::N(n) => sp.by_raw.get(&(n + 1)),
            AnimRef::Name(s) => sp.by_name.get(*s),
        };
        if let Some(&a) = a {
            found = Some(sp.anims[a as usize].raw);
            break;
        }
    }
    let Some(raw) = found else { return };
    let pl = p(w, i);
    if raw != pl.ap.raw || restart {
        pl.ap.tick = 0;
        pl.ap.frame = 0;
    }
    pl.ap.raw = raw;
    let mut c = pl.clone();
    c.sync_anim(w);
    *p(w, i) = c;
}

/// field: the autowalk state of an exit / area arrival (0x08010D7C): not hittable, the walk animation
pub fn start_autowalk(w: &mut World, i: usize) {
    p(w, i).ap.raw = ANIM_RAW[1];
    enter_state(w, i, Gs::AutoWalk);
    p(w, i).a.hittable = false;
}
/// the end of a fade: the player goes idle (SetAnim(0): idle enter, 1 x rand(255))
pub fn set_idle(w: &mut World, i: usize) {
    set_anim_id(w, i, 0);
}

/// a player is in Instant Transmission's target pick (the selector's modal loop replaces the frame)
pub fn it_picking(w: &World) -> bool {
    (0..w.ents.len()).any(|k| !w.ents[k].removed && matches!(&w.ents[k].body, Body::Player(p) if p.it_sel.is_some()))
}

/// the item-get pose ends while Instant Transmission's field run waits for the event its arrival fired: the run's
/// state comes back (story playthrough s099: raw 20 cel 0), the vanish resumes after the event
pub fn resume_it(w: &mut World, i: usize) -> bool {
    if w.field.is_none() || pr(w, i).it.is_none() {
        return false;
    }
    enter_state(w, i, Gs::Skill(14));
    let pl = p(w, i);
    pl.state = PState::Skill;
    pl.ap.raw = 20;
    pl.ap.frame = 0;
    true
}

/// scripts: SetAnim(id) with a script anim id (table 0x08072670 applied by ANIM_RAW / ANIM_STATE)
pub fn script_set_anim(w: &mut World, i: usize, id: usize) {
    set_anim_id(w, i, id.min(ANIM_RAW.len() - 1));
}

/// scripts: `set_pose` (vfunc +0x30): the static pose state (0x03001B00) holding anim `id` on cel `frame`
pub fn script_pose(w: &mut World, i: usize, id: usize, frame: i32) {
    let raw = ANIM_RAW[id.min(ANIM_RAW.len() - 1)];
    enter_state(w, i, Gs::Hold);
    let pl = p(w, i);
    pl.ap.raw = raw;
    pl.ap.frame = frame;
    pl.ap.tick = 0;
}

/// scripts: the state's animation has finished (vfunc +0x34): back in idle, or holding
pub fn script_anim_done(w: &World, i: usize) -> bool {
    matches!(pr(w, i).gs, Gs::Idle | Gs::Idle2 | Gs::Hold | Gs::Hold0)
}

/// SetAnim(id) (vfunc +0x28 0x0800ECF0)
fn set_anim_id(w: &mut World, i: usize, id: usize) {
    p(w, i).ap.raw = ANIM_RAW[id];
    let gs = if id == 0 { pr(w, i).idle_state(w) } else { ANIM_STATE[id] };
    enter_state(w, i, gs);
}
fn coarse(pl: &Player, gs: Gs) -> PState {
    match gs {
        Gs::Walk | Gs::AutoWalk => PState::Walk,
        Gs::Dash => PState::Dash,
        Gs::Punch => PState::Punch,
        Gs::Block => PState::Block,
        Gs::Hurt => PState::Hurt,
        Gs::Ko => PState::Dying,
        Gs::Hold => {
            if pl.dead {
                PState::Dying
            } else if pl.state == PState::Skill {
                PState::Skill
            } else {
                PState::Idle
            }
        }
        Gs::SsUp | Gs::SsRevert | Gs::Skill(20) | Gs::Skill(18) | Gs::Skill18r => PState::Transform,
        g if g.is_skill() => PState::Skill,
        _ => PState::Idle,
    }
}
/// SetState (0x0800ED18): exit the old state object, enter the new one
fn enter_state(w: &mut World, i: usize, gs: Gs) {
    let old = pr(w, i).gs;
    match old {
        Gs::Block => {
            let pl = p(w, i);
            pl.a.blocking = false;
            pl.shield = None;
        }
        Gs::Hurt => set_resist(w, i, 128),
        Gs::Skill(20) | Gs::SsUp => {
            if !pr(w, i).transform_done {
                let ss = pr(w, i).form_ref(w).ss;
                let pl = p(w, i);
                if ss {
                    pl.ss3_gauge = 0;
                } else {
                    pl.ss_gauge = 0;
                }
            }
            sfx_stop(w, 54);
        }
        _ => {}
    }
    if pr(w, i).state == PState::Skill && !gs.is_skill() {
        end_skill(w, i);
    }
    {
        let pl = p(w, i);
        pl.gs = gs;
        pl.t = 0;
    }
    match gs {
        Gs::Idle => {
            let r = w.rng.rand(255);
            let pl = p(w, i);
            pl.ap.tick = r;
            pl.ap.frame = 0;
            pl.s_cnt = 0;
            pl.a.hittable = true;
            set_resist(w, i, 128);
        }
        Gs::Idle2 => {
            let r = w.rng.rand(255);
            let pl = p(w, i);
            pl.ap.tick = r;
            pl.ap.frame = 0;
        }
        Gs::Hold => {}
        // candy states: enter t = 0 (0x0804EA21 / 0x0804E921 / 0x0804EAD5), the frame kept
        Gs::Candy | Gs::CandyEat => p(w, i).ap.tick = 0,
        Gs::CandyWalk => {
            let pl = p(w, i);
            pl.ap.tick = 0;
            pl.candy_code = 0;
        }
        Gs::Punch => {
            {
                let pl = p(w, i);
                pl.ap.tick = 0;
                pl.ap.frame = 0;
            }
            let s = 50 + w.rng.rand(2);
            sfx(w, s, 64);
        }
        Gs::Block => {
            let pl = p(w, i);
            pl.ap.tick = 0;
            pl.ap.frame = 0;
            pl.a.blocking = true;
            pl.s_cnt = 0;
            pl.shield = None;
        }
        Gs::Hurt => {
            {
                let pl = p(w, i);
                pl.ap.tick = 0;
                pl.ap.frame = 0;
            }
            set_resist(w, i, 0);
        }
        Gs::Skill(20) | Gs::SsUp => {
            {
                let pl = p(w, i);
                pl.ap.tick = 0;
                pl.ap.frame = 0;
                pl.transform_done = false;
            }
            sfx(w, 54, 64);
        }
        Gs::SsRevert => {
            let pl = p(w, i);
            pl.ap.tick = 0;
            pl.ap.frame = 0;
            pl.a.hittable = false;
        }
        _ => {
            let pl = p(w, i);
            pl.ap.tick = 0;
            pl.ap.frame = 0;
        }
    }
    let pl = p(w, i);
    pl.state = coarse(pl, gs);
    if pl.state == PState::Dying {
        pl.dead = true;
        pl.a.hittable = false;
    }
}
/// coarse state for the AI / skill code: 'idle' = SetAnim(0)
fn set_state_idle(w: &mut World, i: usize) {
    set_anim_id(w, i, 0);
}

// ---- PlayerLike (the enemies' interface) ---------------------------------------------------------------------------------

pub fn hold(w: &mut World, i: usize, by: EntityId, kind: HoldKind, frames: i32) {
    hold_raw(w, i, by, kind, frames, 10)
}
/// `raw`: the animation the holder shows (frame 0); default the hurt animation's first frame (raw 10)
pub fn hold_raw(w: &mut World, i: usize, by: EntityId, kind: HoldKind, frames: i32, raw: i32) {
    if pr(w, i).dead {
        return;
    }
    end_skill(w, i);
    let frame = w.frame as i64;
    {
        let pl = p(w, i);
        pl.held_by = by;
        pl.hold_kind = Some(kind);
        pl.hold_left = frames;
        pl.hold_count = 0;
        pl.hold_last = frame;
    }
    enter_state(w, i, Gs::Hold);
    let pl = p(w, i);
    pl.state = PState::Held;
    pl.t = 0;
    pl.a.blocking = false;
    if kind == HoldKind::Eat {
        pl.a.alpha = 0;
    } else {
        pl.ap.raw = raw;
    }
    pl.ap.frame = 0;
}
/// 0x0800ED18(player, 0x03001B00): the hold state (frame kept, no tick, no control)
pub fn enter_hold(w: &mut World, i: usize) {
    if pr(w, i).dead {
        return;
    }
    end_skill(w, i);
    enter_state(w, i, Gs::Hold);
}
/// SetAnim(0) (vfunc +0x28 with id 0)
pub fn to_idle(w: &mut World, i: usize) {
    if pr(w, i).dead {
        return;
    }
    set_anim_id(w, i, 0);
}
/// SetAnimRaw(raw - 1), frame = `frame`, SetState(0x03001B00) (BearHug enter 0x0802C4C9)
pub fn hold_pose(w: &mut World, i: usize, raw: i32, frame: i32) {
    if pr(w, i).dead {
        return;
    }
    end_skill(w, i);
    {
        let pl = p(w, i);
        pl.ap.raw = raw;
        pl.ap.frame = frame;
    }
    enter_state(w, i, Gs::Hold);
}
/// SetAnim(id) (vfunc +0x28)
pub fn set_anim_by_id(w: &mut World, i: usize, id: usize) {
    set_anim_id(w, i, id);
}

// ---- Dabura's stone (vtable 0x0804AB34): the spit's accepted hit (0x0802E60A, after the normal hit) does SetAnimRaw(0x15)
// (raw 22) + SetState: counter 150; its damage reaction is counter -= damage >> 10 (HP still lost); control
// 0x0802E384: a new A / B / d-pad press -> counter - 5; frame = counter < 75; counter <= 0 -> a puff at the body centre
// and SetAnim(0). A new spit builds a new state (150 again)
pub fn petrify(w: &mut World, i: usize) {
    if pr(w, i).dead {
        return;
    }
    p(w, i).ap.raw = 22;
    enter_state(w, i, Gs::Stone);
    p(w, i).stone_hp = 150;
}
fn ctl_stone(w: &mut World, i: usize) {
    let pressed = inp(w, i).pressed;
    let pl = p(w, i);
    if pressed & 0xf3 != 0 {
        pl.stone_hp -= 5;
    }
    pl.ap.frame = (75 > pl.stone_hp) as i32;
    if pl.stone_hp > 0 {
        return;
    }
    let (cx, cy) = pr(w, i).a.center();
    fx::puff_rng(w, cx, cy);
    set_anim_id(w, i, 0);
}

// ---- Majin Buu's candy (beam 0x080287AD, candy states 0x0804BE60 / 0x0804B7AC / 0x0804C2CC, eat 0x0802AF11) -------------
/// the beam's accepted hit 0x08028862: SetDef(0x36), a candy idle state (SetAnimRaw(0) + SetState), frame rand(3),
/// candy timer 0x03002FE4 = 300
pub fn candy_hit(w: &mut World, i: usize) {
    if pr(w, i).dead {
        return;
    }
    end_skill(w, i);
    {
        let pl = p(w, i);
        pl.candy_def = true;
        pl.ap.raw = 1;
    }
    enter_state(w, i, Gs::Candy);
    let f = w.rng.rand(3);
    p(w, i).ap.frame = f;
    w.g.candy = 300;
}
/// eat controller t 0: SetDef(0x36), the candy-eat state (enter 0x0804EAD5) placed at (x, y) (24.8)
pub fn candy_eat(w: &mut World, i: usize, x: i32, y: i32) {
    {
        let pl = p(w, i);
        pl.candy_def = true;
        pl.ap.raw = 1;
    }
    enter_state(w, i, Gs::CandyEat);
    let pl = p(w, i);
    pl.a.z = 0;
    pl.a.x = x;
    pl.a.y = y;
}
/// eat t 8: the player object leaves the world (0x08011944); the candy-eat state's destructor clears the candy timer
pub fn candy_gone(w: &mut World, i: usize) {
    p(w, i).gone = true;
    w.g.candy = 0;
}
/// eat t 120: a new player object at (x, y): facing down, the form's def, a new idle state (the constructor's idle
/// tick and SetAnim(0), then the eat's own idle state: three rand(255)); the KO flag -> KO (INFERRED)
pub fn candy_respawn(w: &mut World, i: usize, x: i32, y: i32, ko: bool) {
    {
        let pl = p(w, i);
        pl.gone = false;
        pl.candy_def = false;
        pl.a.x = x;
        pl.a.y = y;
        pl.a.z = 0;
        pl.a.facing = 0;
        pl.a.flash = 0;
    }
    w.rng.rand(255);
    w.rng.rand(255);
    p(w, i).ap.raw = 1;
    enter_state(w, i, Gs::Idle);
    if ko {
        p(w, i).rec.hp = 0;
        after_hp_change(w, i);
    }
}
/// candy idle control 0x08028390: timer 0 -> the form's def, SetAnim(0); a held d-pad key -> facing (left 2, right 3,
/// up 1, down 0; the later one wins), a new candy walk state
fn ctl_candy(w: &mut World, i: usize) {
    let held = inp(w, i).held;
    if w.g.candy == 0 {
        p(w, i).candy_def = false;
        return set_anim_id(w, i, 0);
    }
    if held & DPAD != 0 {
        let pl = p(w, i);
        if held & btn::LEFT != 0 {
            pl.a.facing = 2;
        }
        if held & btn::RIGHT != 0 {
            pl.a.facing = 3;
        }
        if held & btn::UP != 0 {
            pl.a.facing = 1;
        }
        if held & btn::DOWN != 0 {
            pl.a.facing = 0;
        }
        pl.ap.raw = 1;
        enter_state(w, i, Gs::CandyWalk);
    }
}
/// candy walk control 0x080284C0: at t 0 the timer 0 -> the form's def, SetAnim(0); no d-pad -> a new candy idle
/// state; else a walk-speed step (left / right, flags 2, then up / down, flags 1), its move code kept; t 8..120 repeat
/// the kept code; then t = (t + 8) & 0x7F, timer > 0 -> - 1. The facing never changes here
fn ctl_candy_walk(w: &mut World, i: usize) {
    let held = inp(w, i).held;
    let sp = pr(w, i).walk_speed();
    if pr(w, i).ap.tick == 0 {
        if w.g.candy == 0 {
            let pl = p(w, i);
            pl.candy_def = false;
            pl.a.z = 0;
            return set_anim_id(w, i, 0);
        }
        if held & DPAD == 0 {
            p(w, i).ap.raw = 1;
            enter_state(w, i, Gs::Candy);
        } else {
            let mut code = 0;
            if held & btn::LEFT != 0 {
                walk_try(w, i, -sp, 0, 2);
                code = 1;
            } else if held & btn::RIGHT != 0 {
                walk_try(w, i, sp, 0, 2);
                code = 2;
            }
            if held & btn::UP != 0 {
                walk_try(w, i, 0, -sp, 1);
                code += 3;
            } else if held & btn::DOWN != 0 {
                walk_try(w, i, 0, sp, 1);
                code += 6;
            }
            p(w, i).candy_code = code;
        }
    } else {
        let c = pr(w, i).candy_code;
        if matches!(c, 1 | 4 | 7) {
            walk_try(w, i, -sp, 0, 2);
        }
        if matches!(c, 2 | 5 | 8) {
            walk_try(w, i, sp, 0, 2);
        }
        if matches!(c, 3..=5) {
            walk_try(w, i, 0, -sp, 1);
        }
        if matches!(c, 6..=8) {
            walk_try(w, i, 0, sp, 1);
        }
    }
    let pl = p(w, i);
    pl.ap.tick = (pl.ap.tick + 8) & 0x7f;
    if w.g.candy > 0 {
        w.g.candy -= 1;
    }
}
/// mummy bandages: every live bandage's update runs 0x0802D8E6 - the form controller's stat reset (0x0800E074:
/// resists 128, so the hurt state's i-frames end) and walk / dash -48 per stuck bandage
pub fn wrap_stats(w: &mut World, i: usize, stuck: i32) {
    p(w, i).resist = 128;
    refresh(w, i);
    p(w, i).wraps = stuck;
}
pub fn release(w: &mut World, i: usize) {
    if pr(w, i).state != PState::Held {
        return;
    }
    if pr(w, i).hold_kind == Some(HoldKind::Stone) {
        let (cx, cy) = pr(w, i).a.center();
        fx::puff_rng(w, cx, cy);
    }
    {
        let pl = p(w, i);
        pl.held_by = 0;
        pl.hold_kind = None;
        pl.a.alpha = 256;
    }
    set_anim_id(w, i, 0);
}
pub fn lose_hp(w: &mut World, i: usize, n: i32, floor: i32) -> i32 {
    if pr(w, i).dead || n <= 0 {
        return 0;
    }
    let d = n.min(0.max(pr(w, i).rec.hp - floor));
    p(w, i).rec.hp -= d;
    w.stats.damage_taken += d as i64;
    after_hp_change(w, i);
    d
}
pub fn lose_ep(w: &mut World, i: usize, n: i32) {
    let pl = p(w, i);
    pl.rec.ep = 0.max(pl.rec.ep - n);
}
pub fn gain_xp(w: &mut World, i: usize, exp: i64) {
    if !w.opts.xp || exp == 0 {
        return;
    }
    let g = xp_gain(exp, pr(w, i).eff.xp_mult as i64);
    p(w, i).rec.xp += g;
    let nl = level_of(pr(w, i).rec.xp, &w.data.xp_table);
    if nl > pr(w, i).rec.level {
        let old = pr(w, i).rec.level;
        let ch = pr(w, i).form_ref(w).char_;
        let growth = w.data.chars.iter().find(|c| c.index == ch).map(|c| c.growth);
        if let Some(g) = growth {
            let mut rec = pr(w, i).rec.clone();
            level_up(&mut rec, &g, old, nl, &mut w.rng);
            p(w, i).rec = rec;
        }
        p(w, i).rec.level = nl;
        refresh(w, i);
        // the level-up hook #3 plays the jingle (sound 36) and shows the popups: with a script host attached it runs
        // there; without one the jingle alone stands in
        if w.field.as_ref().is_some_and(|f| f.host) {
            let entity = w.ents[i].id;
            w.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run {
                run: crate::fieldcontent::Callback::Event("s003".into()),
                cause: crate::field::Cause::LevelUp { entity },
                slot: pr(w, i).slot,
            }));
        } else {
            sfx(w, 36, 64);
        }
    }
}

/// `give_xp` (0xC2): the kill-XP routine without the hook (the script native runs hook #3 inline itself); true when
/// a level was gained
pub fn gain_xp_levels(w: &mut World, i: usize, exp: i64) -> bool {
    let before = pr(w, i).rec.level;
    let host = w.field.as_ref().map(|f| f.host);
    if let Some(f) = w.field.as_mut() {
        f.host = false;
    }
    let n = w.events.len();
    gain_xp(w, i, exp);
    if let (Some(f), Some(h)) = (w.field.as_mut(), host) {
        f.host = h;
    }
    let up = pr(w, i).rec.level > before;
    if up {
        // the jingle comes from the hook
        if let Some(k) = (n..w.events.len()).rev().find(|&k| matches!(w.events[k], crate::world::Event::Sfx { id: 36, .. })) {
            w.events.remove(k);
        }
    }
    up
}

// ---- update -----------------------------------------------------------------------------------------------------------

pub fn update(w: &mut World, i: usize) {
    // the world map's flying player (fieldworld.rs) replaces the 2D player's states
    if crate::fieldworld::flyer_update(w, i) {
        return;
    }
    if pr(w, i).it_sel.is_some() {
        it_select(w, i);
        let mut pl = pr(w, i).clone();
        pl.sync_anim(w);
        *p(w, i) = pl;
        return;
    }
    // eaten (out of the world): nothing of the player runs
    if pr(w, i).gone {
        return;
    }
    {
        let pl = p(w, i);
        if pl.a.flash > 0 {
            pl.a.flash -= 1;
        }
        pl.t += 1;
    }
    shield_tick(w, i);
    let be = pr(w, i).beam_end.clone();
    let mut keep = Vec::new();
    for (id, list) in be {
        if w.is_removed(id) {
            for (stop, sid) in list {
                if stop {
                    sfx_stop(w, sid);
                } else {
                    sfx(w, sid, 64);
                }
            }
        } else {
            keep.push((id, list));
        }
    }
    p(w, i).beam_end = keep;
    let frame = w.frame;
    let gr = pr(w, i).ghost_rng.clone();
    for &(at, n) in &gr {
        if at == frame {
            w.rng.rand(n);
        }
    }
    p(w, i).ghost_rng = gr.into_iter().filter(|g| g.0 > frame).collect();
    // the death script (KO + 199) blocks the player update for a frame; the story forms' controllers run at HP 0
    let ko = pr(w, i).ko_at;
    let since_ko = if ko >= 0 { w.frame as i64 - ko } else { -1 };
    if since_ko == 199 {
        // the death script's loop runs from here on (the original's one player): no other object is updated any
        // more, and every actor gets message 0xF (0x0800F44A: the walk state's vfunc +0x10 SetAnim(0)s)
        if w.players.len() == 1 {
            w.modal = Some(vec![w.ents[i].id]);
            for id in w.enemies.clone() {
                if let Some(k) = w.find(id).filter(|&k| !w.ents[k].removed) {
                    crate::enemy::stop_msg(w, k);
                }
            }
        }
        let mut pl = pr(w, i).clone();
        pl.sync_anim(w);
        *p(w, i) = pl;
        return;
    }
    if pr(w, i).defuse.is_some() {
        return defuse_step(w, i);
    }
    if since_ko < 199 && (!pr(w, i).dead || pr(w, i).story_form(w)) {
        passive(w, i);
    }
    if pr(w, i).defuse.is_some() {
        let mut pl = pr(w, i).clone();
        pl.sync_anim(w);
        *p(w, i) = pl;
        return;
    }
    anim_tick(w, i);
    // a script commanding the player (scene.rs puppet): the states it sets run, the controller does not
    let puppet = crate::scene::is_puppet(w, w.ents[i].id);
    if puppet {
        crate::scene::player_frames_tick(w, i);
    }
    // a trigger script's nested loop: the state's control does not run (field::hold_nested)
    let nested = w.field.as_ref().is_some_and(|f| f.nested);
    if !pr(w, i).dead && !puppet && !nested {
        control(w, i);
    }
    let mut pl = pr(w, i).clone();
    pl.sync_anim(w);
    *p(w, i) = pl;
    let ko = pr(w, i).ko_at;
    if ko >= 0 && w.frame as i64 - ko == 200 {
        w.events.push(Event::Song { id: 8 });
    }
    // the task list (screen shakes) runs after the objects; the first player is the last object updated
    // (field: once after the whole pass, World::step_entities_list - the player can be updated twice in a frame)
    if pr(w, i).slot == 0 && w.field.is_none() {
        w.tick_shakes();
    }
}

fn tick_once(a: &mut AnimPos, n: i32, shift: i32) -> bool {
    a.tick = (a.tick + 1) & 0xffff;
    let f = a.tick >> shift;
    if f >= n {
        return true;
    }
    a.frame = f;
    false
}
fn tick_loop(a: &mut AnimPos, n: i32, shift: i32) {
    a.tick = (a.tick + 1) & 0xffff;
    let mut f = a.tick >> shift;
    if f >= n {
        a.tick = 0;
        f = 0;
    }
    a.frame = f;
}
fn tick_idle(a: &mut AnimPos, n: i32) {
    a.tick = (a.tick + 1) & 0xffff;
    if n == 2 {
        a.frame = if (a.tick & 0xff) > 0xf0 { 1 } else { 0 };
        return;
    }
    a.frame = (a.tick >> 3) & 0xff;
    if a.frame >= n {
        a.frame = 0;
        a.tick = 0;
    }
}
fn tick_bored(a: &mut AnimPos, n: i32) {
    a.tick = (a.tick + 1) & 0xffff;
    let f = a.tick >> 5;
    if f >= n {
        a.tick = 0x40;
        a.frame = 2;
        return;
    }
    a.frame = f;
}

/// the state's tick (vtable +4)
fn anim_tick(w: &mut World, i: usize) {
    let n = pr(w, i).anim_n(w);
    let gs = pr(w, i).gs;
    let mut a = pr(w, i).ap;
    let end = |w: &mut World, a: AnimPos| {
        p(w, i).ap = a;
    };
    match gs {
        Gs::Idle | Gs::Idle2 => {
            tick_idle(&mut a, n);
            end(w, a)
        }
        Gs::Walk | Gs::LoopB | Gs::LoopC | Gs::AutoWalk => {
            tick_loop(&mut a, n, 3);
            end(w, a)
        }
        // the stone / candy idle / eat ticks do nothing; the hop tick 0x080284AB: height = -sin(t) >> 13
        Gs::Stone | Gs::Candy | Gs::CandyEat => {}
        Gs::CandyWalk => p(w, i).a.z = -(crate::tables::SIN256[(a.tick & 0xff) as usize] >> 13),
        Gs::Dash => {
            tick_loop(&mut a, n, 3);
            end(w, a);
            if a.tick & 15 == 0 {
                sfx(w, 25, 32);
            }
        }
        Gs::Bored => {
            tick_bored(&mut a, n);
            end(w, a)
        }
        Gs::Punch => {
            let done = tick_once(&mut a, n, 2);
            end(w, a);
            if done {
                set_anim_id(w, i, 0);
            }
        }
        Gs::Once3 | Gs::Hurt => {
            let done = tick_once(&mut a, n, 3);
            end(w, a);
            if done {
                set_anim_id(w, i, 0);
            }
        }
        Gs::Once4 => {
            let done = tick_once(&mut a, n, 4);
            end(w, a);
            if done {
                set_anim_id(w, i, 0);
            }
        }
        Gs::Ko | Gs::OnceHold => {
            let done = tick_once(&mut a, n, 5);
            end(w, a);
            if done {
                enter_state(w, i, Gs::Hold);
            }
        }
        Gs::Hold | Gs::Hold0 | Gs::Block => {}
        Gs::Skill(20) | Gs::SsUp => {
            let done = tick_once(&mut a, n, 4);
            end(w, a);
            if done {
                finish_transform(w, i);
            }
        }
        Gs::SsRevert => {
            let done = tick_once(&mut a, n, 4);
            end(w, a);
            if done {
                finish_revert(w, i);
            }
        }
        Gs::Skill(18) | Gs::Skill18r => {
            let done = tick_once(&mut a, n, 2);
            end(w, a);
            if done {
                finish_saiyaman(w, i);
            }
        }
        Gs::Skill(14) => {}
        // a scripted mod skill: its animation loops (or plays once and holds) at the skill's rate
        Gs::Skill(crate::modkit::SCRIPT_GS) => {
            let (looping, shift) = crate::modkit::anim_mode(w, i);
            if looping {
                tick_loop(&mut a, n, shift);
            } else if tick_once(&mut a, n, shift) {
                a.frame = (n - 1).max(0);
            }
            end(w, a)
        }
        Gs::Skill(7) => tick_blast(w, i),
        Gs::Skill(2) => {
            let pl = p(w, i);
            pl.ap.tick = (pl.ap.tick + 1) & 0xffff;
            let t = pl.ap.tick;
            let proj = pl.sk.as_ref().map(|s| s.proj).unwrap_or(0);
            if proj == 0 {
                if t > 16 {
                    set_anim_id(w, i, 0);
                }
            } else {
                p(w, i).ap.frame = (t >> 3) & 1;
            }
        }
        _ => {}
    }
}
/// Energy Blast tick 0x0801AF3C
fn tick_blast(w: &mut World, i: usize) {
    let t = {
        let pl = p(w, i);
        pl.ap.tick = (pl.ap.tick + 1) & 0xffff;
        pl.ap.tick
    };
    // a modded skill on the Energy Blast behaviour may move the shot frame / the refire window
    let real = pr(w, i).sk.as_ref().map(|s| s.id).unwrap_or(7);
    let eng = w.data.skill_engine(real);
    let (fire, recovery) = (eng.and_then(|e| e.fire_frame).unwrap_or(16), eng.and_then(|e| e.recovery).unwrap_or(40));
    if t < recovery {
        if t == fire {
            p(w, i).ap.frame = 1;
            fire_blast(w, i);
        }
        return;
    }
    let cost = pr(w, i).skill_cost(w, real);
    let refire = pr(w, i).sk.as_ref().is_some_and(|s| s.refire);
    if refire && pr(w, i).can_afford(w, cost) {
        if let Some(s) = p(w, i).sk.as_mut() {
            s.refire = false;
        }
        spend(w, i, cost, false);
        let pl = p(w, i);
        pl.ap.frame = 3 - pl.ap.frame;
        pl.ap.tick = fire;
        fire_blast(w, i);
    } else {
        set_anim_id(w, i, 0);
    }
}
fn fire_blast(w: &mut World, i: usize) {
    let (mx, my) = pr(w, i).hand_point();
    let dir = pr(w, i).a.facing;
    let me = w.ents[i].id;
    let real = pr(w, i).sk.as_ref().map(|s| s.id).unwrap_or(7);
    let (a, mult) = if w.data.skill_engine(real).is_none() { (atk(4), 1024) } else { (atk(skill_sub_of(w, real)), skill_mult_of(w, real, pr(w, i).skill_lv(w, real))) };
    let eng = w.data.skill_engine(real).cloned();
    let id = shots::ki_blast(w, me, dir, 1, mx << 8, my << 8, a, mult, true);
    // the player's blast (vtable 0x0804B454) tests a 16 x 16 box centred on its position
    if let Some(k) = w.find(id) {
        if let ShotKind::Straight(o) = &mut w.shot(k).kind {
            o.b = [8, 8, 16, 16];
            if let Some(e) = &eng {
                if let Some((bw, bh)) = e.hit_box {
                    o.b = [bw / 2, bh / 2, bw, bh];
                }
                if let Some(v) = e.speed {
                    let (dx, dy) = DIR_VEC[dir as usize];
                    o.vx = dx * v;
                    o.vy = dy * v;
                }
            }
        }
    }
    p(w, i).blast = id;
}

/// the story form object: countdown - 1, once negative SetAnim(23) (form 24 -> 25) / SetAnim(24) (25 -> 24) every
/// frame the player is not already in that state (0x08010D26)
fn story_tick(w: &mut World, i: usize) {
    p(w, i).story_cnt -= 1;
    if pr(w, i).story_cnt >= 0 {
        return;
    }
    let f = pr(w, i).form_ref(w).clone();
    if f.controller == 0x3001b88 {
        if pr(w, i).gs != Gs::SsUp {
            p(w, i).transform_to = f.up;
            set_anim_id(w, i, 23);
        }
    } else if pr(w, i).gs != Gs::SsRevert {
        set_anim_id(w, i, 24);
    }
}
/// the new form object after a transformation / revert (0x0801064C): form 24's countdown 120 + rand(180), form 25's
/// 240 + rand(120), the fusion's 300
fn new_form_object(w: &mut World, i: usize) {
    let c = pr(w, i).form_ref(w).controller;
    if c == 0x3001b88 {
        let r = w.rng.rand(180);
        p(w, i).story_cnt = 120 + r;
    } else if c == 0x3001b8c {
        let r = w.rng.rand(120);
        p(w, i).story_cnt = 240 + r;
    } else if c == 0x3001b64 {
        p(w, i).fus_cnt = 300;
    }
    let pl = p(w, i);
    if pl.dead && pl.rec.hp == 0 {
        pl.dead = false;
    }
}
/// the cutscene fusions' defuse (script s014, run by the form object when its countdown goes negative)
fn defuse_step(w: &mut World, i: usize) {
    let (t, x0) = {
        let d = p(w, i).defuse.as_mut().unwrap();
        d.0 += 1;
        *d
    };
    if t == 85 {
        swap_to_partner(w, i);
        w.rng.rand(255);
    } else if !(90..=94).contains(&t) {
        anim_tick(w, i);
    }
    if (95..=174).contains(&t) {
        p(w, i).a.x -= 50;
    }
    if t == 175 {
        p(w, i).a.x = (x0 - 16) << 8;
    }
    if t == 295 {
        w.rng.rand(255);
    }
    if t >= 297 {
        p(w, i).defuse = None;
        if !pr(w, i).dead {
            control(w, i);
        }
    }
    let mut pl = pr(w, i).clone();
    pl.sync_anim(w);
    *p(w, i) = pl;
}
fn swap_to_partner(w: &mut World, i: usize) {
    form_exit(w, i);
    let ch = pr(w, i).form_ref(w).char_;
    let pr0 = pr(w, i).clone();
    let partner = pr0.partner.clone().unwrap_or(Partner {
        form: if ch == 7 { 2 } else { 21 },
        hp: pr0.rec.hp,
        max_hp: pr0.rec.max_hp,
        ep: pr0.rec.ep,
        max_ep: pr0.rec.max_ep,
        base: [pr0.rec.str_, pr0.rec.pow, pr0.rec.end, pr0.rec.spd],
        skills: pr0.skills.clone(),
        slot: 0,
        level0: 0,
        ss_gauge: 0,
        ss3_gauge: 0,
        cooldown: 0,
    });
    {
        let pl = p(w, i);
        pl.rec.hp = partner.hp;
        pl.rec.max_hp = partner.max_hp;
        pl.rec.ep = partner.ep;
        pl.rec.max_ep = partner.max_ep;
        pl.rec.str_ = partner.base[0];
        pl.rec.pow = partner.base[1];
        pl.rec.end = partner.base[2];
        pl.rec.spd = partner.base[3];
        pl.skills = if partner.skills.is_empty() { vec![7] } else { partner.skills.clone() };
        pl.sel = partner.slot;
        pl.skill_level = partner.level0;
        pl.ss_gauge = partner.ss_gauge;
        pl.ss3_gauge = partner.ss3_gauge * 65536;
        pl.ss_cooldown = partner.cooldown;
        pl.regen_timer = 0;
    }
    apply_form(w, i, partner.form);
    form_enter(w, i);
    p(w, i).ap.raw = ANIM_RAW[0];
    let gs = pr(w, i).idle_state(w);
    enter_state(w, i, gs);
}

/// the state's control (vtable +8)
fn control(w: &mut World, i: usize) {
    if pr(w, i).gs == Gs::Stone {
        return ctl_stone(w, i);
    }
    if pr(w, i).state == PState::Held {
        return upd_held(w, i);
    }
    match pr(w, i).gs {
        Gs::Candy => return ctl_candy(w, i),
        Gs::CandyWalk => return ctl_candy_walk(w, i),
        Gs::CandyEat => return,
        Gs::Idle => return ctl_idle(w, i),
        Gs::Walk => return ctl_walk(w, i),
        Gs::Dash => return ctl_dash(w, i),
        Gs::Bored => return ctl_bored(w, i),
        Gs::Block => return ctl_block(w, i),
        Gs::Hurt => return ctl_knock(w, i),
        Gs::Skill(14) => return ctl_it(w, i),
        Gs::Skill(18) | Gs::Skill18r => return,
        Gs::AutoWalk => {
            // +0x80 subpx per frame along the facing (0x083C34B0), no collision, input ignored
            let pl = p(w, i);
            let (dx, dy) = crate::geom::DIR_VEC[pl.a.facing as usize & 3];
            pl.a.x += dx * 0x80;
            pl.a.y += dy * 0x80;
            return;
        }
        _ => {}
    }
    match pr(w, i).state {
        PState::Skill => upd_skill(w, i),
        PState::Transform => upd_transform(w, i),
        _ => {}
    }
}

/// footprint inside the arena, on the walk grid and clear of rocks
pub fn fits(w: &World, i: usize, x: i32, y: i32) -> bool {
    let a = &pr(w, i).a;
    let b = w.arena.bounds;
    if !(x >= b.x0 && y >= b.y0 && x + a.foot_w <= b.x1 && y + a.foot_h <= b.y1) {
        return false;
    }
    if let Some(c) = &w.col {
        if !c.test(crate::world::Layer::Walk, x, y, x + a.foot_w, y + a.foot_h) {
            return false;
        }
    }
    for e in &w.ents {
        if let Body::Rock { x: rx, y: ry, .. } = e.body {
            if !e.removed {
                let r = Rect::new(rx + 16, ry + 16, rx + 48, ry + 48);
                if x < r.x1 && r.x0 < x + a.foot_w && y < r.y1 && r.y0 < y + a.foot_h {
                    return false;
                }
            }
        }
    }
    true
}
fn move_by(w: &mut World, i: usize, vx: i32, vy: i32) {
    if vx != 0 {
        let (x, py) = (pr(w, i).a.x, pr(w, i).a.py());
        if fits(w, i, (x + vx) >> 8, py) {
            p(w, i).a.x += vx;
        }
    }
    if vy != 0 {
        let (px, y) = (pr(w, i).a.px(), pr(w, i).a.y);
        if fits(w, i, px, (y + vy) >> 8) {
            p(w, i).a.y += vy;
        }
    }
}
/// walking try-move 0x08012960
fn walk_try(w: &mut World, i: usize, vx: i32, vy: i32, flags: i32) {
    let (x, y) = (pr(w, i).a.x, pr(w, i).a.y);
    let nx24 = x + vx;
    let ny24 = y + vy;
    let mut nx = nx24 >> 8;
    let ny = ny24 >> 8;
    if fits(w, i, nx, ny) {
        let pl = p(w, i);
        pl.a.x = nx24;
        pl.a.y = ny24;
        return;
    }
    let mut x24 = nx24;
    if flags & 1 != 0 && ((nx & 7) < 2 || (nx & 7) > 5) {
        let sx = (nx + 4) & !7;
        if fits(w, i, sx, ny) {
            x24 = sx << 8;
            let pl = p(w, i);
            pl.a.x = x24;
            pl.a.y = ny24;
            nx = sx;
        }
    }
    if flags & 2 != 0 && ((ny & 7) < 2 || (ny & 7) > 5) {
        let sy = (ny + 4) & !7;
        if fits(w, i, nx, sy) {
            let pl = p(w, i);
            pl.a.x = x24;
            pl.a.y = sy << 8;
        }
    }
}
fn cycle_skill(w: &mut World, i: usize) {
    let pl = p(w, i);
    if !pl.skills.is_empty() {
        pl.sel = (pl.sel + 1) % pl.skills.len();
    }
    // the switch's click (story playthrough: L in the Cave of the Ancients)
    sfx(w, 40, 64);
}

fn passive(w: &mut World, i: usize) {
    if pr(w, i).story_form(w) {
        return story_tick(w, i);
    }
    {
        let pl = p(w, i);
        pl.regen_timer += 110;
        if pl.regen_timer >= 0x10000 {
            pl.regen_timer -= 0x10000;
            pl.rec.ep = pl.rec.max_ep.min(pl.rec.ep + ((pl.rec.max_ep * 0x7ae) >> 16));
        }
        if pl.eff.hp_regen256 != 0 {
            pl.hp_regen_acc += pl.eff.hp_regen256;
            if pl.hp_regen_acc >= 256 {
                let n = pl.hp_regen_acc >> 8;
                pl.hp_regen_acc &= 0xff;
                pl.rec.hp = pl.rec.max_hp.min(pl.rec.hp + n);
            }
        }
    }
    if pr(w, i).holy_left > 0 {
        p(w, i).holy_left -= 1;
        if pr(w, i).holy_left == 0 {
            refresh(w, i);
        }
    }
    if w.g.candy > 0 {
        w.g.candy -= 1;
    }
    let f = pr(w, i).form_ref(w).clone();
    let drains = [0x3001b68, 0x3001b6c, 0x3001b78, 0x3001b80].contains(&f.controller);
    if pr(w, i).gs != Gs::SsRevert && drains {
        if f.ss3 {
            let max_ep = pr(w, i).rec.max_ep;
            let pl = p(w, i);
            pl.ss_acc += 22 * max_ep + 0x3333;
            let n = (pl.ss_acc >> 16) as i64;
            if n != 0 {
                let g = pl.ss3_gauge >> 16;
                let mut rev = false;
                if g > n {
                    pl.ss3_gauge = (g - n) * 65536;
                } else if pl.rec.hp != 0 {
                    pl.ss3_gauge = 0;
                    rev = true;
                }
                pl.ss_acc &= 0xffff;
                if rev {
                    revert(w, i);
                }
            }
        } else if f.ss {
            let pl = p(w, i);
            pl.ss_acc += 0xccd;
            if pl.ss_acc >> 16 != 0 {
                let mut rev = false;
                if pl.ss_gauge != 0 {
                    pl.ss_gauge -= 1;
                } else if pl.rec.hp != 0 {
                    rev = true;
                }
                pl.ss_acc &= 0xffff;
                if rev {
                    revert(w, i);
                }
            }
        }
    }
    {
        let pl = p(w, i);
        if !f.ss && !f.ss3 && pl.ss_cooldown > 0 {
            pl.ss_cooldown = 0.max(pl.ss_cooldown - (pl.eff.ss_cooldown_rate >> 6));
        }
    }
    // the cutscene fusion's form object (0x0800E638): after the regen its countdown; negative -> the defuse script
    if f.controller == 0x3001b64 {
        p(w, i).fus_cnt -= 1;
        if pr(w, i).fus_cnt < 0 {
            let pl = p(w, i);
            pl.a.x = pl.a.px() << 8;
            pl.a.y = pl.a.py() << 8;
            pl.a.facing = 0;
            let x0 = pl.a.px();
            pl.defuse = Some((0, x0));
        }
    }
}

fn attack_input(w: &mut World, i: usize) {
    let pressed = inp(w, i).pressed;
    if pressed & btn::A != 0 {
        return melee(w, i);
    }
    if pressed & btn::B != 0 {
        start_skill(w, i);
    }
}
fn try_block(w: &mut World, i: usize) -> bool {
    if inp(w, i).held & btn::R == 0 || !pr(w, i).has_anim(w, 8) {
        return false;
    }
    p(w, i).ap.raw = 8;
    enter_state(w, i, Gs::Block);
    true
}
/// idle control 0x08013022
fn ctl_idle(w: &mut World, i: usize) {
    let InputFrame { held, pressed, .. } = inp(w, i);
    if w.g.candy > 0 {
        return;
    }
    if pressed & btn::L != 0 {
        cycle_skill(w, i);
    }
    if try_block(w, i) {
        return;
    }
    // the boredom counter does not run while an event holds the player (story playthrough: Grand Kai's
    // Mansion s068, 1100+ frames of dialog with Goku idle)
    if w.scene.depth == 0 {
        p(w, i).s_cnt += 1;
    }
    if pr(w, i).s_cnt > 1024 {
        if pr(w, i).has_anim(w, 9) {
            set_anim_id(w, i, 8);
        } else {
            p(w, i).s_cnt = 0;
        }
    }
    let nd = pressed & DPAD;
    if nd != 0 {
        let frame = w.frame as i64;
        let pl = p(w, i);
        if frame - pl.last_dir_frame < 20 && (nd & (nd - 1)) == 0 && pl.last_dir_key & nd != 0 {
            return set_anim_id(w, i, 2);
        }
        pl.last_dir_key = nd;
        pl.last_dir_frame = frame;
    }
    if held & DPAD != 0 {
        let pl = p(w, i);
        if pressed & btn::LEFT != 0 {
            pl.a.facing = 2;
        }
        if pressed & btn::RIGHT != 0 {
            pl.a.facing = 3;
        }
        if pressed & btn::UP != 0 {
            pl.a.facing = 1;
        }
        if pressed & btn::DOWN != 0 {
            pl.a.facing = 0;
        }
        set_anim_id(w, i, 1);
    }
    attack_input(w, i);
}
/// walk control 0x080131BA
fn ctl_walk(w: &mut World, i: usize) {
    let InputFrame { held, pressed, .. } = inp(w, i);
    if pressed & btn::L != 0 {
        cycle_skill(w, i);
    }
    if try_block(w, i) {
        return;
    }
    if held & DPAD != 0 {
        let sp = pr(w, i).walk_speed();
        let mut combo = 0;
        if held & btn::LEFT != 0 {
            walk_try(w, i, -sp, 0, 2);
            combo = 1;
        } else if held & btn::RIGHT != 0 {
            walk_try(w, i, sp, 0, 2);
            combo = 2;
        }
        if held & btn::UP != 0 {
            walk_try(w, i, 0, -sp, 1);
            combo += 3;
        } else if held & btn::DOWN != 0 {
            walk_try(w, i, 0, sp, 1);
            combo += 6;
        }
        let pl = p(w, i);
        pl.a.facing = FACE_TBL[combo][pl.a.facing as usize];
    } else {
        set_anim_id(w, i, 0);
    }
    if w.g.candy > 0 {
        return;
    }
    attack_input(w, i);
}
/// dash control 0x08012DEE
fn ctl_dash(w: &mut World, i: usize) {
    let held = inp(w, i).held;
    let key = pr(w, i).last_dir_key;
    if held & key != key {
        set_anim_id(w, i, 0);
    } else {
        let (vx, vy) = DASH_VEC[pr(w, i).a.facing as usize];
        let sp = pr(w, i).dash_speed();
        let nx = pr(w, i).a.x + vx * sp;
        let ny = pr(w, i).a.y + vy * sp;
        if !fits(w, i, nx >> 8, ny >> 8) {
            w.shake(8, 100);
            p(w, i).ap.raw = 10;
            enter_state(w, i, Gs::Hurt);
            sfx(w, 59, 64);
        } else {
            {
                let pl = p(w, i);
                pl.a.x = nx;
                pl.a.y = ny;
            }
            let ws = pr(w, i).walk_speed();
            let f = pr(w, i).a.facing;
            let (mut tx, mut ty) = (nx, ny);
            if held & btn::UP != 0 && f != 1 {
                ty -= ws;
            }
            if held & btn::DOWN != 0 && f != 0 {
                ty += ws;
            }
            if held & btn::LEFT != 0 && f != 2 {
                tx -= ws;
            }
            if held & btn::RIGHT != 0 && f != 3 {
                tx += ws;
            }
            if fits(w, i, tx >> 8, ty >> 8) {
                let pl = p(w, i);
                pl.a.x = tx;
                pl.a.y = ty;
            }
        }
    }
    attack_input(w, i);
}
fn ctl_bored(w: &mut World, i: usize) {
    if inp(w, i).held & DPAD != 0 {
        set_anim_id(w, i, 0);
    }
    attack_input(w, i);
}
/// hurt / bump control 0x0800DF1C
fn ctl_knock(w: &mut World, i: usize) {
    let (kx, ky) = KNOCK_VEC[pr(w, i).a.facing as usize];
    let (x, y) = (pr(w, i).a.x, pr(w, i).a.y);
    if fits(w, i, (x - kx) >> 8, (y - ky) >> 8) {
        let pl = p(w, i);
        pl.a.x -= kx;
        pl.a.y -= ky;
    }
}
/// block control 0x08012BC6 + the energy shield object 0x0801DFE0
fn ctl_block(w: &mut World, i: usize) {
    let InputFrame { held, pressed, .. } = inp(w, i);
    let Some((pow, _)) = pr(w, i).shield else {
        if held & btn::R == 0 {
            return set_anim_id(w, i, 0);
        }
        if pressed & btn::B != 0 && pr(w, i).energy_left(w) != 0 {
            p(w, i).shield = Some((0, true));
        }
        return;
    };
    {
        let pl = p(w, i);
        pl.s_cnt += (pl.eff.ep_block_factor * 0x3277) >> 4;
    }
    let n = pr(w, i).s_cnt >> 16;
    spend(w, i, n, true);
    p(w, i).s_cnt &= 0xffff;
    let on = pr(w, i).energy_left(w) != 0 && held & btn::R != 0 && held & btn::B != 0;
    p(w, i).shield = Some((pow, on));
    if pow == 0 && !on {
        if held & btn::R != 0 {
            p(w, i).shield = None;
        } else {
            set_anim_id(w, i, 0);
        }
        return;
    }
    set_resist(w, i, if pow >= 24 { 0 } else { 128 });
}
fn shield_tick(w: &mut World, i: usize) {
    let pl = p(w, i);
    let Some((mut pow, on)) = pl.shield else { return };
    if on {
        pow += 1;
        if pow >= 0x38 {
            pow = 0x18;
        }
    } else if pow >= 0x18 {
        pow = 0x17;
    } else if pow > 0 {
        pow -= 1;
    }
    pl.shield = Some((pow, on));
}

/// 0x08012A24: one instant hit test to every object; a hit plays 48 + rand(2); then SetAnim(4 + rand(3))
fn melee(w: &mut World, i: usize) {
    let a = &pr(w, i).a;
    let rect = Rect::at(PUNCH_RECT[a.facing as usize], a.px(), a.py());
    let me = w.ents[i].id;
    let mut m = HitMsg { team: 1, rect, dir: a.facing, mult: 0x400, atk: atk(2), attacker: me, att_stats: a.stats, att_is_player: true, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
    // field: areas without combat (area record +8 = 0) send no hit; a hit nobody accepted becomes the interact
    // message (talk, A pickups), then the A-trigger test — any of them suppresses the punch (field.rs / fieldnpc.rs)
    let combat = w.field.as_ref().is_none_or(|f| f.combat);
    let any = combat && broadcast(w, &mut m, false);
    if w.field.is_some() && !any && (crate::fieldnpc::interact(w, i) || !combat) {
        return;
    }
    if any {
        let s = 48 + w.rng.rand(2);
        sfx(w, s, 64);
        if m.crit {
            w.shake(15, 75);
        }
    }
    if pr(w, i).rec.hp == 0 {
        return;
    }
    let r = w.rng.rand(3);
    p(w, i).punch_anim = r;
    set_anim_id(w, i, 4 + r as usize);
}
/// called by an enemy when this player's attack dealt damage (melee EP / HP hooks)
pub fn on_dealt(w: &mut World, i: usize, msg: &HitMsg, dmg: i64) {
    if dmg <= 0 {
        return;
    }
    let me = w.ents[i].id;
    let pl = p(w, i);
    pl.dealt_before += 1;
    if msg.source == me && msg.atk.kind == 0 {
        if pl.eff.melee_hp_instead {
            pl.rec.hp = pl.rec.max_hp.min(pl.rec.hp + 1);
        } else {
            pl.rec.ep = pl.rec.max_ep.min(pl.rec.ep + pl.eff.melee_ep);
        }
    }
}

pub fn can_use_skill(w: &World, i: usize, id: i32) -> bool {
    let pl = pr(w, i);
    if w.g.candy > 0 {
        return false;
    }
    let real = id;
    let id = w.data.skill_kind(real);
    if let Some(c) = w.data.skill_engine(real).and_then(|e| SkillEngine::pick(&e.check, pl.skill_lv(w, real))) {
        if !pl.can_afford(w, c) {
            return false;
        }
    }
    if id == crate::modkit::SCRIPTED {
        return crate::modkit::skill_ready(w, i, real);
    }
    let f = pl.form_ref(w);
    match id {
        12 | 19 | 21 => false,
        18 => true,
        14 => pl.can_afford(w, 40),
        20 => {
            if f.up == f.down || f.ss3 {
                return false;
            }
            if f.ss {
                return true;
            }
            pl.ss_cooldown == 0 && pl.can_afford(w, 10)
        }
        22 => {
            if f.ss3 {
                return true;
            }
            f.ss && f.up != 255 && pl.can_afford(w, 10)
        }
        2 => {
            if pl.big_bang != 0 && w.alive(pl.big_bang) {
                return false;
            }
            pl.can_afford(w, [10, 33, 100][pl.skill_lv(w, real).min(2) as usize])
        }
        _ if w.data.skill_engine(real).is_some_and(|e| !e.check.is_empty()) => true,
        _ => match skill_check(id) {
            Some(c) => pl.can_afford(w, c[(pl.skill_lv(w, real) as usize).min(c.len() - 1)]),
            None => pl.can_afford(w, pl.skill_cost(w, real)),
        },
    }
}

pub fn start_skill(w: &mut World, i: usize) {
    let real = pr(w, i).skill();
    if !can_use_skill(w, i, real) {
        return;
    }
    w.events.push(Event::SkillStart { id: real });
    // `id` = the behaviour (state machine) the skill runs: its own id, or a mod skill's `engine.base`
    let id = w.data.skill_kind(real);
    let lv = pr(w, i).skill_lv(w, real);
    if id == crate::modkit::SCRIPTED {
        return crate::modkit::skill_start(w, i, real, lv);
    }
    let mult = skill_mult_of(w, real, lv);
    if [4, 5, 7, 8, 13, 17, 23].contains(&id) {
        let c = pr(w, i).skill_cost(w, real);
        spend(w, i, c, false);
    }
    if id == 3 {
        spend(w, i, 2, false);
    }
    let f = pr(w, i).form_ref(w).clone();
    if id == 20 {
        return if f.ss { revert(w, i) } else { start_transform(w, i, f.up) };
    }
    if id == 22 {
        return if f.ss3 { revert(w, i) } else { start_transform(w, i, f.up) };
    }
    if id == 18 {
        p(w, i).ap.raw = if f.up != 255 { 20 } else { 12 };
        enter_state(w, i, if f.up != 255 { Gs::Skill(18) } else { Gs::Skill18r });
        return;
    }
    if id == 14 {
        return start_it(w, i, lv);
    }
    enter_state(w, i, Gs::Skill(id as u8));
    let serial = w.next_serial();
    {
        let pl = p(w, i);
        pl.state = PState::Skill;
        pl.t = 0;
        pl.sk = Some(SkillInst { id: real, serial, held: true, release: -1, ..Default::default() });
    }
    if let Some(raw) = skill_raw(id) {
        p(w, i).ap.raw = raw;
        let mut c = pr(w, i).clone();
        c.sync_anim(w);
        c.ep_acc = 0;
        *p(w, i) = c;
        skill_enter(w, i, id, lv, mult);
        return;
    }
    if let Some(e) = w.data.skill_engine(real).filter(|e| !e.anim.is_empty()) {
        let names: Vec<&'static str> = e.anim.clone();
        if !names.is_empty() {
            let refs: Vec<AnimRef> = names.into_iter().map(AnimRef::Name).collect();
            set_anim_named(w, i, &refs, true);
            return;
        }
    }
    let refs: &[AnimRef] = match id {
        7 | 4 | 23 => &[AnimRef::Name("kiblast")],
        2 | 17 => &[AnimRef::Name("special_charge"), AnimRef::Name("kiblast")],
        9 => &[AnimRef::Name("special_charge"), AnimRef::Name("attack1")],
        8 => &[AnimRef::Name("attack2"), AnimRef::Name("attack1")],
        5 => &[AnimRef::Name("special_charge"), AnimRef::Name("dash")],
        13 => &[AnimRef::Name("special_charge"), AnimRef::Name("meditate")],
        14 => &[AnimRef::Name("meditate"), AnimRef::Name("special_charge")],
        _ => &[AnimRef::Name("special_beam"), AnimRef::Name("kiblast")],
    };
    set_anim_named(w, i, refs, true);
}

fn sk_mut(w: &mut World, i: usize) -> Option<&mut SkillInst> {
    p(w, i).sk.as_mut()
}

/// what each skill state's enter creates (the objects' constructors play the charge sound 3)
fn skill_enter(w: &mut World, i: usize, id: i32, lv: i32, mult: i32) {
    let me = w.ents[i].id;
    let a = atk(skill_sub_of(w, pr(w, i).sk.as_ref().map(|s| s.id).unwrap_or(id)));
    let facing = pr(w, i).a.facing;
    let serial = pr(w, i).sk.as_ref().map(|s| s.serial).unwrap_or(0);
    match id {
        15 | 16 | 6 | 1 | 3 | 10 | 11 => sfx(w, 3, 64),
        2 => {
            let (bx, by) = shots::BIGBANG_POS[facing as usize];
            let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
            let bid = shots::add(w, shots::big_bang(me, facing, (px + bx) << 8, (py + by) << 8, lv));
            p(w, i).big_bang = bid;
            if let Some(s) = sk_mut(w, i) {
                s.proj = bid;
            }
            sfx(w, 3, 64);
        }
        17 => {
            let id2 = shots::add(w, shots::masenko(me, facing, mult, lv));
            if let Some(s) = sk_mut(w, i) {
                s.proj = id2;
            }
            sfx(w, 3, 64);
        }
        5 => sfx(w, 24, 64),
        8 => sfx(w, 47, 64),
        9 => {
            let f = fx::add(w, Fx::EnergySword { owner: me, slash: -1, t: 0, alive: Alive::Skill { p: me, serial } });
            if let Some(s) = sk_mut(w, i) {
                s.fx = f;
            }
            sfx(w, 3, 64);
        }
        4 => {
            let (hx, hy) = BURNING_HOLD[facing as usize];
            let (dx, dy) = DIR_VEC[facing as usize];
            let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
            let o = StraightOpts { vx: dx * 768, vy: dy * 768, b: [7, 7, 14, 14], a, mult, name: shots::SName::Burning, delay: 1 << 30, fx_end: 16, rng_hit: 2, sfx_hit: 48, ..Default::default() };
            let sid = shots::add_straight(w, me, facing, 1, (px + hx) << 8, (py + hy) << 8, o);
            if let Some(s) = sk_mut(w, i) {
                s.proj = sid;
            }
        }
        _ => {}
    }
}

fn end_skill(w: &mut World, i: usize) {
    let Some(s) = pr(w, i).sk.clone() else { return };
    let shot_kind = |w: &World, id: EntityId| w.find(id).and_then(|k| match &w.ents[k].body {
        Body::Shot(s) => Some((k, s.kind.clone())),
        _ => None,
    });
    match w.data.skill_kind(s.id) {
        crate::modkit::SCRIPTED => crate::modkit::skill_end(w, i),
        15 | 16 | 6 | 1 | 3 => {
            if s.proj == 0 {
                sfx_stop(w, 3);
            }
        }
        2 => {
            if let Some((_, ShotKind::BigBang(b))) = shot_kind(w, s.proj) {
                if !b.launched {
                    sfx_stop(w, 3);
                }
            }
        }
        17 => {
            if let Some((_, ShotKind::MasenkoCursor(c))) = shot_kind(w, s.proj) {
                if c.released < 0 {
                    sfx_stop(w, 3);
                }
            }
        }
        5 => {
            // 0x0801AB96: a dragon still out is cancelled: stop 32, every segment explodes at once (head first)
            if s.fx != 0 && w.alive(s.fx) {
                sfx_stop(w, 32);
                if let Some(k) = w.find(s.fx) {
                    fx::dragon_explode(w, k, true);
                }
                if let Some(x) = sk_mut(w, i) {
                    x.fx = 0;
                }
            }
        }
        9 => sfx_stop(w, if s.charged { 32 } else { 3 }),
        10 => {
            if s.proj != 0 && w.alive(s.proj) {
                if let Some(k) = w.find(s.proj) {
                    w.ents[k].removed = true;
                }
                p(w, i).beam_end.retain(|b| b.0 != s.proj);
                sfx_stop(w, 32);
                if let Some(x) = sk_mut(w, i) {
                    x.proj = 0;
                }
            } else {
                sfx_stop(w, 3);
            }
        }
        11 => {
            if s.fx != 0 {
                sfx_stop(w, 3);
            }
            if s.proj != 0 && w.alive(s.proj) {
                if let Some(k) = w.find(s.proj) {
                    w.ents[k].removed = true;
                }
                sfx_stop(w, 32);
                if let Some(x) = sk_mut(w, i) {
                    x.proj = 0;
                }
            }
        }
        13 => {
            let committed = s.charge;
            for (k, g) in s.ghosts.iter().enumerate() {
                if let Some(gi) = w.find(*g) {
                    if (k as i32) < committed {
                        if let ShotKind::Ghost(gh) = &mut w.shot(gi).kind {
                            gh.free = true;
                            gh.wait = 0;
                        }
                    } else {
                        w.ents[gi].removed = true;
                        w.rng.rand(4096);
                    }
                } else if (k as i32) >= committed {
                    w.rng.rand(4096);
                }
            }
            if let Some(x) = sk_mut(w, i) {
                x.ghosts.clear();
            }
        }
        _ => {}
    }
    w.events.push(Event::SkillEnd);
    let s = pr(w, i).sk.clone().unwrap_or_default();
    if s.proj != 0 && w.alive(s.proj) {
        if let Some((k, kind)) = shot_kind(w, s.proj) {
            match kind {
                ShotKind::GrowBeam(_) | ShotKind::Kame(_) => shots::beam_stop(w, k),
                ShotKind::BigBang(b) if !b.launched => w.ents[k].removed = true,
                ShotKind::MasenkoCursor(c) if c.released < 0 => w.ents[k].removed = true,
                ShotKind::Straight(o) if o.delay != 0 => w.ents[k].removed = true,
                _ => {}
            }
        }
    }
    for g in &s.ghosts {
        if let Some(gi) = w.find(*g) {
            if let ShotKind::Ghost(gh) = &w.shot_ref(gi).kind {
                if !gh.free {
                    w.ents[gi].removed = true;
                }
            }
        }
    }
    if s.fx != 0 {
        if let Some(k) = w.find(s.fx) {
            if !matches!(&w.ents[k].body, Body::Fx(f) if matches!(f.as_ref(), Fx::DragonFist { .. })) {
                w.ents[k].removed = true;
            }
        }
    }
    p(w, i).sk = None;
}

fn upd_skill(w: &mut World, i: usize) {
    if pr(w, i).sk.is_none() {
        return set_state_idle(w, i);
    }
    let held_now = inp(w, i).held & btn::B != 0;
    {
        let s = sk_mut(w, i).unwrap();
        s.t += 1;
        if s.held && !held_now {
            s.held = false;
            s.release = s.t;
        }
    }
    let s = pr(w, i).sk.clone().unwrap();
    let real = s.id;
    let id = w.data.skill_kind(real);
    if id == crate::modkit::SCRIPTED {
        return crate::modkit::skill_update(w, i, held_now);
    }
    let lv = pr(w, i).skill_lv(w, real);
    let mult = skill_mult_of(w, real, lv);
    let a = atk(skill_sub_of(w, real));
    let held = held_now;
    let me = w.ents[i].id;
    let facing = pr(w, i).a.facing;
    let tick_up = |w: &mut World| -> i32 {
        let pl = p(w, i);
        pl.ap.tick = (pl.ap.tick + 1) & 0xffff;
        pl.ap.tick
    };
    match id {
        7 => {
            if inp(w, i).pressed & btn::B != 0 {
                sk_mut(w, i).unwrap().refire = true;
            }
        }
        15 | 16 | 6 | 1 | 3 => {
            let t = tick_up(w);
            if t == 1 && s.fx == 0 {
                let off = if id == 1 { BEAM_CHARGE_BANSHEE } else { BEAM_CHARGE_KAME };
                let f = fx::beam_charge(w, me, off, fx::BeamSet::Other);
                sk_mut(w, i).unwrap().fx = f;
            }
            if t < 24 {
                return;
            }
            if t == 24 {
                if !held {
                    return set_state_idle(w, i);
                }
                let fxid = pr(w, i).sk.as_ref().unwrap().fx;
                if fxid != 0 {
                    if let Some(k) = w.find(fxid) {
                        w.ents[k].removed = true;
                    }
                    sk_mut(w, i).unwrap().fx = 0;
                }
                sfx_stop(w, 3);
                sfx(w, 32, 64);
                // the beam object (KameBeam) fires at the fire position; it was built at the press (tick 1)
                let vis = if id == 1 {
                    shots::BEAM_POS_BANSHEE_FIRE
                } else if id == 3 {
                    shots::BEAM_POS_BBK_FIRE
                } else {
                    shots::BEAM_POS_KAME_FIRE
                };
                let (fx0, fy0) = vis[facing as usize];
                let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                let bid = shots::add(w, shots::kame_beam(me, facing, px + fx0, py + fy0, w.frame as i64 - 23, a, mult, true));
                sk_mut(w, i).unwrap().proj = bid;
                p(w, i).beam_end.push((bid, vec![(true, 32), (false, 33)]));
                if id == 3 {
                    let (fx0, fy0) = BBK_FLARE_POS[facing as usize];
                    let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                    fx::add(w, Fx::BbkFlare { x: px + fx0, y: py + fy0, dir: facing, t: 0, ended: false, alive: Alive::Beam { id: bid } });
                }
            }
            let d = if id == 1 || id == 3 { 1174 } else if id == 6 { [128, 427, 1707][lv as usize] } else { [107, 384, 1280][lv as usize] };
            let d = w.data.skill_engine(real).and_then(|e| SkillEngine::pick(&e.drain, lv)).unwrap_or(d);
            drain(w, i, d);
            if pr(w, i).energy_left(w) == 0 {
                return set_state_idle(w, i);
            }
            p(w, i).ap.frame = ((t >> 3) & 1) + if id == 1 { 0 } else { 1 };
            if !held {
                set_state_idle(w, i);
            }
        }
        11 => {
            let t = tick_up(w);
            if t == 1 && s.fx == 0 {
                let f = fx::add(w, Fx::RecObj { what: fx::RecWhat::FlameCharge, owner: me, t: 0, alive: Alive::SkillTickLt { p: me, serial: s.serial, n: 24 } });
                sk_mut(w, i).unwrap().fx = f;
            }
            if t < 24 {
                return;
            }
            if t == 24 {
                if !held {
                    return set_state_idle(w, i);
                }
                let fxid = pr(w, i).sk.as_ref().unwrap().fx;
                if fxid != 0 {
                    if let Some(k) = w.find(fxid) {
                        w.ents[k].removed = true;
                    }
                    sk_mut(w, i).unwrap().fx = 0;
                }
                sfx_stop(w, 3);
                sfx(w, 32, 64);
                let (ffx, ffy) = FLAME_FIRST[facing as usize];
                let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                let (x, y) = (px + ffx, py + ffy);
                let b0 = flame_link_box(w, 0x0875_68b4, x, y);
                let fid = fx::add(w, Fx::FlameKame { owner: me, dir: facing, n: lv.min(2) + 3, links: vec![[x, y, 0, 0, b0.x0, b0.y0, b0.x1, b0.y1]], alive: Alive::Skill { p: me, serial: s.serial }, a, mult });
                sk_mut(w, i).unwrap().proj = fid;
                return;
            }
            drain(w, i, [107, 376, 1238][lv as usize]);
            if pr(w, i).energy_left(w) == 0 || !held {
                return set_state_idle(w, i);
            }
            let t2 = tick_up(w);
            p(w, i).ap.frame = ((t2 >> 3) & 1) + 1;
        }
        10 => {
            let t = tick_up(w);
            if t < 24 {
                if !held {
                    return set_state_idle(w, i);
                }
                p(w, i).ap.frame = if t < 20 { (t >> 2) & 1 } else { 2 };
                return;
            }
            if t == 24 {
                if !held {
                    return set_state_idle(w, i);
                }
                // the beam object (0x08036FA2) at player + 0x0871DF9C[dir], built now
                let (ffx, ffy) = shots::FINAL_FLASH_POS[facing as usize];
                let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                sfx(w, 32, 64);
                let bid = shots::add(w, shots::final_flash(me, facing, px + ffx, py + ffy, a, mult, lv.min(2) as u8, w.frame as i64));
                sk_mut(w, i).unwrap().proj = bid;
                p(w, i).beam_end.push((bid, vec![(true, 32)]));
                sfx_stop(w, 3);
            }
            drain(w, i, [107, 363, 1174][lv as usize]);
            if pr(w, i).energy_left(w) == 0 || !held {
                let proj = pr(w, i).sk.as_ref().unwrap().proj;
                if let Some(k) = w.find(proj) {
                    shots::full_beam_end(w, k);
                }
                sk_mut(w, i).unwrap().proj = 0;
                return set_state_idle(w, i);
            }
            p(w, i).ap.frame = ((t >> 3) & 1) + 3;
        }
        2 => {
            let Some(bi) = w.find(s.proj).filter(|_| s.proj != 0) else { return };
            if !held {
                if pr(w, i).ap.tick >= 16 {
                    if let ShotKind::BigBang(b) = &mut w.shot(bi).kind {
                        b.launched = true;
                    }
                    sfx_stop(w, 3);
                    sfx(w, 32, 64);
                } else {
                    w.ents[bi].removed = true;
                    sfx_stop(w, 3);
                }
                sk_mut(w, i).unwrap().proj = 0;
                p(w, i).ap.tick = 0;
                return;
            }
            let ch = match &w.shot_ref(bi).kind {
                ShotKind::BigBang(b) => b.charge256,
                _ => 0,
            };
            if ch >> 8 >= 1000 {
                return;
            }
            let acc = {
                let sk = sk_mut(w, i).unwrap();
                sk.acc += [43, 141, 427][lv as usize];
                sk.acc
            };
            let d = acc >> 8;
            if let ShotKind::BigBang(b) = &mut w.shot(bi).kind {
                b.charge256 += (d as i64) << 8;
            }
            spend(w, i, d, true);
            sk_mut(w, i).unwrap().acc &= 0xff;
        }
        4 => {
            let t = tick_up(w);
            if t == 16 {
                let (mx, my) = pr(w, i).hand_point();
                if let Some(k) = w.find(s.proj).filter(|_| s.proj != 0) {
                    shots::launch(w, k, mx << 8, my << 8);
                }
                sk_mut(w, i).unwrap().proj = 0;
                sfx(w, 34, 64);
                p(w, i).ap.frame = 1;
            } else if t == 32 {
                set_state_idle(w, i);
            }
        }
        23 => {
            let t = tick_up(w);
            if t > 32 {
                return set_state_idle(w, i);
            }
            p(w, i).ap.frame = t >> 4;
            if t == 16 {
                let (ox, oy) = VOLLEY_POS[facing as usize];
                let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                shots::add(w, shots::volley_wave(me, facing, px + ox, py + oy));
                sfx(w, 34, 64);
            }
        }
        8 => {
            let t = tick_up(w);
            if t < 8 {
                p(w, i).ap.frame = t >> 2;
                return;
            }
            if t >= 60 || !held {
                return set_state_idle(w, i);
            }
            p(w, i).ap.frame = ((t >> 2) & 1) + 2;
            if t == 8 && lv > 1 {
                let f = fx::add(w, Fx::RecObj { what: fx::RecWhat::EnergyPunch, owner: me, t: 0, alive: Alive::Skill { p: me, serial: s.serial } });
                sk_mut(w, i).unwrap().fx = f;
            }
            // one message to every object per frame (no sound / RNG): the melee rect (Lv3 the larger one) from the
            // position before the step
            {
                let a0 = &pr(w, i).a;
                let r = if lv > 1 { ENERGY_PUNCH_RECT_L3 } else { PUNCH_RECT }[facing as usize];
                let rect = Rect::at(r, a0.px(), a0.py());
                let st = pr(w, i).a.stats;
                let mut m = HitMsg { team: 1, rect, dir: facing, mult, atk: a, attacker: me, att_stats: st, att_is_player: true, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
                broadcast(w, &mut m, false);
            }
            if lv > 0 {
                let (vx, vy) = DASH_VEC[facing as usize];
                let (x, y) = (pr(w, i).a.x, pr(w, i).a.y);
                if fits(w, i, (x + vx * 0x60) >> 8, (y + vy * 0x60) >> 8) {
                    let pl = p(w, i);
                    pl.a.x += vx * 0x60;
                    pl.a.y += vy * 0x60;
                }
            }
        }
        9 => {
            if s.fx == 0 {
                let f = fx::add(w, Fx::EnergySword { owner: me, slash: -1, t: 0, alive: Alive::Skill { p: me, serial: s.serial } });
                sk_mut(w, i).unwrap().fx = f;
            }
            if !s.charged {
                if held {
                    spend(w, i, 4, false);
                    let t = tick_up(w);
                    p(w, i).ap.frame = (t >> 2) & 1;
                } else if s.t >= 80 {
                    {
                        let sk = sk_mut(w, i).unwrap();
                        sk.charged = true;
                        sk.hits = 0;
                    }
                    sfx_stop(w, 3);
                    sfx(w, 32, 64);
                    let fxid = pr(w, i).sk.as_ref().unwrap().fx;
                    if let Some(k) = w.find(fxid) {
                        if let Body::Fx(f) = &mut w.ents[k].body {
                            if let Fx::EnergySword { slash, .. } = f.as_mut() {
                                if *slash < 0 {
                                    *slash = 0;
                                }
                            }
                        }
                    }
                } else {
                    set_state_idle(w, i);
                }
                return;
            }
            let c = {
                let sk = sk_mut(w, i).unwrap();
                sk.hits += 1;
                sk.hits
            };
            if c >= 180 {
                return set_state_idle(w, i);
            }
            let hk = inp(w, i).held;
            if hk & DPAD != 0 {
                let hw = pr(w, i).walk_speed() >> 1;
                if hk & btn::LEFT != 0 {
                    walk_try(w, i, -hw, 0, 2);
                } else if hk & btn::RIGHT != 0 {
                    walk_try(w, i, hw, 0, 2);
                }
                if hk & btn::UP != 0 {
                    walk_try(w, i, 0, -hw, 1);
                } else if hk & btn::DOWN != 0 {
                    walk_try(w, i, 0, hw, 1);
                }
            }
            sk_mut(w, i).unwrap().hits = c + 1;
            let frame = ((c / 3) & 7) + 2;
            p(w, i).ap.frame = frame;
            let facing = pr(w, i).a.facing;
            let def = if facing == 3 { [14, 13, 12, 11, 10, 17, 16, 15] } else { [10, 11, 12, 13, 14, 15, 16, 17] }[(frame - 2) as usize];
            let [ox, oy, bw, bh] = SWORD_BOX[def - 10];
            let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
            let rect = Rect::new(px - ox, py - oy, px - ox + bw, py - oy + bh);
            // every swing frame: one message to the first receiver; every accepted frame plays 48 + rand(2)
            let st = pr(w, i).a.stats;
            let dir = [2, 0, 0, 0, 3, 1, 1, 1][def - 10];
            let mut m = HitMsg { team: 1, rect, dir, mult: 0x2800, atk: a, attacker: me, att_stats: st, att_is_player: true, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
            if broadcast(w, &mut m, true) {
                let r = 48 + w.rng.rand(2);
                sfx(w, r, 64);
            }
        }
        5 => {
            let tick = pr(w, i).ap.tick;
            if s.charged {
                if tick >= 24 {
                    return set_state_idle(w, i);
                }
                let t = tick_up(w);
                p(w, i).ap.frame = ((t >> 3) & 1) + 2;
                return;
            }
            if s.fx == 0 {
                let t = tick_up(w);
                if !held {
                    return set_state_idle(w, i);
                }
                if t == 16 {
                    let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                    let f = fx::add(w, fx::dragon_fist(w, px, py, facing, me));
                    sk_mut(w, i).unwrap().fx = f;
                    sfx(w, 32, 64);
                }
                p(w, i).ap.frame = t >> 3;
                return;
            }
            if tick < 90 {
                let (vx, vy) = DASH_VEC[facing as usize];
                let (x, y) = (pr(w, i).a.x, pr(w, i).a.y);
                let mut v = 0;
                if fits(w, i, (x + vx * 0x60) >> 8, (y + vy * 0x60) >> 8) {
                    let pl = p(w, i);
                    pl.a.x += vx * 0x60;
                    pl.a.y += vy * 0x60;
                    v = 0x60;
                }
                if let Some(k) = w.find(s.fx) {
                    fx::dragon_shift(w, k, v);
                }
            }
            let f = pr(w, i).eff.ep_cost_factor;
            let acc = {
                let sk = sk_mut(w, i).unwrap();
                sk.acc += f * 80;
                sk.acc
            };
            spend(w, i, acc >> 8, true);
            sk_mut(w, i).unwrap().acc &= 0xff;
            let tick = pr(w, i).ap.tick;
            if !(held && tick < 300 && pr(w, i).energy_left(w) != 0) {
                if let Some(k) = w.find(s.fx) {
                    fx::dragon_launch(w, k);
                }
                {
                    let sk = sk_mut(w, i).unwrap();
                    sk.fx = 0;
                    sk.charged = true;
                }
                p(w, i).ap.tick = 0;
                sfx_stop(w, 32);
                sfx(w, 23, 64);
            }
            let t = tick_up(w);
            p(w, i).ap.frame = ((t >> 3) & 1) + 2;
        }
        13 => {
            let t = tick_up(w);
            if t >= 40 {
                let n = s.ghosts.len() as i32;
                sk_mut(w, i).unwrap().charge = s.charge.max(n);
                if held {
                    p(w, i).ap.frame = 0;
                    if n < 3 && pr(w, i).can_afford(w, 160) {
                        spend(w, i, 160, false);
                        p(w, i).ap.tick = 0;
                    }
                } else {
                    set_state_idle(w, i);
                }
                return;
            }
            p(w, i).ap.frame = t >> 3;
            if t == 16 {
                let (gx, gy) = shots::GHOST_SPAWN[facing as usize];
                let (px, py) = (pr(w, i).a.px(), pr(w, i).a.py());
                let gid = shots::ghost(w, me, facing, (px + gx) << 8, (py + gy) << 8, s.ghosts.len() as i32);
                sk_mut(w, i).unwrap().ghosts.push(gid);
            }
        }
        17 => {
            let tick = pr(w, i).ap.tick;
            if tick == 0 {
                if inp(w, i).released & btn::B != 0 {
                    {
                        let pl = p(w, i);
                        pl.ap.tick = 1;
                        pl.ap.frame = 1;
                    }
                    sfx_stop(w, 3);
                    sfx(w, 34, 64);
                    if let Some(k) = w.find(s.proj).filter(|_| s.proj != 0) {
                        shots::masenko_release(w, k);
                    }
                }
                return;
            }
            let t = tick_up(w);
            if t >= 32 {
                sk_mut(w, i).unwrap().proj = 0;
                set_state_idle(w, i);
            }
        }
        _ => set_state_idle(w, i),
    }
}

/// a Flame Kamehameha link's record box at (x, y)
pub fn flame_link_box(w: &World, rec: u32, x: i32, y: i32) -> Rect {
    let g = w.data.record(rec).unwrap_or([4, 4, 8, 8]);
    Rect::new(x - g[0], y - g[1], x - g[0] + g[2], y - g[1] + g[3])
}

#[allow(dead_code)]
fn front_rect(w: &World, i: usize, reach: i32, thick: i32) -> Rect {
    let a = &pr(w, i).a;
    let b = a.body_box();
    let (cx, cy) = a.center();
    let h = thick >> 1;
    match a.facing {
        3 => Rect::new(b.x1, cy - h, b.x1 + reach, cy + h),
        2 => Rect::new(b.x0 - reach, cy - h, b.x0, cy + h),
        0 => Rect::new(cx - h, b.y1, cx + h, b.y1 + reach),
        _ => Rect::new(cx - h, b.y0 - reach, cx + h, b.y0),
    }
}

// ---- forms ---------------------------------------------------------------------------------------------------------------

fn start_transform(w: &mut World, i: usize, to: i32) {
    if to == 255 || w.data.form(to).is_none() {
        return;
    }
    {
        let pl = p(w, i);
        pl.transform_to = to;
        pl.ap.raw = 11;
    }
    enter_state(w, i, Gs::Skill(20));
    p(w, i).a.blocking = false;
    let (ax, ay) = pr(w, i).a.center();
    let me = w.ents[i].id;
    fx::add(w, Fx::Loop { id: 0x0875_2d28, x: ax, y: ay + 1, first: 0, n: 3, rec: 0, rate: 0x4000, acc: 0, alive: Alive::Transform { p: me } });
}
/// transformation control 0x0801C4F4
fn upd_transform(w: &mut World, i: usize) {
    let gs = pr(w, i).gs;
    if gs != Gs::Skill(20) && gs != Gs::SsUp {
        return;
    }
    let n = pr(w, i).anim_n(w);
    let den = (n * 16).max(1) as i64;
    let max_ep = pr(w, i).rec.max_ep as i64;
    let tick = pr(w, i).ap.tick as i64;
    let v = (max_ep * tick).div_euclid(if n * 16 == 0 { 1 } else { den }) & 0xffff;
    let ss = pr(w, i).form_ref(w).ss;
    let pl = p(w, i);
    if ss {
        pl.ss3_gauge = v * 65536;
    } else {
        pl.ss_gauge = v as i32;
    }
}
fn aura_form(w: &World, i: usize) -> bool {
    [0x3001b68, 0x3001b6c, 0x3001b78, 0x3001b7c, 0x3001b80, 0x3001b84, 0x3001b8c].contains(&pr(w, i).form_ref(w).controller)
}
fn form_enter(w: &mut World, i: usize) {
    if aura_form(w, i) {
        sfx(w, 54, 64);
    }
}
fn form_exit(w: &mut World, i: usize) {
    if aura_form(w, i) {
        sfx_stop(w, 54);
    }
}
/// script `set_form` (0x080036A5): leave the old form (0x0801712C), set the record's form, refresh the player entity
/// (0x08010692: SetAnim(0), the idle's rand(255)), enter the new form (0x0801711C; story playthrough s103: stop /
/// play sfx 54, one draw)
pub fn script_set_form(w: &mut World, i: usize, form: i32) {
    form_exit(w, i);
    apply_form(w, i, form);
    form_enter(w, i);
    set_anim_id(w, i, 0);
    new_form_object(w, i);
}
fn finish_transform(w: &mut World, i: usize) {
    {
        let pl = p(w, i);
        pl.transform_done = true;
        pl.ss_cooldown = 0x37fff;
    }
    let f = pr(w, i).form_ref(w).clone();
    let to = if f.up != 255 { f.up } else { pr(w, i).transform_to };
    form_exit(w, i);
    apply_form(w, i, to);
    form_enter(w, i);
    set_anim_id(w, i, 0);
    new_form_object(w, i);
    let f = pr(w, i).form_ref(w).clone();
    let pl = p(w, i);
    if f.ss3 {
        pl.ss3_gauge = pl.rec.max_ep as i64 * 65536;
    } else if f.ss {
        pl.ss_gauge = pl.rec.max_ep;
    }
}
pub fn revert(w: &mut World, i: usize) {
    if pr(w, i).form_ref(w).down == 255 || pr(w, i).dead {
        return;
    }
    set_anim_id(w, i, 24);
}
fn finish_saiyaman(w: &mut World, i: usize) {
    let to = if pr(w, i).gs == Gs::Skill(18) { 14 } else { 10 };
    form_exit(w, i);
    apply_form(w, i, to);
    form_enter(w, i);
    set_anim_id(w, i, 0);
}
fn finish_revert(w: &mut World, i: usize) {
    let f = pr(w, i).form_ref(w).clone();
    if f.down == 255 || w.data.form(f.down).is_none() {
        return set_anim_id(w, i, 0);
    }
    if f.ss || f.ss3 {
        let pl = p(w, i);
        pl.ss_gauge = 0;
        pl.ss3_gauge = 0;
    }
    form_exit(w, i);
    apply_form(w, i, f.down);
    form_enter(w, i);
    set_anim_id(w, i, 0);
    new_form_object(w, i);
}

// ---- Instant Transmission ---------------------------------------------------------------------------------------------

fn start_it(w: &mut World, i: usize, lv: i32) {
    {
        let pl = p(w, i);
        pl.ap.raw = 20;
        pl.ap.frame = 0;
    }
    enter_state(w, i, Gs::Hold);
    p(w, i).state = PState::Skill;
    let tier = if pr(w, i).can_afford(w, 600) {
        3
    } else if pr(w, i).can_afford(w, 120) {
        2
    } else if pr(w, i).can_afford(w, 40) {
        1
    } else {
        0
    };
    let cam = w.camera(pr(w, i).slot as usize);
    let mut cand: Vec<(EntityId, bool)> = w.live_enemies().into_iter().filter(|&e| overlaps(&w.en_ref(e).a.body_box(), &cam)).take(16).map(|e| (w.ents[e].id, false)).collect();
    if w.opts.pvp {
        // PvP rule set: the opposing players on this player's camera are targets too (after the enemies, 16 at most)
        let me = w.ents[i].id;
        for &id in &w.players {
            if cand.len() >= 16 || id == me {
                continue;
            }
            if let Some(k) = w.find(id).filter(|&k| !w.ents[k].removed && matches!(w.ents[k].body, Body::Player(_)) && pvp_targetable(w, k)) {
                if overlaps(&pr(w, k).a.body_box(), &cam) {
                    cand.push((id, false));
                }
            }
        }
    }
    // field: then the trigger list (0x08012846, the game's list order = our scan order reversed): a trigger whose
    // test answers the pick (only `0x0804AA08`, activation 0x0802266F) with its rect (x0 + 15, y0 + 7, x1, y1) on
    // camera; 16 candidates in all
    let mut tcand = vec![];
    if let Some(f) = w.field.as_ref() {
        for t in f.triggers.iter().rev() {
            if cand.len() + tcand.len() >= 16 {
                break;
            }
            if t.act != crate::fieldcontent::Activation::Unknown(0x0802_266F) {
                continue;
            }
            let r = Rect::new(t.rect.x0 + 15, t.rect.y0 + 7, t.rect.x1, t.rect.y1);
            if overlaps(&r, &cam) {
                tcand.push((t.obj, r, false));
            }
        }
    }
    let (cx, cy) = pr(w, i).a.center();
    let serial = w.next_serial();
    p(w, i).it_sel = Some(ItSel { serial, n: (lv + 1).min(tier), cand, picks: vec![], x: cx << 8, y: cy << 8, vx: 0, vy: 0, cursor: true, tcand, order: vec![] });
    let me = w.ents[i].id;
    let f = fx::add(w, Fx::ItMarkers { t: 0, alive: Alive::It { p: me, serial } });
    w.modal = Some(vec![me, f]);
}
fn it_select(w: &mut World, i: usize) {
    let mut s = pr(w, i).it_sel.clone().unwrap();
    let h = inp(w, i).held;
    if h & btn::B == 0 {
        w.modal = None;
        p(w, i).it_sel = None;
        if s.picks.is_empty() && s.order.is_empty() {
            return set_anim_id(w, i, 0);
        }
        let (dir, px, py) = (pr(w, i).a.facing, pr(w, i).a.px(), pr(w, i).a.py());
        {
            let pl = p(w, i);
            pl.it = Some(ItRun { serial: s.serial, dir, idx: 0, targets: s.picks.clone(), origin: (px, py), t: -24, picks: s.order.clone() });
            pl.a.hittable = false;
            pl.ap.raw = 20;
        }
        enter_state(w, i, Gs::Skill(14));
        p(w, i).a.hittable = false;
        return;
    }
    if s.n > 0 && s.cursor {
        let (px, py) = (s.x >> 8, s.y >> 8);
        let hitk = s.cand.iter().position(|&(id, picked)| {
            !picked
                && w.actor_by_id(id).is_some_and(|a| {
                    let b = a.body_box();
                    px >= b.x0 && px < b.x1 && py >= b.y0 && py < b.y1
                })
        });
        // the field's trigger candidates follow the enemies in the selector's list (first match wins)
        let hitt = if hitk.is_none() { s.tcand.iter().position(|&(_, b, picked)| !picked && px >= b.x0 && px < b.x1 && py >= b.y0 && py < b.y1) } else { None };
        if let Some(k) = hitk {
            s.cand[k].1 = true;
            s.picks.push(s.cand[k].0);
            if w.field.is_some() {
                s.order.push(ItPick::Ent(s.cand[k].0));
            }
            s.n -= 1;
            if s.n == 0 {
                s.cursor = false;
            }
        } else if let Some(k) = hitt {
            s.tcand[k].2 = true;
            s.order.push(ItPick::Trig(s.tcand[k].0));
            s.n -= 1;
            if s.n == 0 {
                s.cursor = false;
            }
        }
    }
    if s.cursor {
        s.vx = (s.vx * 3) / 4 + if h & btn::RIGHT != 0 { 256 } else { 0 } - if h & btn::LEFT != 0 { 256 } else { 0 };
        s.vy = (s.vy * 3) / 4 + if h & btn::DOWN != 0 { 256 } else { 0 } - if h & btn::UP != 0 { 256 } else { 0 };
        s.x += s.vx;
        s.y += s.vy;
        let cam = w.camera(pr(w, i).slot as usize);
        if s.x < cam.x0 << 8 || s.x > cam.x1 << 8 {
            s.x = (cam.x0 << 8).max((cam.x1 << 8).min(s.x));
            s.vx = 0;
        }
        if s.y < cam.y0 << 8 || s.y > cam.y1 << 8 {
            s.y = (cam.y0 << 8).max((cam.y1 << 8).min(s.y));
            s.vy = 0;
        }
    }
    p(w, i).it_sel = Some(s);
}
/// the field's Instant Transmission run (state vt 0x0804E334, update 0x0801BB1A): t from -24; t < 0 vanish (trigger
/// checks off, list +0xD0), at -23 the pick's EP (40 / 80 / 480, only while a pick is left) and sfx 31 (always); t 0
/// appear at the pick (a trigger: centre of (x0, y0, x1 + 15, y1 + 7) - (8, -4), facing down; an enemy: below it) or
/// back at the origin; t >= 24 checks on: a trigger pick is armed to fire in this frame's check (0x080125FA), the
/// next pick at t -24; an enemy pick punches like the battle run; no pick left: idle
fn ctl_it_field(w: &mut World, i: usize) {
    let Some(mut it) = pr(w, i).it.clone() else { return set_anim_id(w, i, 0) };
    p(w, i).a.hittable = false;
    // the event the arrival fired holds the run (story playthrough: s099's item pose, then the vanish resumes on the
    // frame after the event ends)
    if w.field.as_ref().is_some_and(|f| f.busy) {
        return;
    }
    it.t += 1;
    let t = it.t;
    let save = |w: &mut World, it: ItRun| p(w, i).it = Some(it);
    let checks = |w: &mut World, on: bool| {
        if let Some(f) = w.field.as_mut() {
            f.trig_off = !on;
        }
    };
    if t < 0 {
        if t == -23 {
            if it.picks.len() > it.idx {
                let c = [40, 80, 480][it.idx.min(2)];
                spend(w, i, c, false);
            }
            sfx(w, 31, 64);
        }
        checks(w, false);
        let pl = p(w, i);
        pl.ap.raw = 20;
        pl.ap.frame = -t >> 3;
        return save(w, it);
    }
    if t == 0 {
        loop {
            if it.idx >= it.picks.len() {
                let pl = p(w, i);
                pl.a.x = it.origin.0 << 8;
                pl.a.y = it.origin.1 << 8;
                pl.a.facing = it.dir;
                return save(w, it);
            }
            match it.picks[it.idx] {
                ItPick::Trig(o) => {
                    let r = w.field.as_ref().and_then(|f| f.triggers.iter().find(|t| t.obj == o)).map(|t| t.rect);
                    if let Some(r) = r {
                        let (cx, cy) = ((r.x0 + r.x1 + 15) >> 1, (r.y0 + r.y1 + 7) >> 1);
                        let pl = p(w, i);
                        pl.a.x = (cx - 8) << 8;
                        pl.a.y = (cy + 4) << 8;
                        pl.a.facing = 1;
                        return save(w, it);
                    }
                }
                ItPick::Ent(e) => {
                    if let Some(e) = w.actor_by_id(e).filter(|_| !w.is_removed(e)).cloned() {
                        let pl = p(w, i);
                        pl.a.x = (((e.px() * 2 + e.foot_w) >> 1) - 8) << 8;
                        pl.a.y = (e.py() + e.foot_h + 8) << 8;
                        pl.a.facing = 1;
                        return save(w, it);
                    }
                }
            }
            it.idx += 1;
        }
    }
    if t < 24 {
        p(w, i).ap.frame = (24 - t) >> 3;
        return save(w, it);
    }
    checks(w, true);
    if it.idx >= it.picks.len() {
        p(w, i).it = None;
        return set_anim_id(w, i, 0);
    }
    match it.picks[it.idx] {
        ItPick::Trig(o) => {
            if let Some(t) = w.field.as_mut().and_then(|f| f.triggers.iter_mut().find(|t| t.obj == o)) {
                t.forced = true;
            }
            it.idx += 1;
            it.t = -24;
            save(w, it)
        }
        ItPick::Ent(e) => {
            if t == 24 {
                let a = &pr(w, i).a;
                let rect = Rect::at(PUNCH_RECT[a.facing as usize], a.px(), a.py());
                let me = w.ents[i].id;
                let mut m = HitMsg { team: 1, rect, dir: a.facing, mult: 0x600, atk: atk(2), attacker: me, att_stats: a.stats, att_is_player: true, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
                if broadcast(w, &mut m, false) {
                    let r = 48 + w.rng.rand(2);
                    sfx(w, r, 64);
                }
            }
            let _ = e;
            p(w, i).ap.frame = (t - 24) >> 2;
            if pr(w, i).ap.frame >= pr(w, i).anim_n(w) {
                let pl = p(w, i);
                pl.ap.frame = 0;
                pl.ap.raw = 20;
                it.idx += 1;
                it.t = -24;
            }
            save(w, it)
        }
    }
}

fn ctl_it(w: &mut World, i: usize) {
    if w.field.is_some() {
        return ctl_it_field(w, i);
    }
    let Some(mut it) = pr(w, i).it.clone() else { return set_anim_id(w, i, 0) };
    p(w, i).a.hittable = false;
    it.t += 1;
    let t = it.t;
    let save = |w: &mut World, it: ItRun| p(w, i).it = Some(it);
    if t < 0 {
        if t == -23 && it.targets.len() > it.idx {
            let c = [40, 80, 480][it.idx.min(2)];
            spend(w, i, c, false);
            sfx(w, 31, 64);
        }
        let pl = p(w, i);
        pl.ap.raw = 20;
        pl.ap.frame = -t >> 3;
        return save(w, it);
    }
    if t == 0 {
        while it.idx < it.targets.len() && w.is_removed(it.targets[it.idx]) {
            it.idx += 1;
        }
        if it.idx >= it.targets.len() {
            let pl = p(w, i);
            pl.a.x = it.origin.0 << 8;
            pl.a.y = it.origin.1 << 8;
            pl.a.facing = it.dir;
            return save(w, it);
        }
        let e = w.actor_by_id(it.targets[it.idx]).cloned().unwrap();
        let pl = p(w, i);
        pl.a.x = (((e.px() * 2 + e.foot_w) >> 1) - 8) << 8;
        pl.a.y = (e.py() + e.foot_h + 8) << 8;
        pl.a.facing = 1;
        return save(w, it);
    }
    if t < 24 {
        p(w, i).ap.frame = (24 - t) >> 3;
        return save(w, it);
    }
    if it.idx >= it.targets.len() {
        p(w, i).it = None;
        return set_anim_id(w, i, 0);
    }
    if t == 24 {
        let a = &pr(w, i).a;
        let rect = Rect::at(PUNCH_RECT[a.facing as usize], a.px(), a.py());
        let me = w.ents[i].id;
        let mut m = HitMsg { team: 1, rect, dir: a.facing, mult: 0x600, atk: atk(2), attacker: me, att_stats: a.stats, att_is_player: true, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
        if broadcast(w, &mut m, false) {
            let r = 48 + w.rng.rand(2);
            sfx(w, r, 64);
        }
    }
    p(w, i).ap.frame = (t - 24) >> 2;
    if pr(w, i).ap.frame >= pr(w, i).anim_n(w) {
        let pl = p(w, i);
        pl.ap.frame = 0;
        pl.ap.raw = 20;
        it.idx += 1;
        it.t = -24;
    }
    save(w, it);
}

// ---- held ---------------------------------------------------------------------------------------------------------------

fn upd_held(w: &mut World, i: usize) {
    let k = pr(w, i).hold_kind;
    let by = pr(w, i).held_by;
    let InputFrame { pressed, .. } = inp(w, i);
    let frame = w.frame as i64;
    let mash = |w: &mut World, mask: u16, need: i32, gap: i64| -> bool {
        let pl = p(w, i);
        if pressed & mask != 0 {
            if frame - pl.hold_last > gap {
                pl.hold_count = 0;
            }
            pl.hold_count += 1;
            pl.hold_last = frame;
        }
        pl.hold_count > need
    };
    if by != 0 && w.is_removed(by) {
        return release(w, i);
    }
    match k {
        // the vampire grab: the escape count, the drain and the vampire's position are the VampireGrab node's
        Some(HoldKind::Vampire) => {}
        Some(HoldKind::Hug) => {
            if mash(w, DPAD, 4, 90) {
                return release(w, i);
            }
        }
        Some(HoldKind::Latch) => {
            if mash(w, btn::A | btn::B | DPAD, 8, 15) {
                return release(w, i);
            }
        }
        Some(HoldKind::Grapple) => {
            if let Some(ba) = w.actor_by_id(by).cloned() {
                let (bx, by2) = ba.foot_center();
                let (px, py) = pr(w, i).a.foot_center();
                let dx = (bx - px).signum();
                let dy = (by2 - py).signum();
                if (bx - px).abs() + (by2 - py).abs() > 24 {
                    move_by(w, i, dx * 512, dy * 512);
                }
            }
        }
        _ => {}
    }
    if matches!(k, Some(HoldKind::Latch) | Some(HoldKind::Hug)) {
        if let Some(ba) = w.actor_by_id(by).cloned() {
            let (bx, byy) = ba.foot_center();
            let hug = k == Some(HoldKind::Hug);
            let nx = bx - 8 + if hug { 8 } else { 0 };
            let ny = byy - 4 + if hug { -2 } else { 2 };
            if fits(w, i, nx, ny) {
                let pl = p(w, i);
                pl.a.x = nx << 8;
                pl.a.y = ny << 8;
            }
        }
    }
    if pr(w, i).hold_left > 0 {
        p(w, i).hold_left -= 1;
        if pr(w, i).hold_left == 0 {
            release(w, i);
        }
    }
}

// ---- damage ---------------------------------------------------------------------------------------------------------------

/// PvP rule set (`World::opts.pvp`, multiplayer only, not in the original game): another player's attack (melee,
/// skills, its shots and effects: attacker = that player) is accepted like an enemy attack, same damage pipeline
/// PvP: player entity `i` can currently be hit by another player's attack (alive, hittable, not being eaten) - what
/// the homing shots and Instant Transmission may pick as an opposing target
pub fn pvp_targetable(w: &World, i: usize) -> bool {
    let pl = pr(w, i);
    !pl.dead && pl.a.hittable && !(pl.state == PState::Held && pl.hold_kind == Some(HoldKind::Eat))
}

fn pvp_hit(w: &World, i: usize, msg: &HitMsg) -> bool {
    w.opts.pvp && msg.att_is_player && msg.attacker != w.ents[i].id && w.find(msg.attacker).is_some_and(|k| matches!(w.ents[k].body, Body::Player(_)))
}

pub fn on_message(w: &mut World, i: usize, msg: &mut HitMsg) -> bool {
    {
        let pl = pr(w, i);
        if msg.team == 1 && !pvp_hit(w, i, msg) {
            return false;
        }
        if pl.state == PState::Held && pl.hold_kind == Some(HoldKind::Eat) {
            return false;
        }
        // the eaten player is not in the world
        if pl.gone {
            return false;
        }
        // message kind 1 tests the footprint (0x0800FDFA), else the body box
        if !overlaps(&msg.rect, &if msg.foot { pl.a.foot_box() } else { pl.a.body_box() }) {
            return false;
        }
        // KO: 0x08010F74 skips ComputeDamage but still accepts a box overlapping the body (an enemy wave puffs)
        if pl.dead {
            return msg.team != 1;
        }
        // not hittable (+0x1CC: reverting, Instant Transmission, the cutscene fusions): not accepted
        if !pl.a.hittable {
            return false;
        }
    }
    let pl = pr(w, i);
    let front = (msg.dir ^ 1) == pl.a.facing;
    let blocking = pl.a.blocking && front;
    let def = pl.a.stats;
    let res = compute_damage(msg.atk, msg.mult, &msg.att_stats, &def, blocking, &mut w.rng);
    let dmg = res.dmg;
    msg.crit = res.crit;
    msg.raw = res.raw;
    msg.dmg = dmg;
    if res.unblocked != dmg {
        w.events.push(Event::DamageRoll { target: w.ents[i].id, dmg: res.unblocked });
    }
    w.events.push(Event::Damage { target: w.ents[i].id, attacker: msg.attacker, dmg, crit: res.crit });
    if msg.team == 1 {
        // PvP: the attacker's hit cue and on-hit hooks, as when it hits an enemy
        w.events.push(Event::Hit);
        if let Some(ai) = w.find(msg.attacker).filter(|_| dmg > 0) {
            on_dealt(w, ai, msg, dmg);
        }
    }
    let before = pr(w, i).rec.hp;
    let lost = if dmg > 0 && before > 0 { p(w, i).hp_acc.take(dmg) } else { 0 };
    // 0x0800E8E6 runs for every accepted hit, 0 damage included: the 12-frame flash restarts
    p(w, i).a.flash = 12;
    if dmg == 0 || before == 0 {
        return true;
    }
    w.stats.hits_taken += 1;
    if lost > 0 {
        let hp = pr(w, i).rec.hp as i64;
        w.stats.damage_taken += lost.min(hp);
        p(w, i).rec.hp = 0.max(hp - lost) as i32;
        let thorns = pr(w, i).eff.thorns;
        if thorns != 0 && msg.atk.kind == 0 {
            if let Some((ei, _)) = w.enemy_by_id(msg.attacker) {
                crate::enemy::take_direct(w, ei, (dmg * thorns as i64) >> 8);
            }
        }
        after_hp_change(w, i);
        if pr(w, i).dead {
            return true;
        }
    }
    // stone: the counter's reaction 0x0802E348 replaces the hurt state
    if pr(w, i).gs == Gs::Stone {
        p(w, i).stone_hp -= (dmg >> 10) as i32;
        return true;
    }
    if pr(w, i).state == PState::Held {
        return true;
    }
    if msg.no_react {
        // the flames skip the reaction; in the transformation singleton (SetAnim(23)) they call SetAnim(0)
        if pr(w, i).gs == Gs::SsUp {
            set_anim_id(w, i, 0);
        }
        return true;
    }
    if blocking {
        let (bx, by) = BLOCK_PUSH[pr(w, i).a.facing as usize];
        let (x, y) = (pr(w, i).a.x, pr(w, i).a.y);
        if fits(w, i, (x + bx) >> 8, (y + by) >> 8) {
            let pl = p(w, i);
            pl.a.x += bx;
            pl.a.y += by;
        }
        return true;
    }
    {
        let pl = p(w, i);
        pl.a.facing = msg.dir ^ 1;
        pl.hurt_dir = msg.dir;
        pl.ap.raw = 10;
    }
    enter_state(w, i, Gs::Hurt);
    true
}
/// KO (0x0801088A)
fn after_hp_change(w: &mut World, i: usize) {
    if pr(w, i).equip.contains(&153) {
        refresh(w, i);
    }
    if pr(w, i).rec.hp <= 0 && !pr(w, i).dead {
        {
            let pl = p(w, i);
            pl.rec.hp = 0;
            pl.held_by = 0;
            pl.hold_kind = None;
            pl.a.alpha = 256;
        }
        let f = pr(w, i).form_ref(w).clone();
        if f.flags & 0x40 != 0 && w.data.form(f.down).is_some() {
            sfx_stop(w, 54);
            p(w, i).form = f.down;
        }
        p(w, i).ap.raw = 18;
        enter_state(w, i, Gs::Ko);
        // the death script (started here) first runs on the next frame: the script runner comes before the object
        // updates, whichever object dealt the KO (an injected hit lands before it: world.rs Injector)
        let fr = w.frame as i64 + 1;
        p(w, i).ko_at = fr;
    }
}

// ---- lockstep ---------------------------------------------------------------------------------------------------------------

/// copy the emulator's animation fields at frame 0
pub fn lockstep_init(w: &mut World, i: usize, raw: i32, frame: i32, tick: i32, state: &str) {
    let gs = match state {
        "ss_up" => Gs::SsUp,
        "ss_revert" => Gs::SsRevert,
        _ => pr(w, i).idle_state(w),
    };
    let mut pl = pr(w, i).clone();
    pl.gs = gs;
    pl.state = coarse(&pl, gs);
    if gs == Gs::SsUp {
        pl.transform_to = pl.form_ref(w).up;
        pl.transform_done = false;
    }
    pl.ap = AnimPos { raw, frame, tick };
    pl.sync_anim(w);
    *p(w, i) = pl;
}

/// what the lockstep compares: state name, raw animation (-1 when the sprite lacks it), frame, ROM frame record
pub fn lockstep_view(w: &World, i: usize) -> (String, i32, i32, u32) {
    let pl = pr(w, i);
    // eaten: the recorder finds no player object
    if pl.gone {
        return ("gone".into(), 0, 0, 0);
    }
    let state = pl.gs.name();
    // candy: the def is 0x36 (sprite 45)
    let spr = if pl.candy_def { Some(45) } else { pl.a.sprite };
    let Some(a) = spr.and_then(|s| w.data.sprite(s)).and_then(|s| s.anim_raw(pl.ap.raw)) else { return (state, -1, pl.ap.frame, 0) };
    let rom = a.dirs[pl.a.facing as usize & 3].get(pl.ap.frame.max(0) as usize).map(|s| s.rom).unwrap_or(0);
    let rom = if pl.ap.frame < 0 { 0 } else { rom };
    (state, a.canon_raw, pl.ap.frame, rom)
}

// ---- mod hooks (modkit.rs): the player operations a scripted skill uses ----------------------------------------------

pub(crate) fn mod_can_afford(w: &World, i: usize, cost: i32) -> bool {
    pr(w, i).can_afford(w, cost)
}
pub(crate) fn mod_spend(w: &mut World, i: usize, cost: i32) {
    spend(w, i, cost, false)
}
pub(crate) fn mod_drain(w: &mut World, i: usize, per256: i32) {
    drain(w, i, per256)
}
pub(crate) fn mod_energy_left(w: &World, i: usize) -> i64 {
    pr(w, i).energy_left(w)
}
pub(crate) fn mod_skill_cost(w: &World, i: usize, id: i32) -> i32 {
    pr(w, i).skill_cost(w, id)
}
pub(crate) fn mod_skill_lv(w: &World, i: usize, id: i32) -> i32 {
    pr(w, i).skill_lv(w, id)
}
/// enter the scripted skill state (Gs::Skill(SCRIPT_GS)) with a fresh skill instance
pub(crate) fn mod_enter_skill(w: &mut World, i: usize, real: i32) {
    enter_state(w, i, Gs::Skill(crate::modkit::SCRIPT_GS));
    let serial = w.next_serial();
    let pl = p(w, i);
    pl.state = PState::Skill;
    pl.t = 0;
    pl.sk = Some(SkillInst { id: real, serial, held: true, release: -1, ..Default::default() });
}
pub(crate) fn mod_set_anim(w: &mut World, i: usize, names: &[&'static str]) {
    let refs: Vec<AnimRef> = names.iter().map(|n| AnimRef::Name(*n)).collect();
    set_anim_named(w, i, &refs, true);
}
/// show the animation `raw` from its first cel
pub(crate) fn mod_set_anim_raw(w: &mut World, i: usize, raw: i32) {
    let pl = p(w, i);
    pl.ap.raw = raw;
    pl.ap.tick = 0;
    pl.ap.frame = 0;
    let mut c = pr(w, i).clone();
    c.sync_anim(w);
    *p(w, i) = c;
}
/// a scripted skill's play-once animation has reached its end
pub(crate) fn mod_anim_done(w: &World, i: usize) -> bool {
    let (looping, shift) = crate::modkit::anim_mode(w, i);
    !looping && (pr(w, i).ap.tick >> shift) >= pr(w, i).anim_n(w)
}
pub(crate) fn mod_to_idle(w: &mut World, i: usize) {
    set_state_idle(w, i)
}
pub(crate) fn mod_fits(w: &World, i: usize, x: i32, y: i32) -> bool {
    fits(w, i, x, y)
}
