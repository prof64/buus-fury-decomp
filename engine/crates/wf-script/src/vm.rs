//! Exact, resumable interpreter of the original bytecode and event graphs.
//!
//! Faithful to the ROM's interpreter (`0x08004038`) instruction by instruction and to the event stepper
//! (`0x080124F8`), with one structural change: the GBA blocks a script by running a *nested main loop*
//! inside a native (`run_mode` `0x0803D8A0`); here such a native makes [`Thread::run`] return
//! [`Status::Blocked`] and the host calls [`Thread::resume`] when the engine-side action has finished.
//! The whole thread is plain data (`Clone + Serialize + PartialEq`): snapshot it every frame for rollback.
//!
//! State, as in the ROM:
//! * bytecode frame: operand stack (16 words of the 0x6C-byte interp object), loop-counter stack (4 words),
//!   pc, and a pointer to a param block P0..P3;
//! * param blocks: a top-level run owns one; nested `run_script` / `run_event` *share* the caller's block
//!   (after writing the P values they pass);
//! * G0..G3 (`0x03001058`): global words shared by every script and persisting between runs ([`Globals`]);
//! * event activation: header, current node index, param block.

use crate::bytecode::{decode_at, Op};
use crate::natives::{self, Sig};
use crate::rom::{Entry, NodeBody, ScriptId, ScriptSet};
use serde::{Deserialize, Serialize};

pub const STACK_WORDS: usize = 16;
pub const LOOP_WORDS: usize = 4;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Globals {
    pub g: [i32; 4],
}

/// Serializable script key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum ScriptKey {
    Node { event: u16, node: u16 },
    Args { event: u16, node: u16 },
    Inline(u32),
}

impl From<ScriptId> for ScriptKey {
    fn from(s: ScriptId) -> Self {
        match s {
            ScriptId::Node { event, node } => ScriptKey::Node { event, node },
            ScriptId::Args { event, node } => ScriptKey::Args { event, node },
            ScriptId::Inline(a) => ScriptKey::Inline(a),
        }
    }
}
impl From<ScriptKey> for ScriptId {
    fn from(s: ScriptKey) -> Self {
        match s {
            ScriptKey::Node { event, node } => ScriptId::Node { event, node },
            ScriptKey::Args { event, node } => ScriptId::Args { event, node },
            ScriptKey::Inline(a) => ScriptId::Inline(a),
        }
    }
}

/// What a finished bytecode frame is used for by its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Top level / nested `run_script`: result discarded (top level keeps it in `Thread::result`).
    Plain,
    /// Bytecode node of an event: a single value left on the stack selects the next node.
    EventNode,
    /// printf-argument code of a text node: the stack becomes the arguments.
    TextArgs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub script: ScriptKey,
    pub pc: usize,
    pub stack: Vec<i32>,
    pub loops: Vec<i32>,
    /// Index into `Thread::params`.
    pub params: usize,
    pub role: Role,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventAct {
    pub header: u32,
    pub event: Option<u16>,
    /// Node about to run (or running).
    pub node: u16,
    pub params: usize,
    /// A node is in progress (child frame or host node pending).
    pub in_node: bool,
    /// The host was told the event graph started (`Host::scene`, the scene object's creation).
    #[serde(default)]
    pub entered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    Code(Frame),
    Event(EventAct),
}

/// A blocked native / node waiting for the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pending {
    Native { native: u8, pushes: bool },
    Node,
    /// the host blocked on an event graph's start (`Host::scene`)
    Scene,
}

/// One running script or event invocation (with its nested activations).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub acts: Vec<Act>,
    pub params: Vec<[i32; 4]>,
    pub pending: Option<Pending>,
    /// Final operand stack of a top-level bytecode run (callers read `stack[0]`).
    pub result: Option<Vec<i32>>,
    /// Terminal value of a top-level event (stored to the caller's state word on the GBA).
    pub event_result: Option<u16>,
    pub steps: u64,
    /// Instruction budget (a 0 repeat count loops 2^32 times on the GBA).
    pub max_steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Finished,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmError {
    NoScript(ScriptKey),
    NoEvent(u32),
    BadNode { header: u32, node: u16 },
    Decode(String),
    StackUnderflow { script: ScriptKey, pc: usize },
    StackOverflow { script: ScriptKey, pc: usize },
    LoopUnderflow { script: ScriptKey, pc: usize },
    DivideByZero { script: ScriptKey, pc: usize },
    /// `run_script(idx)` on an event whose node 0 is not bytecode: the game's runner asserts.
    NotBytecode(i32),
    StepLimit,
    NotBlocked,
    Blocked,
    Host(String),
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// What a host call did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeResult {
    /// Finished now. For natives with a result, `Some(v)` is pushed (`None` pushes nothing — only native
    /// 0x8C does that on the GBA, when there is no player object).
    Done(Option<i32>),
    /// The action spans frames (the GBA ran a nested main loop). Resume with the result.
    Block,
    /// The native runs a whole event inline before the caller continues (an engine hook the game calls from
    /// inside the native, e.g. give_xp's level-up popup #3); like `run_event`, sharing the caller's param block.
    /// Only for natives that may block (queued actions in the Rhai runtime).
    RunEvent(u16),
}

