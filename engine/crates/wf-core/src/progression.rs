//! Character progression (re/buu/notes.md §3): level(xp), LevelUp 0x08013678 (4 x rand(256) per level),
//! RecalcStats 0x08013C66 (equipment + effect scripts + Holy Water), Fusion 0x0801375E.

use crate::content::{Character, Equip};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Record {
    pub level: i32,
    pub xp: i64,
    pub hp: i32,
    pub max_hp: i32,
    pub ep: i32,
    pub max_ep: i32,
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub spd: i32,
    pub points: i32,
}

pub fn level_of(xp: i64, table: &[i64]) -> i32 {
    let mut l = 50;
    for (i, &t) in table.iter().enumerate() {
        if xp >= t {
            l = 50 + i as i32;
        }
    }
    l
}

pub fn level_up_rolls(rec: &mut Record, g: &[i64; 8], r: [i64; 4]) {
    let mut mh = rec.max_hp as i64;
    mh += (((g[0] * r[0]) >> 8) + g[1]) >> 16;
    mh += (mh * (((g[2] * r[1]) >> 8) + g[3])) >> 16;
    let mut me = rec.max_ep as i64;
    me += (g[4] * r[2] + g[5]) >> 16;
    me += (me * (((g[6] * r[3]) >> 8) + g[7])) >> 16;
    rec.max_hp = mh as i32;
    rec.max_ep = me as i32;
    rec.hp = rec.max_hp;
    rec.ep = rec.max_ep;
    rec.points += 3;
}

pub fn level_up(rec: &mut Record, g: &[i64; 8], old_l: i32, new_l: i32, rng: &mut Rng) {
    for _ in old_l..new_l {
        let a = rng.rand(256) as i64;
        let b = rng.rand(256) as i64;
        let c = rng.rand(256) as i64;
        let d = rng.rand(256) as i64;
        level_up_rolls(rec, g, [a, b, c, d]);
    }
    rec.level = new_l;
}

