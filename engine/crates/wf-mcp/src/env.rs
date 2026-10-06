//! One agent session = one wf-core battle `Session` + what an agent needs around it: the per-frame input log (all
//! player slots), keyframe snapshots for O(1) rewinds, named save states, structured events since the last
//! observation, per-slot autopilots (the slots the agent is not driving this frame) and replays.
//!
//! Time travel uses true snapshots (`Session::snapshot` = a `World` clone, ~2 µs; story sessions add the script
//! host's state); the input log is kept so a replay (config + inputs) can always re-simulate the session from frame 0
//! and check the state hash. A session's frame is the number of steps taken (= the input log's length): in story
//! sessions and across field area loads `World.frame` (the object passes) stands still in menus, holds and loads.

use crate::macros::{Policy, Step};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use wf_core::battle::{BattleConfig, Outcome};
use wf_core::content::GameData;
use wf_core::field::FieldConfig;
use wf_core::player::{PState, Player};
use wf_core::session::{Session, Snapshot};
use wf_core::world::{Body, Event, World, MAX_PLAYERS};

pub type Held = [u16; MAX_PLAYERS];

/// a session's state: the core session, and for story sessions the script host's between-frames state
#[derive(Clone)]
pub struct Snap {
    pub s: Snapshot,
    pub host: Option<Box<wf_import::HostSnapshot>>,
}

/// a story session's script host (the app's story mode, wf-app `StorySim`: the same `wf_import::StoryHost`)
pub struct Story {
    pub host: wf_import::StoryHost,
    /// the content's scripts (a fork builds its own host from them)
    pub files: Arc<BTreeMap<String, String>>,
    pub start: StoryStart,
}

/// how a story session started (replays re-create it)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoryStart {
    /// RNG seed of the session (the title's boot carries it into the new game)
    pub seed: u32,
}
const KEYFRAME_EVERY: u32 = 120;
const MAX_EVENTS: usize = 400;

/// what one frame did, from the point of view of one player slot
#[derive(Clone, Copy, Debug, Default)]
pub struct Delta {
    pub hp_lost: i64,
    pub dealt: i64,
    pub hits_dealt: u32,
    pub no_damage_hits: u32,
    pub healed_enemy: i64,
    pub hits_taken: u32,
    pub kills: u32,
    pub phase: bool,
}

pub struct Agent {
    pub id: String,
    pub s: Session,
    /// human label of the fight (fighter vs enemies)
    pub label: String,
    /// the new_battle arguments after resolution (for listing / evaluate_controller)
    pub spec: Value,
    pub inputs: Vec<Held>,
    initial: Snap,
    keyframes: Vec<(u32, Snap)>,
    pub named: BTreeMap<String, (Snap, Vec<Held>)>,
    /// structured events since the last draining observation
    pub events: Vec<Value>,
    pub events_dropped: usize,
    /// the events of the last frame (controllers see these without draining)
    pub last_events: Vec<Value>,
    /// hazard positions on the previous frame (velocities)
    pub prev_pos: BTreeMap<u32, (i32, i32)>,
    /// slots driven by a policy when the agent does not drive them
    pub autopilot: Vec<Option<Box<dyn Policy>>>,
    pub autopilot_desc: Vec<String>,
    /// scratch session for look-ahead (forecasts)
    scratch: RefCell<Option<Session>>,
    pub record: bool,
    /// per enemy: (top AI state, its stack entry serial, frame it started) for `in_state`
    pub state_since: BTreeMap<u32, (u16, u32, u32)>,
    /// field sessions: how it started (battle sessions: None, the config is `s.config`)
    pub field_cfg: Option<FieldConfig>,
    /// story sessions: the script host (title, New Game, the story's events and dialogue, saves)
    pub story: Option<Story>,
    /// the mods this session's content was built with (None: the plain base pack)
    pub content: Option<Arc<crate::content::ModSet>>,
}

pub fn coarse(s: PState) -> &'static str {
    match s {
        PState::Idle => "idle",
        PState::Walk => "walk",
        PState::Dash => "dash",
        PState::Punch => "punch",
        PState::Block => "block",
        PState::Hurt => "hurt",
        PState::Dying => "dying",
        PState::Skill => "skill",
        PState::Transform => "transform",
        PState::Held => "held",
    }
}

pub fn can_act(p: &Player) -> bool {
    !p.dead && p.held_by == 0 && matches!(p.state, PState::Idle | PState::Walk | PState::Dash)
}