/// Context handed to the host for one native call.
pub struct Call<'a> {
    pub native: u8,
    /// Arguments in source order (first pushed first).
    pub args: &'a [i32],
    pub script: ScriptKey,
    pub pc: usize,
    /// P0..P3 of the calling frame.
    pub params: &'a mut [i32; 4],
    pub globals: &'a mut Globals,
}

/// An event graph starting / ending (`0x08012588` run_scene: the scene object's creation broadcasts world message
/// 0x0F, its end 0x10). The original's first node runs one frame after the start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneCall {
    Enter { event: Option<u16> },
    Exit { event: Option<u16> },
}

/// A text box or choice node of an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeCall {
    Text { event: Option<u16>, node: u16, kind: u8, arg: u8, text: String, ref_: u32, args: Vec<i32> },
    Choice { event: Option<u16>, node: u16, arg: u8, text: String },
}

/// Engine effects. Everything the original natives and event nodes do to the game goes through here.
pub trait Host {
    /// Any native the VM does not implement itself (all but G/P access, `run_script`, `run_event`).
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String>;
    /// Setter `n >= 8` (game variables that scripts assign; see `script_setters.json`).
    fn set_var(&mut self, var: u8, value: i32, params: &mut [i32; 4], globals: &mut Globals) -> Result<(), String>;
    /// Text box / choice node. Usually blocks; the result value is ignored (a choice stores its selection
    /// host-side, read back by native 0x18).
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String>;
    /// An event graph starts (may block: the original runs its first node a frame later) or ends.
    fn scene(&mut self, call: SceneCall) -> Result<NativeResult, String> {
        let _ = call;
        Ok(NativeResult::Done(None))
    }
    /// Remake extensions beyond the 248 originals (multiplayer API: `player_count`, `player(n)`,
    /// `give_item_to(n, i)`, …; see [`EXTENSIONS`]). Original bytecode never calls these.
    fn ext(&mut self, call: ExtCall<'_>) -> Result<NativeResult, String> {
        Err(format!("extension `{}` not supported by this host", call.name))
    }
}

/// A call to a remake extension.
pub struct ExtCall<'a> {
    pub name: &'a str,
    pub args: &'a [i32],
    /// The player that triggered the running handler (1-based; 0 = none / engine).
    pub who: i32,
    pub params: &'a mut [i32; 4],
    pub globals: &'a mut Globals,
}

/// Kind of an extension function (same rules as natives: queries need an empty action queue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtKind {
    /// Returns a value, no effect.
    Query,
    /// Effect without a frame span (runs at once when the queue is empty, else queued).
    Instant,
    /// Effect that may span frames (queued; the host may block).
    Action,
}