pub fn new_game_record(c: &Character) -> Record {
    Record {
        level: c.level,
        xp: c.xp,
        hp: c.hp,
        max_hp: c.hp,
        ep: c.ep,
        max_ep: c.ep,
        str_: c.str_,
        pow: c.pow,
        end: c.end,
        spd: c.spd,
        points: 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowthMode {
    Rolled,
    Average,
    Min,
    Max,
}

pub fn record_at_level(
    c: &Character,
    level: i32,
    xp_table: &[i64],
    mode: GrowthMode,
    seed: u32,
) -> Record {
    let mut rec = new_game_record(c);
    let mut rng = Rng::from_seed(seed);
    let target = c.level.max(level.min(200));
    for l in c.level..target {
        match mode {
            GrowthMode::Rolled => level_up(&mut rec, &c.growth, l, l + 1, &mut rng),
            _ => {
                let v = match mode {
                    GrowthMode::Min => 0,
                    GrowthMode::Max => 255,
                    _ => 128,
                };
                level_up_rolls(&mut rec, &c.growth, [v; 4]);
            }
        }
    }
    rec.level = target;
    if let Some(&t) = xp_table.get((target - 50) as usize) {
        rec.xp = rec.xp.max(t);
    }
    rec
}

pub fn fuse(a: &Record, b: &Record) -> Record {
    let half = |x: i32, y: i32| (x + y) / 2;
    Record {
        level: a.level.max(b.level),
        xp: (a.xp + b.xp) / 2,
        hp: ((a.hp + b.hp) * 154) >> 8,
        max_hp: ((a.max_hp + b.max_hp) * 154) >> 8,
        ep: ((a.ep + b.ep) * 154) >> 8,
        max_ep: ((a.max_ep + b.max_ep) * 154) >> 8,
        str_: half(a.str_, b.str_) + 10,
        pow: half(a.pow, b.pow) + 10,
        end: half(a.end, b.end) + 10,
        spd: half(a.spd, b.spd) + 16,
        points: 0,
    }
}

/// effective record stats and factors (record +0x4C..+0x67)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct EffStats {
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub end2: i32,
    pub spd: i32,
    /// /16
    pub ep_cost_factor: i32,
    pub ep_block_factor: i32,
    /// /256
    pub xp_mult: i32,
    pub thorns: i32,
    /// /65536
    pub block_factor: i32,
    pub ss_cooldown_rate: i32,
    pub hp_regen256: i32,
    pub crit_pct: i32,
    pub melee_ep: i32,
    pub melee_hp_instead: bool,
}

pub fn recalc_stats(rec: &Record, equip: &[Option<&Equip>], holy_water: bool) -> EffStats {
    recalc_stats_with(rec, equip, holy_water.then_some(HOLY_WATER))
}

/// the timed buff Holy Water gives (GD+0x4E0..0x4EC: STR / POW / END / SPD)
pub const HOLY_WATER: [i32; 4] = [25, 25, 25, 25];

/// RecalcStats with the running timed buff (`boost_stats` 0xF2 / Holy Water: GD+0x4DC frames left, GD+0x4E0..0x4EC
/// the STR / POW / END / SPD bonuses, applied by 0x080046AE)
pub fn recalc_stats_with(
    rec: &Record,
    equip: &[Option<&Equip>],
    boost: Option<[i32; 4]>,
) -> EffStats {
    let mut s = EffStats {
        str_: rec.str_,
        pow: rec.pow,
        end: rec.end,
        end2: rec.end,
        spd: rec.spd,
        ep_cost_factor: 16,
        ep_block_factor: 16,
        xp_mult: 256,
        thorns: 0,
        block_factor: 0x1000,
        ss_cooldown_rate: 0x4000,
        hp_regen256: 0,
        crit_pct: 0,
        melee_ep: 1,
        melee_hp_instead: false,
    };
    let has = |id: i32| equip.iter().flatten().any(|e| e.id == id);
    for e in equip.iter().flatten() {
        s.str_ += e.str_;
        s.pow += e.pow;
        s.end += e.end;
        s.end2 += e.end2;
        s.spd += e.spd;
        s.xp_mult += e.xp;
        s.crit_pct += e.crit_pct;
        if e.block_half {
            s.block_factor >>= 1;
        }
        if e.energy_cost_half {
            s.ep_cost_factor >>= 1;
        }
        if e.energy_block_half {
            s.ep_block_factor >>= 1;
        }
        if e.ss_cooldown78 {
            s.ss_cooldown_rate = (s.ss_cooldown_rate * 7) >> 3;
        }
        s.thorns += e.thorns;
        s.hp_regen256 += e.hp_regen256;
        s.melee_ep = if e.melee_ep == 2 {
            2
        } else {
            s.melee_ep + if e.melee_ep == 1 { 1 } else { 0 }
        };
        if e.melee_hp_instead {
            s.melee_hp_instead = true;
        }
        if e.id == 40 && has(73) && has(107) {
            s.str_ += 50;
            s.end += 50;
            s.end2 += 50;
            s.pow += 50;
        }
        if e.id == 153 && rec.hp * 20 <= rec.max_hp {
            s.str_ += 50;
            s.pow += 50;
            s.end += 50;
            s.end2 += 50;
            s.spd += 50;
        }
    }
    if let Some([bs, bp, be, bd]) = boost {
        s.str_ += bs;
        s.pow += bp;
        s.end += be;
        s.end2 += be;
        s.spd += bd;
    }
    s.str_ = s.str_.max(0);
    s.pow = s.pow.max(0);
    s.end = s.end.max(0);
    s.end2 = s.end2.max(0);
    s.spd = s.spd.max(0);
    s
}

pub fn xp_gain(enemy_exp: i64, mult256: i64) -> i64 {
    (enemy_exp * mult256 + 255) >> 8
}