pub fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Running => "running",
        Outcome::Won => "won",
        Outcome::Lost => "lost",
    }
}

/// "#3 Majin Fighter" / "you" / "player 2" / a shot's owner
pub fn describe(w: &World, id: u32, me: Option<u32>) -> String {
    if Some(id) == me {
        return "you".into();
    }
    if let Some(slot) = w.players.iter().position(|&p| p == id) {
        return format!("player {slot}");
    }
    if let Some((_, e)) = w.enemy_by_id(id) {
        let name = w.data.enemy(e.ty).map(|t| t.name.as_str()).unwrap_or("?");
        if e.part {
            return format!("#{} {} (part of #{})", id, name, e.parent);
        }
        return format!("#{id} {name}");
    }
    if let Some(i) = w.find(id) {
        return format!("#{} {}", id, w.ents[i].kind());
    }
    format!("#{id}")
}

impl Agent {
    pub fn new(id: String, data: Arc<GameData>, config: BattleConfig, label: String, spec: Value) -> Agent {
        Agent::from_session(id, Session::new(data, config), None, label, spec)
    }

    /// a field (overworld) session: an area with its placed objects; scripts / dialogue are not run (no host)
    pub fn new_field(id: String, data: Arc<GameData>, config: FieldConfig, label: String, spec: Value) -> Agent {
        Agent::from_session(id, Session::new_field(data, config.clone()), Some(config), label, spec)
    }

    /// a story session on the game's title screen (START -> Single Player -> New Game), as the app's
    /// `launcher::title_app`: headless, one step per game logic iteration (`Options::no_lag`)
    pub fn new_story(id: String, data: Arc<GameData>, files: Arc<BTreeMap<String, String>>, start: StoryStart) -> Result<Agent, String> {
        let mut s = Self::story_session(&data, &start);
        wf_core::field::attach_host(&mut s.world, true);
        let mut host = wf_import::StoryHost::new(&s.data, &files)?;
        host.start(&mut s);
        // (as the app's title: the placeholder area under the title shows no area banner)
        s.world.scene.banner = None;
        let mut a = Agent::from_session(id, s, None, "story".into(), json!({"story": start}));
        a.story = Some(Story { host, files, start });
        a.initial = a.snap();
        Ok(a)
    }