/// The multiplayer extension API: (name, arity, kind). Players are 1-based; `player(1)` is the leader, whose
/// actor is the original actor 0. No function exposes the *local* peer (rollback determinism).
pub const EXTENSIONS: &[(&str, u8, ExtKind)] = &[
    ("player_count", 0, ExtKind::Query),
    ("player", 1, ExtKind::Query),            // actor id of player n
    ("player_of_actor", 1, ExtKind::Query),   // 1-based player index controlling actor id, 0 if none
    ("player_character", 1, ExtKind::Query),  // character index played by player n
    ("player_hp", 1, ExtKind::Query),
    ("player_max_hp", 1, ExtKind::Query),
    ("player_energy", 1, ExtKind::Query),
    ("player_x", 1, ExtKind::Query),
    ("player_y", 1, ExtKind::Query),
    ("player_button", 2, ExtKind::Query),     // (n, button) held this frame: 0 A, 1 B, 2 L, 3 R, 4 start
    ("player_pressed", 2, ExtKind::Query),    // (n, button) pressed this frame
    ("player_alive", 1, ExtKind::Query),
    ("set_player_hp", 2, ExtKind::Instant),
    ("set_player_energy", 2, ExtKind::Instant),
    ("set_player_character", 2, ExtKind::Instant),
    ("give_item_to", 2, ExtKind::Action),     // (n, item): the pickup popup is per player
    ("give_equipment_to", 2, ExtKind::Action),
    ("give_xp_to", 2, ExtKind::Action),
    ("revive_player", 1, ExtKind::Action),
    ("warp_player", 6, ExtKind::Action),      // (n, world, map, variant, x, y)
    ("gather_players", 2, ExtKind::Action),   // (x, y): bring every player next to (x, y) (cutscene start)
    ("say_to", 3, ExtKind::Action),           // (n, speaker_portrait, string_id): text box for one player only
];

pub fn ext_info(name: &str) -> Option<(u8, ExtKind)> {
    EXTENSIONS.iter().find(|e| e.0 == name).map(|e| (e.1, e.2))
}

impl Thread {
    fn empty(params: [i32; 4]) -> Thread {
        Thread { acts: Vec::new(), params: vec![params], pending: None, result: None, event_result: None, steps: 0, max_steps: 50_000_000 }
    }

    /// Run a bytecode unit (inline script / the runner on node 0) with its own param block.
    pub fn new(script: ScriptId, params: [i32; 4]) -> Thread {
        let mut t = Thread::empty(params);
        t.acts.push(Act::Code(Frame {
            script: script.into(),
            pc: 0,
            stack: Vec::new(),
            loops: Vec::new(),
            params: 0,
            role: Role::Plain,
        }));
        t
    }

    /// Run a whole event (`0x08012588` run_event) with its own param block.
    pub fn new_event(set: &ScriptSet, event: u16, params: [i32; 4]) -> Result<Thread, VmError> {
        let ev = set.events.get(&event).ok_or(VmError::NoEvent(event as u32))?;
        let mut t = Thread::empty(params);
        t.acts.push(Act::Event(EventAct { header: ev.header, event: Some(event), node: 0, params: 0, in_node: false, entered: false }));
        Ok(t)
    }

    pub fn is_finished(&self) -> bool {
        self.acts.is_empty()
    }

    /// Deliver the result of a blocked native / node and continue running.
    pub fn resume(
        &mut self,
        value: Option<i32>,
        set: &ScriptSet,
        g: &mut Globals,
        host: &mut dyn Host,
    ) -> Result<Status, VmError> {
        match self.pending.take().ok_or(VmError::NotBlocked)? {
            Pending::Native { pushes, .. } => {
                if pushes {
                    if let (Some(v), Some(Act::Code(f))) = (value, self.acts.last_mut()) {
                        push(f, v)?;
                    }
                }
            }
            Pending::Node => self.node_done(None),
            Pending::Scene => {}
        }
        self.run(set, g, host)
    }

    /// The event on top finished its current node; `next` overrides `node.next` (already stored).
    fn node_done(&mut self, next: Option<i32>) {
        if let Some(Act::Event(e)) = self.acts.last_mut() {
            e.in_node = false;
            if let Some(n) = next {
                e.node = n as u16; // strh: truncated to 16 bits
            }
        }
    }

    /// Run until everything finished or the host blocked.
    pub fn run(&mut self, set: &ScriptSet, g: &mut Globals, host: &mut dyn Host) -> Result<Status, VmError> {
        if self.pending.is_some() {
            return Err(VmError::Blocked);
        }
        loop {
            self.steps += 1;
            if self.steps > self.max_steps {
                return Err(VmError::StepLimit);
            }
            match self.acts.last() {
                None => return Ok(Status::Finished),
                Some(Act::Event(_)) => {
                    if self.step_event(set, host)? {
                        return Ok(Status::Blocked);
                    }
                }
                Some(Act::Code(_)) => {
                    if self.step_code(set, g, host)? {
                        return Ok(Status::Blocked);
                    }
                }
            }
        }
    }

