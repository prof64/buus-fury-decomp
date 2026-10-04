//! RNG, ComputeDamage and LevelUp against the values recorded in the real game (data/battle/buu_vectors.json,
//! buu_enemy_hits.json) - the same checks as site/tests/battle/buu.test.ts, exact equality.
mod common;
use common::json;
use wf_core::damage::{compute_damage_raw, Draw, HpAcc};
use wf_core::progression::{level_up, Record};
use wf_core::rng::Rng;

#[test]
fn rng_matches_reference_algorithm() {
    let mut r = Rng::new(0x12345678, 0x9abcdef0);
    let (mut s0, mut s1) = (0x12345678u32, 0x9abcdef0u32);
    for _ in 0..20 {
        let a = s0.wrapping_add(0x02cbbc71) ^ s1;
        let b = s1.rotate_left(3).wrapping_add(0x0632d80f) ^ a;
        s0 = a;
        s1 = b;
        assert_eq!(r.next(), a.wrapping_sub(b));
    }
}

#[test]
fn damage_vectors_exact() {
    let v = json("buu_vectors.json");
    let mut n = 0;
    for d in v["damage"].as_array().unwrap() {
        let g = |k: &str| {
            d[k].as_i64()
                .unwrap_or(d[k].as_bool().map(|b| b as i64).unwrap_or(0))
        };
        let kind = g("kind");
        let blocking = d["blocking"].as_bool().unwrap() && (g("hitDir") ^ 1) == g("defFacing");
        let mut rng = Rng::new(
            d["rng"][0].as_u64().unwrap() as u32,
            d["rng"][1].as_u64().unwrap() as u32,
        );
        let res = compute_damage_raw(
            kind as u8,
            g("sub") as u8,
            g("power"),
            g("mult"),
            if kind == 0 { g("S") } else { g("POW") },
            g("critBonus"),
            if kind == 0 { g("E") } else { g("E2") },
            g("resist"),
            blocking,
            g("blockFactor"),
            &mut wf_core::damage::rng_draw(&mut rng),
        );
        assert_eq!(
            (res.crit as i64, res.raw, res.dmg),
            (g("crit"), g("raw"), g("dmg")),
            "{d}"
        );
        n += 1;
    }
    assert!(n >= 48);
}

#[test]
fn every_recorded_enemy_hit_is_a_formula_output() {
    let h = json("buu_enemy_hits.json");
    let mut n = 0;
    for row in h["hits"].as_array().unwrap() {
        let r: Vec<i64> = row
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        let (kind, sub, power, mult, s, pow, e, e2, actual) =
            (r[1], r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]);
        let mut found = false;
        'outer: for resist in [128, 0] {
            for roll in 0..26 {
                for crit in [false, true] {
                    let mut f = |d: Draw| match d {
                        Draw::Chance(_) => crit as i64,
                        Draw::Rand(_) => roll,
                    };
                    let out = compute_damage_raw(
                        kind as u8,
                        sub as u8,
                        power,
                        mult,
                        if kind == 0 { s } else { pow },
                        0,
                        if kind == 0 { e } else { e2 },
                        resist,
                        false,
                        0x1000,
                        &mut f,
                    );
                    if out.dmg == actual {
                        found = true;
                        break 'outer;
                    }
                }
            }
        }
        assert!(found, "row {row}");
        n += 1;
    }
    assert!(n > 500);
}

#[test]
fn hp_acc_carries_remainder() {
    let mut a = HpAcc::default();
    assert_eq!([a.take(1500), a.take(1500), a.take(100)], [1, 1, 1]);
}

#[test]
fn levelup_vectors_exact() {
    let v = json("buu_vectors.json");
    let d = common::data();
    for x in v["levelup"].as_array().unwrap() {
        let ch = x["char"].as_i64().unwrap() as i32;
        let c = d.chars.iter().find(|c| c.index == ch).unwrap();
        let b = &x["before"];
        let gi = |o: &serde_json::Value, k: &str| o[k].as_i64().unwrap() as i32;
        let mut rec = Record {
            level: gi(x, "old_level"),
            xp: b["xp"].as_i64().unwrap(),
            hp: gi(b, "hp"),
            max_hp: gi(b, "maxHp"),
            ep: gi(b, "ep"),
            max_ep: gi(b, "maxEp"),
            points: gi(b, "points"),
            ..Default::default()
        };
        let mut rng = Rng::new(
            x["rng"][0].as_u64().unwrap() as u32,
            x["rng"][1].as_u64().unwrap() as u32,
        );
        level_up(
            &mut rec,
            &c.growth,
            gi(x, "old_level"),
            gi(x, "new_level"),
            &mut rng,
        );
        let a = &x["after"];
        assert_eq!(
            (rec.hp, rec.max_hp, rec.ep, rec.max_ep, rec.points),
            (
                gi(a, "hp"),
                gi(a, "maxHp"),
                gi(a, "ep"),
                gi(a, "maxEp"),
                gi(a, "points")
            )
        );
    }
}