    fn story_session(data: &Arc<GameData>, start: &StoryStart) -> Session {
        let cfg = FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::Seed(start.seed), ..Default::default() };
        let mut s = Session::new_field(Arc::clone(data), cfg);
        s.world.opts.no_lag = true;
        // the three slot files, empty (as the app's title: its hash covers the slot list)
        for i in 0..wf_core::menu::save::SLOTS {
            s.world.saves.set_slot_bytes(i, &[]);
        }
        wf_core::menu::title::open_title(&mut s.world);
        s
    }

    /// the session's state (story sessions: with the script host's)
    pub fn snap(&self) -> Snap {
        Snap { s: self.s.snapshot(), host: self.story.as_ref().map(|st| Box::new(st.host.snapshot())) }
    }
    fn restore_snap(&mut self, sn: &Snap) {
        self.s.restore(&sn.s);
        if let (Some(st), Some(h)) = (self.story.as_mut(), sn.host.as_ref()) {
            st.host.restore(h);
        }
    }

    /// one frame of the core (story sessions: in the script host's order, as the app's story mode)
    fn raw_step(&mut self, held: &Held) -> Vec<Event> {
        match self.story.as_mut() {
            None => self.s.step(held).to_vec(),
            Some(st) => {
                let mut frames = [wf_core::input::InputFrame::default(); MAX_PLAYERS];
                for (k, f) in frames.iter_mut().enumerate() {
                    *f = wf_core::input::InputFrame::from_held(held[k], self.s.prev[k]);
                    self.s.prev[k] = held[k];
                }
                // (the title's New Game / Continue replace the world: the new field is the host's too)
                wf_core::field::attach_host(&mut self.s.world, true);
                st.host.step_inputs(&mut self.s, frames);
                self.s.world.events.clone()
            }
        }
    }

    fn make_session(&self) -> Session {
        if let Some(st) = &self.story {
            return Self::story_session(&self.s.data, &st.start);
        }
        match &self.field_cfg {
            Some(f) => Session::new_field(Arc::clone(&self.s.data), f.clone()),
            None => Session::new(Arc::clone(&self.s.data), self.s.config.clone()),
        }
    }

    fn from_session(id: String, s: Session, field_cfg: Option<FieldConfig>, label: String, spec: Value) -> Agent {
        let initial = s.snapshot();
        let n = s.world.players.len().max(1);
        let initial = Snap { s: initial, host: None };
        Agent {
            id,
            s,
            label,
            spec,
            inputs: Vec::new(),
            initial,
            keyframes: Vec::new(),
            named: BTreeMap::new(),
            events: Vec::new(),
            events_dropped: 0,
            last_events: Vec::new(),
            prev_pos: BTreeMap::new(),
            autopilot: (0..n).map(|_| None).collect(),
            autopilot_desc: vec!["idle".into(); n],
            scratch: RefCell::new(None),
            record: true,
            state_since: BTreeMap::new(),
            field_cfg,
            story: None,
            content: None,
        }
    }

    pub fn data(&self) -> &Arc<GameData> {
        &self.s.data
    }
    pub fn w(&self) -> &World {
        &self.s.world
    }
    /// steps taken (the input log's length; `World.frame` stands still in menus, holds and loads)
    pub fn frame(&self) -> u32 {
        self.inputs.len() as u32
    }
    pub fn slots(&self) -> usize {
        self.s.world.players.len().max(1)
    }
    pub fn pidx(&self, slot: usize) -> Option<usize> {
        self.s.world.player_idx(slot)
    }
    pub fn player(&self, slot: usize) -> Option<&Player> {
        self.pidx(slot).map(|i| self.s.world.pl_ref(i))
    }
    pub fn player_id(&self, slot: usize) -> Option<u32> {
        self.s.world.players.get(slot).copied()
    }
    pub fn outcome(&self) -> Outcome {
        self.s.outcome
    }
    pub fn hash(&self) -> String {
        format!("{:016x}", self.s.hash())
    }

    /// advance one frame: `slot` holds `mask`, the other slots run their autopilot (idle when none)
    pub fn step(&mut self, slot: usize, mask: u16) -> Delta {
        let mut held: Held = [0; MAX_PLAYERS];
        held[slot.min(MAX_PLAYERS - 1)] = mask;
        for k in 0..self.slots().min(MAX_PLAYERS) {
            if k == slot {
                continue;
            }
            if let Some(mut ap) = self.autopilot.get_mut(k).and_then(|a| a.take()) {
                let m = match ap.next(self, k) {
                    Step::Press(m) => m,
                    Step::Done(_) => 0,
                };
                held[k] = m;
                self.autopilot[k] = Some(ap);
            }
        }
        self.step_held(held, slot)
    }

    /// advance one frame with explicit buttons for every slot; the delta is computed for `view_slot`
    pub fn step_held(&mut self, held: Held, view_slot: usize) -> Delta {
        let f = self.frame();
        if f.is_multiple_of(KEYFRAME_EVERY) && f > 0 && self.keyframes.last().is_none_or(|k| k.0 < f) {
            let sn = self.snap();
            self.keyframes.push((f, sn));
        }
        if !self.record {
            self.raw_step(&held);
            self.inputs.push(held);
            self.track_states();
            return Delta::default();
        }
        // before: hazard positions, enemy HP / defeated, player HP, phase
        let w = &self.s.world;
        self.prev_pos.clear();
        for e in &w.ents {
            if e.removed {
                continue;
            }
            match &e.body {
                Body::Shot(s) => {
                    self.prev_pos.insert(e.id, (s.px(), s.py()));
                }
                Body::Hand(h) => {
                    self.prev_pos.insert(e.id, (h.a.px(), h.a.py()));
                }
                _ => {}
            }
        }
        let names0: BTreeMap<u32, String> = w.ents.iter().filter(|e| matches!(e.body, Body::Enemy(_))).map(|e| (e.id, describe(w, e.id, None))).collect();
        let before: Vec<(u32, i64, bool)> = w.ents.iter().filter_map(|e| if let Body::Enemy(en) = &e.body { Some((e.id, en.hp1024, en.defeated || e.removed)) } else { None }).collect();
        let php: Vec<(u32, i32, bool, PState)> = w.players.iter().filter_map(|&id| w.find(id).map(|i| { let p = w.pl_ref(i); (id, p.hp(), p.dead, p.state) })).collect();
        let phase0 = w.battle.phase;
        let out0 = self.s.outcome;
        let me = self.player_id(view_slot);

        let evs: Vec<Event> = self.raw_step(&held);
        self.inputs.push(held);
        self.track_states();
        // (events carry the session's frame: the step count `rewind {to_frame}` takes)
        let fr = self.inputs.len() as u32;

        let w = &self.s.world;
        let name = |id: u32| -> String { if w.find(id).is_some() { describe(w, id, me) } else { names0.get(&id).cloned().unwrap_or_else(|| describe(w, id, me)) } };
        let mut d = Delta::default();
        let mut out = Vec::new();
        let enemy_hp = |id: u32| w.enemy_by_id(id).map(|(_, e)| e.hp1024);
        let mut seen_enemy = Vec::new();
        for ev in &evs {
            match ev {
                Event::Damage { target, attacker, dmg, crit } => {
                    if let Some(&(pid, hp0, _, _)) = php.iter().find(|p| p.0 == *target) {
                        let hp1 = w.find(pid).map(|i| w.pl_ref(i).hp()).unwrap_or(0);
                        let lost = (hp0 - hp1).max(0) as i64;
                        let blocked = w.find(pid).is_some_and(|i| w.pl_ref(i).a.blocking);
                        let mut v = json!({"f": fr, "type": "hurt", "by": name(*attacker), "hp": -lost});
                        if Some(pid) != me {
                            v["type"] = json!("ally_hurt");
                            v["target"] = json!(describe(w, pid, me));
                        } else {
                            d.hp_lost += lost;
                            if lost > 0 || *dmg > 0 {
                                d.hits_taken += 1;
                            }
                        }
                        if *crit {
                            v["crit"] = json!(true);
                        }
                        if blocked {
                            v["blocked"] = json!(true);
                        }
                        if lost == 0 {
                            v["note"] = json!(if *dmg == 0 { "no damage (shielded / immune)" } else { "no HP lost (recovering i-frames)" });
                        }
                        out.push(v);
                    } else if let Some(&(eid, hp0, _)) = before.iter().find(|b| b.0 == *target) {
                        let hp1 = enemy_hp(eid).unwrap_or(0);
                        let first = !seen_enemy.contains(&eid);
                        seen_enemy.push(eid);
                        let change = if first { (hp0 - hp1) / 1024 } else { 0 };
                        let raw = dmg / 1024;
                        let by_me = Some(*attacker) == me;
                        let effect = if !first {
                            "same frame"
                        } else if hp1 < hp0 {
                            "damage"
                        } else if hp1 > hp0 {
                            "healed it"
                        } else if *dmg == 0 {
                            "no damage (resisted / blocked)"
                        } else {
                            "no damage (its current state ignores this attack)"
                        };
                        if by_me {
                            if hp1 < hp0 && first {
                                d.dealt += change;
                                d.hits_dealt += 1;
                            } else if hp1 > hp0 && first {
                                d.healed_enemy += -change;
                            } else if first {
                                d.no_damage_hits += 1;
                            }
                        }
                        let mut v = json!({"f": fr, "type": "hit", "by": name(*attacker), "target": name(eid), "effect": effect, "hp_change": -change, "computed": raw, "target_hp": (hp1 / 1024).max(-9999)});
                        if *crit {
                            v["crit"] = json!(true);
                        }
                        out.push(v);
                    }
                }
                Event::SkillStart { id } => {
                    out.push(json!({"f": fr, "type": "skill", "name": crate::catalog::skill_name(&w.data, *id)}));
                }
                Event::Info { text } => out.push(json!({"f": fr, "type": "info", "text": text})),
                Event::Field(fe) => {
                    let mut v = serde_json::to_value(fe).unwrap_or(Value::Null);
                    if let Value::Object(m) = &mut v {
                        m.insert("f".into(), json!(fr));
                        m.insert("type".into(), json!("field"));
                    }
                    out.push(v);
                }
                _ => {}
            }
        }
        for e in &w.ents {
            if let Body::Enemy(en) = &e.body {
                let dead = en.defeated || e.removed;
                match before.iter().find(|b| b.0 == e.id) {
                    Some(b) if !b.2 && dead => {
                        d.kills += 1;
                        out.push(json!({"f": fr, "type": "defeated", "target": name(e.id)}));
                    }
                    None if !en.part && !dead => out.push(json!({"f": fr, "type": "spawn", "target": describe(w, e.id, me)})),
                    _ => {}
                }
            }
        }
        for b in &before {
            if !b.2 && w.find(b.0).is_none() {
                d.kills += 1;
                out.push(json!({"f": fr, "type": "defeated", "target": name(b.0)}));
            }
        }
        for &(pid, _, dead0, st0) in &php {
            if let Some(i) = w.find(pid) {
                let p = w.pl_ref(i);
                let who = if Some(pid) == me { "you".to_string() } else { describe(w, pid, me) };
                if p.dead && !dead0 {
                    out.push(json!({"f": fr, "type": "ko", "who": who}));
                }
                if p.state == PState::Held && st0 != PState::Held {
                    out.push(json!({"f": fr, "type": "grabbed", "who": who, "by": describe(w, p.held_by, me), "hint": "mash buttons / wait"}));
                }
            }
        }
        if w.battle.phase != phase0 {
            d.phase = true;
            out.push(json!({"f": fr, "type": "phase", "phase": w.battle.phase, "of": w.battle.phases.len()}));
        }
        if self.s.outcome != out0 {
            out.push(json!({"f": fr, "type": "outcome", "outcome": outcome_str(self.s.outcome)}));
        }
        self.last_events = out.clone();
        self.events.extend(out);
        if self.events.len() > MAX_EVENTS {
            let n = self.events.len() - MAX_EVENTS;
            self.events.drain(..n);
            self.events_dropped += n;
        }
        d
    }

    fn track_states(&mut self) {
        let w = &self.s.world;
        let f = w.frame;
        for e in &w.ents {
            if let Body::Enemy(en) = &e.body {
                if e.removed {
                    continue;
                }
                if let Some(top) = en.ai.stack.last() {
                    let cur = self.state_since.get(&e.id);
                    if cur.is_none_or(|c| c.0 != top.state || c.1 != top.serial) {
                        self.state_since.insert(e.id, (top.state, top.serial, f.saturating_sub(1)));
                    }
                }
            }
        }
    }

    /// frames the enemy has been in its current top AI state (None: unknown, e.g. right after a restore)
    pub fn in_state(&self, id: u32) -> Option<u32> {
        let (_, e) = self.s.world.enemy_by_id(id)?;
        let top = e.ai.stack.last()?;
        let c = self.state_since.get(&id)?;
        (c.0 == top.state && c.1 == top.serial).then(|| self.s.world.frame - c.2)
    }

    fn reset_transient(&mut self) {
        self.events.clear();
        self.events_dropped = 0;
        self.last_events.clear();
        self.prev_pos.clear();
    }

    /// go back to frame `target` of the current branch (later inputs are dropped)
    pub fn rewind_to(&mut self, target: u32) {
        let target = target.min(self.frame());
        self.keyframes.retain(|k| k.0 <= target);
        let (f0, snap) = match self.keyframes.last() {
            Some((f, s)) => (*f, s.clone()),
            None => (0, self.initial.clone()),
        };
        self.restore_snap(&snap);
        let rec = self.record;
        self.record = false;
        let log: Vec<Held> = self.inputs[f0 as usize..target as usize].to_vec();
        self.inputs.truncate(f0 as usize);
        for h in log {
            self.step_held(h, 0);
        }
        self.record = rec;
        self.reset_transient();
    }

    pub fn save(&mut self, name: &str) {
        let sn = self.snap();
        self.named.insert(name.to_string(), (sn, self.inputs.clone()));
    }

    pub fn load(&mut self, name: &str) -> Result<(), String> {
        let (snap, inputs) = self.named.get(name).cloned().ok_or_else(|| format!("no saved state \"{name}\" (have: {})", self.named.keys().cloned().collect::<Vec<_>>().join(", ")))?;
        self.restore_with(&snap, inputs);
        Ok(())
    }

    /// restore a snapshot taken after `inputs` (its own input log, possibly another branch)
    pub fn restore_with(&mut self, snap: &Snap, inputs: Vec<Held>) {
        self.restore_snap(snap);
        // keyframes belong to the previous branch: keep only those on the common input prefix
        let common = self.inputs.iter().zip(inputs.iter()).take_while(|(a, b)| a == b).count() as u32;
        self.keyframes.retain(|k| k.0 <= common);
        if common < self.frame() {
            self.state_since.clear();
        }
        self.inputs = inputs;
        self.reset_transient();
    }

    /// a copy of this session (same state, inputs, saves) under a new id
    pub fn fork(&self, id: String) -> Agent {
        let mut a = Agent::from_session(id, self.make_session(), self.field_cfg.clone(), self.label.clone(), self.spec.clone());
        if let Some(st) = &self.story {
            // (the host compiles the same scripts; its state comes with the snapshot below)
            let host = wf_import::StoryHost::new(&self.s.data, &st.files).expect("the scripts compiled for this session");
            a.story = Some(Story { host, files: Arc::clone(&st.files), start: st.start.clone() });
        }
        a.restore_snap(&self.snap());
        a.inputs = self.inputs.clone();
        a.initial = self.initial.clone();
        a.keyframes = self.keyframes.clone();
        a.named = self.named.clone();
        a.content = self.content.clone();
        a
    }

    /// look ahead `frames` frames from the current state with `slot` holding `hold` and every other slot idle,
    /// without changing anything: the first frame the slot's player loses HP -> (frames from now, attacker, HP)
    pub fn forecast(&self, slot: usize, frames: u32, hold: u16) -> Option<(u32, String, i32)> {
        // (story sessions: the look-ahead would need its own script host; not forecast)
        if self.story.is_some() {
            return None;
        }
        let me = self.player_id(slot)?;
        let mut sc = self.scratch.borrow_mut();
        if sc.is_none() {
            *sc = Some(self.make_session());
        }
        let t = sc.as_mut().unwrap();
        t.restore(&self.s.snapshot());
        let mut held: Held = [0; MAX_PLAYERS];
        held[slot] = hold;
        let hp = |s: &Session| s.world.find(me).map(|i| s.world.pl_ref(i).hp()).unwrap_or(0);
        for k in 1..=frames {
            let h0 = hp(t);
            let evs = t.step(&held);
            let attacker = evs.iter().find_map(|e| match e {
                Event::Damage { target, attacker, .. } if *target == me => Some(*attacker),
                _ => None,
            });
            let h1 = hp(t);
            if h1 < h0 {
                let by = attacker.map(|a| describe(&t.world, a, Some(me))).unwrap_or_else(|| "?".into());
                return Some((k, by, h0 - h1));
            }
            if t.outcome != Outcome::Running {
                return None;
            }
        }
        None
    }

    pub fn replay(&self) -> Replay {
        let n = self.slots();
        let mut rle: Vec<(Vec<u16>, u32)> = Vec::new();
        for h in &self.inputs {
            let v = h[..n].to_vec();
            match rle.last_mut() {
                Some((m, c)) if *m == v => *c += 1,
                _ => rle.push((v, 1)),
            }
        }
        Replay {
            format: "wf-agent-replay/1".into(),
            config: self.s.config.clone(),
            field: self.field_cfg.clone(),
            story: self.story.as_ref().map(|st| st.start.clone()),
            slots: n,
            inputs: rle,
            frames: self.inputs.len() as u32,
            hash: self.hash(),
            outcome: outcome_str(self.s.outcome).into(),
            content: self.content.as_ref().map(|m| m.content_id.clone()),
            mod_dirs: self.content.as_ref().map(|m| m.dirs.iter().map(|d| d.display().to_string()).collect()).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay {
    pub format: String,
    pub config: BattleConfig,
    /// a field session's start (then `config` is unused)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<FieldConfig>,
    /// a story session's start (title screen, seed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<StoryStart>,
    pub slots: usize,
    /// run-length encoded held buttons: [[mask per slot], frames]
    pub inputs: Vec<(Vec<u16>, u32)>,
    pub frames: u32,
    pub hash: String,
    #[serde(default)]
    pub outcome: String,
    /// the mods the session ran with (ids in load order + the hash of their files); absent = no mods. play_replay
    /// refuses other content (`wf_replay::ContentId::mods_mismatch`'s message)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<wf_replay::ContentId>,
    /// the mod folders it was made with (play_replay loads them unless `mods` is given)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mod_dirs: Vec<String>,
}

impl Replay {
    pub fn held(&self) -> Vec<Held> {
        let mut out = Vec::with_capacity(self.frames as usize);
        for (m, c) in &self.inputs {
            let mut h: Held = [0; MAX_PLAYERS];
            for (k, v) in m.iter().enumerate().take(MAX_PLAYERS) {
                h[k] = *v;
            }
            for _ in 0..*c {
                out.push(h);
            }
        }
        out
    }
}