    /// One event step. Returns true if blocked.
    fn step_event(&mut self, set: &ScriptSet, host: &mut dyn Host) -> Result<bool, VmError> {
        let Some(Act::Event(e)) = self.acts.last_mut() else { unreachable!() };
        if e.in_node {
            unreachable!("event resumed while its node is in progress");
        }
        if !e.entered {
            e.entered = true;
            let event = e.event;
            if let NativeResult::Block = host.scene(SceneCall::Enter { event }).map_err(VmError::Host)? {
                self.pending = Some(Pending::Scene);
                return Ok(true);
            }
            return Ok(false);
        }
        let header = e.header;
        let ev = set
            .headers
            .get(&header)
            .and_then(|i| set.events.get(i))
            .ok_or(VmError::NoEvent(header))?;
        let entry = ev.entries.get(&e.node).ok_or(VmError::BadNode { header, node: e.node })?;
        match entry {
            Entry::Terminal(v) => {
                let v = *v;
                let event = ev.index.into();
                host.scene(SceneCall::Exit { event }).map_err(VmError::Host)?;
                self.acts.pop();
                if self.acts.is_empty() {
                    self.event_result = Some(v);
                }
                // a parent bytecode frame (run_event native) simply continues
            }
            Entry::Node(n) => {
                let (idx, params) = (e.node, e.params);
                e.node = n.next;
                e.in_node = true;
                match &n.body {
                    NodeBody::Code(id) => {
                        self.acts.push(Act::Code(Frame {
                            script: (*id).into(),
                            pc: 0,
                            stack: vec![],
                            loops: vec![],
                            params,
                            role: Role::EventNode,
                        }));
                    }
                    NodeBody::TextArgs { args, .. } => {
                        self.acts.push(Act::Code(Frame {
                            script: (*args).into(),
                            pc: 0,
                            stack: vec![],
                            loops: vec![],
                            params,
                            role: Role::TextArgs,
                        }));
                    }
                    NodeBody::Text { text, ref_ } => {
                        let call = NodeCall::Text {
                            event: ev.index.into(),
                            node: idx,
                            kind: n.kind,
                            arg: n.arg,
                            text: text.clone(),
                            ref_: *ref_,
                            args: vec![],
                        };
                        return self.host_node(host, call);
                    }
                    NodeBody::Choice { text } => {
                        let call = NodeCall::Choice { event: ev.index.into(), node: idx, arg: n.arg, text: text.clone() };
                        return self.host_node(host, call);
                    }
                    NodeBody::Goto { header: h, event } => {
                        let Some(Act::Event(e)) = self.acts.last_mut() else { unreachable!() };
                        e.header = *h;
                        e.event = *event;
                        e.node = 0;
                        e.in_node = false;
                    }
                    NodeBody::Unknown { .. } => return Err(VmError::BadNode { header, node: idx }),
                }
            }
        }
        Ok(false)
    }

    fn host_node(&mut self, host: &mut dyn Host, call: NodeCall) -> Result<bool, VmError> {
        match host.node(call).map_err(VmError::Host)? {
            // nodes never run an event inline
            NativeResult::Done(_) | NativeResult::RunEvent(_) => {
                self.node_done(None);
                Ok(false)
            }
            NativeResult::Block => {
                self.pending = Some(Pending::Node);
                Ok(true)
            }
        }
    }

    /// A bytecode frame executed `end`.
    fn finish_frame(&mut self, set: &ScriptSet, host: &mut dyn Host) -> Result<bool, VmError> {
        let Some(Act::Code(f)) = self.acts.pop() else { unreachable!() };
        match f.role {
            Role::Plain => {
                if self.acts.is_empty() {
                    self.result = Some(f.stack);
                }
            }
            Role::EventNode => {
                let next = if f.stack.len() == 1 { Some(f.stack[0]) } else { None };
                self.node_done(next);
            }
            Role::TextArgs => {
                // the text node itself: show it with the computed arguments
                let Some(Act::Event(e)) = self.acts.last() else { unreachable!() };
                let ev = set.headers.get(&e.header).and_then(|i| set.events.get(i)).ok_or(VmError::NoEvent(e.header))?;
                let ScriptKey::Args { node, .. } = f.script else { unreachable!() };
                let Some(Entry::Node(n)) = ev.entries.get(&node) else { unreachable!() };
                let NodeBody::TextArgs { text, ref_, .. } = &n.body else { unreachable!() };
                let call = NodeCall::Text {
                    event: Some(ev.index),
                    node,
                    kind: n.kind,
                    arg: n.arg,
                    text: text.clone(),
                    ref_: *ref_,
                    args: f.stack,
                };
                return self.host_node(host, call);
            }
        }
        Ok(false)
    }

