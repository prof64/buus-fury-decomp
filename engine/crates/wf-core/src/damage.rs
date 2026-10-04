//! Damage: Character::ComputeDamage 0x0800E9D8 (re/buu/notes.md §4), same integer steps as the game (64-bit
//! intermediates; floor shifts). Damage is produced in 1/1024 HP; HP loss carries the remainder (`HpAcc`).
//!
//!   melee:  crit = sub != 3 && chance(82*S + critBonus)   power += power>>1    P = (S*power*0xCCD) >> 16
//!   energy: P = power
//!   D >= 200 -> D = min(D/4 + 150, 220)
//!   X = ((221-D)*P*297) >> 16 ;  A = S + ((S*S*(S+100)) >> 13) ;  Y = (mult*X + 1023) >> 10
//!   raw = (((256 + rand(26)) * Y) >> 8) * A ;  dmg = (resist*raw + 127) >> 7
//!   blocking && dmg -> dmg = dmg*blockFactor >> 16

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// attack-type table 0x086B8088: sub id -> kind (0 melee STR vs END, 1 energy POW vs END2), power
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttackType {
    pub sub: u8,
    pub kind: u8,
    pub power: i32,
}

pub const fn atk(sub: u8) -> AttackType {
    AttackType {
        sub,
        kind: if sub == 0 || sub == 2 || sub == 3 {
            0
        } else {
            1
        },
        power: if sub == 16 { 0 } else { 1024 },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CombatStats {
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub end2: i32,
    /// obj+0x1B8 / +0x1B9, 128 = x1
    pub res_m: i32,
    pub res_e: i32,
    /// obj+0x1BC = crit% * 656
    pub crit_bonus: i32,
    /// obj+0x1C0, 0x1000 = 1/16
    pub block_factor: i32,
}

impl Default for CombatStats {
    fn default() -> Self {
        CombatStats {
            str_: 1,
            pow: 1,
            end: 0,
            end2: 0,
            res_m: 128,
            res_e: 128,
            crit_bonus: 0,
            block_factor: 0x1000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DamageResult {
    pub crit: bool,
    pub raw: i64,
    pub dmg: i64,
    /// the damage before the block factor (ComputeDamage stores it at hit+0x18, 0x0800EABE, then applies the block)
    pub unblocked: i64,
}

pub fn effective_def(d: i64) -> i64 {
    if d >= 200 {
        (d / 4 + 150).min(220)
    } else {
        d
    }
}

/// draws: crit chance (melee, sub != 3), then rand(26). `roll` overrides the RNG (formula-coverage tests).
#[allow(clippy::too_many_arguments)]
pub fn compute_damage_raw(
    kind: u8,
    sub: u8,
    power: i64,
    mult: i64,
    s: i64,
    crit_bonus: i64,
    d: i64,
    resist: i64,
    blocking: bool,
    block_factor: i64,
    rng: &mut dyn FnMut(Draw) -> i64,
) -> DamageResult {
    let mut p = power;
    let mut crit = false;
    if kind == 0 {
        if sub != 3 {
            crit = rng(Draw::Chance(0x52 * s + crit_bonus)) != 0;
            if crit {
                p += p >> 1;
            }
        }
        p = (s * p * 0xccd) >> 16;
    }
    let d = effective_def(d);
    let x = ((221 - d) * p * 297) >> 16;
    let a = s + ((s * s * (s + 100)) >> 13);
    let y = (mult * x + 1023) >> 10;
    let r = rng(Draw::Rand(26));
    let raw = (((256 + r) * y) >> 8) * a;
    let mut dmg = (resist * raw + 127) >> 7;
    let unblocked = dmg;
    if blocking && dmg != 0 {
        dmg = (dmg * block_factor) >> 16;
    }
    DamageResult {
        crit,
        raw,
        dmg,
        unblocked,
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Draw {
    Chance(i64),
    Rand(u32),
}

pub fn rng_draw(rng: &mut Rng) -> impl FnMut(Draw) -> i64 + '_ {
    move |d| match d {
        Draw::Chance(p) => rng.chance(p) as i64,
        Draw::Rand(n) => rng.rand(n) as i64,
    }
}

/// attacker / defender form: picks STR/END/resist by kind; `blocking` must include the frontal test
pub fn compute_damage(
    a: AttackType,
    mult: i32,
    att: &CombatStats,
    def: &CombatStats,
    blocking: bool,
    rng: &mut Rng,
) -> DamageResult {
    let melee = a.kind == 0;
    compute_damage_raw(
        a.kind,
        a.sub,
        a.power as i64,
        mult as i64,
        if melee { att.str_ } else { att.pow } as i64,
        att.crit_bonus as i64,
        if melee { def.end } else { def.end2 } as i64,
        if melee { def.res_m } else { def.res_e } as i64,
        blocking,
        def.block_factor as i64,
        &mut rng_draw(rng),
    )
}

/// HP loss with the 1/1024 remainder carried (player 0x08010BF2)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HpAcc {
    pub acc: i64,
}
impl HpAcc {
    pub fn take(&mut self, dmg1024: i64) -> i64 {
        self.acc += dmg1024;
        let whole = self.acc.div_euclid(1024);
        self.acc -= whole * 1024;
        whole
    }
}
