//! Enemy AI interpreter (content `ai/state_*.json`, decoded from the ROM): an AI stack of states (obj+0x238) whose
//! top state's behaviour tree runs once per frame, plus the on-hit ("aux") tree of the top state evaluated when the
//! enemy is hit. Semantics: re/buu/notes.md §8 / §8b and the reference interpreter (site/.../buu/ai.ts), ported
//! node for node.
//!
//! Representation: each state's tree is compiled once into a flat node array (`SubState` inlined, as the reference
//! builds it); every push of a state instantiates a fresh runtime array (`NodeRt` per node) - plain data, so the
//! enemy (and the world) stays `Clone + Hash`. Node code addresses its runtime through (stack level, entry serial,
//! node index): writes after the entry was replaced or popped are dropped, which is what the reference's orphaned
//! runtime objects amount to.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

macro_rules! kinds {
    ($($k:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum K { $($k,)* Never }
        impl K {
            pub fn from_name(s: &str) -> K {
                match s { $(stringify!($k) => K::$k,)* _ => K::Never }
            }
        }
    };
}

kinds!(
    Sequence, Selector, ParallelAll, ParallelAny, GotoState, PushState, PopState, SubState,
    // conditions
    Chance, IfPlayerWithin, IfPlayerWithin2, IfPlayerFarther, IfPlayerInMeleeRange, IfPlayerAttackingMe, IfInPlayerBeamLine,
    IfTimeInState, IfCooldownOver, IfCooldown2Over, IfCounterEq, IfGlobalFlag, IfBuuPacified, IfAxisAligned, IfHpBelow,
    IfPlayerInFront, IfPlayerInZone, IfPlayerInRoomZone, IfPlayerBlastIncoming, IfPlayerInLineOfFire, IfContactHit, IfMeleeHit,
    IfContactExplode, IfLandedOnRock, IfPlayerPetrified, IfPlayerNotPetrified, IfSphereDestroyed, IfPlayerNearAndHittable,
    IfLastKidBuu, IfChildCount, IfLatchOverlap, IfButtonMashed, IfPlayerOnTop, IfPartnerHolding, IfUnknown,
    // simple actions
    Succeed, Idle, RunMotionController, PartMirrorAnim, StandStill, FacePlayer, Wait, MarkTime, MarkTime2, SetCounter,
    IncCounter, SelfMessage11, RunScript, InlineScript, StaggerMinions, SpawnPuff, VampireSummon, VampireSummon2,
    SelfDestruct, TriggerSelf, Block,
    // movement
    ChasePlayer, ChasePlayerThroughRocks, BackOffFacingPlayer, BackOffFacingPlayerThroughRocks, StepAwayFromPlayer,
    WalkToPlayer, FlyToPlayer, AlignWithPlayer, LineUpForShot, Wander, MoveToPoint, WanderToRandomPoint, CirclePlayer,
    HoverDrift, BackStep, KnockbackSlide, LungeStep, FollowMajinBuu, TankDrive, RandomStep,
    // melee
    MeleeAttack, BioMechPunch, HeavyPunch, WindupMelee, HeavyStrike, ChargedPunch, MiniYakonLunge, DriftContact,
    // ranged
    KiBlast, GunShot, RifleShot, NinjaStarThrow, MummyWrapShot, MummyWrapBurst, TankShoot, SoldierThrow, CyborgShot,
    TurretFire, TurretOpen, TurretClose, BombThrow, FlameBreath, GrappleArm, EnergyBarrage, FinalFlash, MouthBeamDown,
    KidBuuKamehameha, StoneSpit, AimAtPlayer,
    // teleports
    NinjaTeleport, TeleportNearPlayer, TeleportChain4, NinjaDisguise, NinjaReveal,
    // grabs
    VampireGrab, LatchDrain, GrabWindup, BearHug, EnergyDrain, FreezePlayer, EatCandy,
    // Buu
    BuuStretchPunch, CandyBeam, AbsorbStance, AbsorbPose, AbsorbPose2, SuperBuuPowerUp, ThrowBigBall, GlideToWaypoint,
    SummonMinion, KidBuuSplitAnim, KidBuuSplit, KidBuuMerge, BabidiHover, BabidiGesture,
    // Janemba
    JanembaSpawnParts, JanembaHandStrikeA, JanembaHandStrikeB, JanembaAscend, JanembaDescendSummon, WaitMinisDefeated,
    MiniHopToward, MiniHopFlee, MiniRollAttack, HandAmbush, HandSink, SuperJanembaWave, JanembaPartFlash,
    // others
    BrolySpawnRocks, JumpSmash, FinishLandingAnim, StunnedAnim, StunnedAnim21, PlayAnimHold, RecoverAnim, PlayAnim0,
    PlayAnim12, PlayAnim12b, PlayAnim12Swarm, PlayAnim12Yakon, PlayAnim12Yakon2, PlayAnim13, PlayAnim13Yakon, PlayAnimOnce,
    PlayAnimTimed, DarkSphere, TauntAnim, Hop, SummonMiniYakons, WaitSummonsDead, SpawnEnzymes, MechTransform, RaiseShield,
    LowerShield, ShieldBreak, GhostAnimA, GhostAnimB, WaitFormationSlot, MoveToFormationA, MoveToFormationB, WormSpawnBody,
    WormHeadMove, WormBodyFollow,
    // reaction states
    StunHold, DeathBlink, ShockStun, VolleyballTransform, VolleyballRestore, VolleyballLaunch,
    // on-hit only
    IfAttack, ApplyDamage, IfHitFromFront, SetHitDir, HealFraction, FreezeHelperHit, ShieldHit,
    // mods (modkit.rs): a condition / action node whose code is a Rhai function (`call`: "file.rhai:fn")
    ModCondition, ModAction,
);

/// node names whose content label does not match what the game does (emulator-verified): 0x08018E29 moves AWAY
/// from the player while facing it
fn rename(s: &str) -> &str {
    match s {
        "StepTowardPlayer" => "BackOffFacingPlayer",
        "StepTowardPlayerThroughRocks" => "BackOffFacingPlayerThroughRocks",
        x => x,
    }
}

const PARAMS: [&str; 15] = ["state", "speed", "p", "r2", "frames", "n", "frac", "turn_p", "x", "y", "k", "dist", "atk", "gap", "presses"];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P {
    State,
    Speed,
    Prob,
    R2,
    Frames,
    N,
    Frac,
    TurnP,
    X,
    Y,
    K,
    Dist,
    Atk,
    Gap,
    Presses,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Params {
    vals: [i64; 15],
    has: u32,
}
impl Params {
    pub fn get(&self, p: P, d: i64) -> i64 {
        let i = p as usize;
        if self.has & (1 << i) != 0 {
            self.vals[i]
        } else {
            d
        }
    }
    pub fn g(&self, p: P) -> i64 {
        self.get(p, 0)
    }
    pub fn set(&mut self, p: P, v: i64) {
        self.vals[p as usize] = v;
        self.has |= 1 << p as usize;
    }
}

#[derive(Clone, Debug)]
pub struct CNode {
    pub k: K,
    pub name: String,
    pub addr: u32,
    pub p: Params,
    pub kids: Vec<u16>,
    /// RunScript: the queued event's header address (def+0xC); InlineScript: the inline bytecode (def+0xC)
    pub script: u32,
    /// ModCondition / ModAction: the hook (`"scripts/ai/x.rhai:fn"`) and its integer arguments (`args`)
    pub call: String,
    pub args: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct BehTree {
    pub state: u16,
    pub nodes: Vec<CNode>,
}
pub type HitTree = BehTree;

fn params_of(v: &Value) -> Params {
    let mut p = Params::default();
    if let Some(o) = v.get("params").and_then(|x| x.as_object()) {
        for (k, x) in o {
            if let Some(i) = PARAMS.iter().position(|n| n == k) {
                p.vals[i] = crate::content::int(&x["raw"]);
                p.has |= 1 << i;
            }
        }
    }
    p
}

fn push_node(out: &mut Vec<CNode>, v: &Value, all: Option<&BTreeMap<u16, Value>>, depth: u32) -> u16 {
    let raw_name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
    let name = rename(raw_name);
    let p = params_of(v);
    // SubState in a behaviour tree: the referenced state's tree, built in place
    if let (Some(all), true) = (all, name == "SubState") {
        let st = p.g(P::State) as u16;
        if let (Some(t), true) = (all.get(&st), depth < 24) {
            return push_node(out, t, Some(all), depth + 1);
        }
        let idx = out.len() as u16;
        out.push(CNode { k: K::Never, name: format!("missing state 0x{st:x}"), addr: 0, p, kids: vec![], script: 0, call: String::new(), args: vec![] });
        return idx;
    }
    let idx = out.len() as u16;
    let addr = crate::content::int(v.get("addr").unwrap_or(&Value::Null)) as u32;
    // RunScript's <script> param (a header address); InlineScript's bytecode sits at def+0xC
    let script = match name {
        "RunScript" => v.get("params").and_then(|x| x.get("script")).map(|x| crate::content::int(&x["raw"]) as u32).unwrap_or(0),
        "InlineScript" if addr != 0 => addr + 0xC,
        _ => 0,
    };
    let call = v.get("call").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let args = v.get("args").and_then(|x| x.as_array()).map(|a| a.iter().map(|x| crate::content::int(x) as i32).collect()).unwrap_or_default();
    out.push(CNode { k: K::from_name(name), name: name.to_string(), addr, p, kids: vec![], script, call, args });
    let mut kids = Vec::new();
    for c in v.get("children").and_then(|x| x.as_array()).into_iter().flatten() {
        kids.push(push_node(out, c, all, depth));
    }
    out[idx as usize].kids = kids;
    idx
}

pub fn compile_behaviour(id: u16, all: &BTreeMap<u16, Value>) -> BehTree {
    let mut nodes = Vec::new();
    if let Some(v) = all.get(&id) {
        push_node(&mut nodes, v, Some(all), 0);
    }
    if nodes.is_empty() {
        nodes.push(CNode { k: K::Never, name: format!("missing state 0x{id:x}"), addr: 0, p: Params::default(), kids: vec![], script: 0, call: String::new(), args: vec![] });
    }
    BehTree { state: id, nodes }
}

pub fn compile_onhit(v: &Value) -> HitTree {
    let mut nodes = Vec::new();
    push_node(&mut nodes, v, None, 0);
    BehTree { state: 0, nodes }
}

// ---------------------------------------------------------------------------------------------------------------
// runtime

/// path follower state (path.rs), embedded in the node that owns it
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathRt {
    pub speed: i32,
    pub budget: i32,
    pub wx: i32,
    pub wy: i32,
    pub valid: i32,
    pub counter: i32,
    pub gx: i32,
    pub gy: i32,
}

/// per-node runtime: frame counter `f` of actions, composite cursor `i`, the decorator's `on`, parallel `done`
/// bits, scratch `d` (the reference's per-action data record) and an embedded path follower
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRt {
    pub f: i32,
    pub i: i32,
    pub on: bool,
    pub done: u64,
    pub d: [i32; 8],
    pub path: PathRt,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StackEntry {
    pub state: u16,
    pub serial: u32,
    /// the state's compiled tree (static data; re-attached from the game data after a deserialize)
    #[serde(skip)]
    pub tree: Arc<BehTree>,
    pub rt: Vec<NodeRt>,
}
impl Hash for StackEntry {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.state.hash(h);
        self.serial.hash(h);
        self.rt.hash(h);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AiCtl {
    pub stack: Vec<StackEntry>,
    pub gen: u32,
    /// entries replaced / popped while a tree was being evaluated: a GotoState does not stop the running tree (it
    /// finishes its update on its own runtime), so their runtimes stay addressable until that evaluation ends
    // (no skip_serializing_if: postcard byte snapshots are not self-describing, a skipped field breaks restore)
    #[serde(default)]
    pub orphans: Vec<StackEntry>,
}
impl Hash for AiCtl {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.stack.hash(h);
        self.gen.hash(h);
        // empty between frames (only live inside one evaluation)
        if !self.orphans.is_empty() {
            self.orphans.hash(h);
        }
    }
}

impl AiCtl {
    pub fn top_state(&self) -> Option<u16> {
        self.stack.last().map(|s| s.state)
    }
    pub fn states(&self) -> Vec<u16> {
        self.stack.iter().map(|s| s.state).collect()
    }
}

pub fn new_rt(tree: &BehTree) -> Vec<NodeRt> {
    tree.nodes
        .iter()
        .map(|n| {
            let mut r = NodeRt::default();
            let speed = match n.k {
                K::WanderToRandomPoint => 0xb3,
                _ => n.p.g(P::Speed) as i32,
            };
            r.path = PathRt { speed, budget: 50, ..Default::default() };
            r
        })
        .collect()
}