    /// Execute one instruction of the top bytecode frame. Returns true if blocked.
    fn step_code(&mut self, set: &ScriptSet, g: &mut Globals, host: &mut dyn Host) -> Result<bool, VmError> {
        let Some(Act::Code(f)) = self.acts.last_mut() else { unreachable!() };
        let sc = set.get(f.script.into()).ok_or(VmError::NoScript(f.script))?;
        let ins = match sc.insns.get(&f.pc) {
            Some(i) => i.clone(),
            None => decode_at(&sc.code, f.pc).map_err(|e| VmError::Decode(e.to_string()))?,
        };
        let at = f.pc;
        f.pc += ins.len;
        match ins.op {
            Op::PushU8(v) => push(f, v as i32)?,
            Op::Push(v) => push(f, v)?,
            Op::PushN(ref vs) => {
                for &v in vs {
                    push(f, v)?
                }
            }
            Op::Native(n) => return self.call_native(n, at, set, g, host),
            Op::Set(n) => {
                let v = pop(f, at)?;
                self.setter(n, v, g, host)?;
            }
            Op::And | Op::Or | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Eq | Op::Ne | Op::Gt | Op::Ge
            | Op::Lt | Op::Le => {
                let top = pop(f, at)?;
                let sec = pop(f, at)?;
                let r = match ins.op {
                    Op::And => (sec != 0 && top != 0) as i32,
                    Op::Or => (sec != 0 || top != 0) as i32,
                    Op::Add => sec.wrapping_add(top),
                    Op::Sub => sec.wrapping_sub(top),
                    Op::Mul => sec.wrapping_mul(top),
                    Op::Div | Op::Mod => {
                        if top == 0 {
                            return Err(VmError::DivideByZero { script: f.script, pc: at });
                        }
                        if ins.op == Op::Div {
                            sec.wrapping_div(top)
                        } else {
                            sec.wrapping_rem(top)
                        }
                    }
                    Op::Eq => (sec == top) as i32,
                    Op::Ne => (sec != top) as i32,
                    Op::Gt => (sec > top) as i32,
                    Op::Ge => (sec >= top) as i32,
                    Op::Lt => (sec < top) as i32,
                    _ => (sec <= top) as i32,
                };
                push(f, r)?;
            }
            Op::Neg => {
                let v = pop(f, at)?;
                push(f, v.wrapping_neg())?;
            }
            Op::End => return self.finish_frame(set, host),
            Op::Jmp(t) => f.pc = t,
            Op::Jz(t) => {
                if pop(f, at)? == 0 {
                    f.pc = t;
                }
            }
            Op::Pop => {
                pop(f, at)?;
            }
            Op::Inc(n) | Op::PreInc(n) | Op::PostInc(n) | Op::Dec(n) | Op::PreDec(n) | Op::PostDec(n) => {
                // handler 0x08003E84..: call getter native n (pushes the value), then setter n with old±1
                if self.call_native(n, at, set, g, host)? {
                    return Err(VmError::Host(format!("getter 0x{n:02x} blocked")));
                }
                let Some(Act::Code(f)) = self.acts.last_mut() else { unreachable!() };
                let old = pop(f, at)?;
                let (newv, keep) = match ins.op {
                    Op::Inc(_) => (old.wrapping_add(1), None),
                    Op::PreInc(_) => (old.wrapping_add(1), Some(old.wrapping_add(1))),
                    Op::PostInc(_) => (old.wrapping_add(1), Some(old)),
                    Op::Dec(_) => (old.wrapping_sub(1), None),
                    Op::PreDec(_) => (old.wrapping_sub(1), Some(old.wrapping_sub(1))),
                    _ => (old.wrapping_sub(1), Some(old)),
                };
                if let Some(k) = keep {
                    push(f, k)?;
                }
                self.setter(n, newv, g, host)?;
            }
            Op::LoopPush => {
                let v = pop(f, at)?;
                if f.loops.len() >= LOOP_WORDS {
                    return Err(VmError::StackOverflow { script: f.script, pc: at });
                }
                f.loops.push(v);
            }
            Op::Loop(t) => {
                let c = f.loops.last_mut().ok_or(VmError::LoopUnderflow { script: f.script, pc: at })?;
                *c = c.wrapping_sub(1);
                if *c != 0 {
                    f.pc = t;
                } else {
                    f.loops.pop();
                }
            }
        }
        Ok(false)
    }

    fn cur_params(&mut self) -> &mut [i32; 4] {
        let Some(Act::Code(f)) = self.acts.last() else { unreachable!() };
        let i = f.params;
        &mut self.params[i]
    }

    fn setter(&mut self, n: u8, v: i32, g: &mut Globals, host: &mut dyn Host) -> Result<(), VmError> {
        match n {
            0..=3 => g.g[n as usize] = v,
            4..=7 => self.cur_params()[n as usize - 4] = v,
            _ => {
                let Some(Act::Code(f)) = self.acts.last() else { unreachable!() };
                let i = f.params;
                host.set_var(n, v, &mut self.params[i], g).map_err(VmError::Host)?
            }
        }
        Ok(())
    }

    /// Execute native `n`. Returns true if the thread blocked.
    fn call_native(
        &mut self,
        n: u8,
        at: usize,
        set: &ScriptSet,
        g: &mut Globals,
        host: &mut dyn Host,
    ) -> Result<bool, VmError> {
        let Sig { pops, pushes } = natives::sig(n);
        let Some(Act::Code(f)) = self.acts.last_mut() else { unreachable!() };
        if f.stack.len() < pops as usize {
            return Err(VmError::StackUnderflow { script: f.script, pc: at });
        }
        let args: Vec<i32> = f.stack.split_off(f.stack.len() - pops as usize);
        let pi = f.params;
        match n {
            0..=3 => push(f, g.g[n as usize])?,
            4..=7 => {
                let v = self.params[pi][n as usize - 4];
                let Some(Act::Code(f)) = self.acts.last_mut() else { unreachable!() };
                push(f, v)?
            }
            natives::RUN_SCRIPT | natives::RUN_SCRIPT_P0 | natives::RUN_SCRIPT_P0_P1 => {
                // nested runner 0x08004098 on node 0 of event idx, sharing the caller's param block
                for (k, &v) in args[1..].iter().enumerate() {
                    self.params[pi][k] = v;
                }
                let id = set.run_script_target(args[0]).ok_or(VmError::NotBytecode(args[0]))?;
                self.acts.push(Act::Code(Frame {
                    script: id.into(),
                    pc: 0,
                    stack: vec![],
                    loops: vec![],
                    params: pi,
                    role: Role::Plain,
                }));
            }
            natives::RUN_EVENT..=natives::RUN_EVENT_P0_P3 => {
                // 0x08012588: write P0..Pk of the current block, run the whole event with it (blocks the caller)
                for (k, &v) in args[1..].iter().enumerate() {
                    self.params[pi][k] = v;
                }
                let idx = u16::try_from(args[0]).map_err(|_| VmError::NoEvent(args[0] as u32))?;
                let ev = set.events.get(&idx).ok_or(VmError::NoEvent(idx as u32))?;
                self.acts.push(Act::Event(EventAct {
                    header: ev.header,
                    event: Some(idx),
                    node: 0,
                    params: pi,
                    in_node: false,
                    entered: false,
                }));
            }
            _ => {
                let script = f.script;
                let r = host
                    .native(Call { native: n, args: &args, script, pc: at, params: &mut self.params[pi], globals: g })
                    .map_err(VmError::Host)?;
                match r {
                    NativeResult::Done(v) => {
                        if pushes {
                            if let Some(v) = v {
                                let Some(Act::Code(f)) = self.acts.last_mut() else { unreachable!() };
                                push(f, v)?;
                            }
                        }
                    }
                    NativeResult::Block => {
                        self.pending = Some(Pending::Native { native: n, pushes });
                        return Ok(true);
                    }
                    NativeResult::RunEvent(idx) => {
                        let ev = set.events.get(&idx).ok_or(VmError::NoEvent(idx as u32))?;
                        self.acts.push(Act::Event(EventAct {
                            header: ev.header,
                            event: Some(idx),
                            node: 0,
                            params: pi,
                            in_node: false,
                            entered: false,
                        }));
                    }
                }
            }
        }
        Ok(false)
    }
}

fn push(f: &mut Frame, v: i32) -> Result<(), VmError> {
    if f.stack.len() >= STACK_WORDS {
        return Err(VmError::StackOverflow { script: f.script, pc: f.pc });
    }
    f.stack.push(v);
    Ok(())
}

fn pop(f: &mut Frame, at: usize) -> Result<i32, VmError> {
    f.stack.pop().ok_or(VmError::StackUnderflow { script: f.script, pc: at })
}
