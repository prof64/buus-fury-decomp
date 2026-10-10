"""Exact integer model of Character::ComputeDamage (0x0800E9D8).

Inputs (all as the game sees them):
  attack_kind   attack-type byte0 (0 = melee/physical, else energy)       [hit+8 -> byte0]
  attack_sub    attack-type byte1 (3 = melee that can never crit)          [hit+8 -> byte1]
  power         attack-type u32 at +4 (1024 = 1.0)                          [hit+8 -> +4]
  mult          per-hit multiplier (1024 = 1.0)                              [hit+4]
  atk_STR, atk_POW, atk_crit_bonus (attacker obj +0x1B0/+0x1B2/+0x1BC)
  def_END (melee, obj+0x1B4), def_END2 (energy, obj+0x1B6), resist byte (obj+0x1B8 melee / +0x1B9 energy, 128 = 1.0)
  block: defender obj+0x1C8 != 0 and attack direction (hit byte0 ^ 1) == defender facing -> * obj+0x1C0 / 65536
Returns damage in 1/1024 HP units (HP loss = accumulated >> 10).
"""


def compute(rng, kind, sub, power, mult, atk_str, atk_pow, atk_crit_bonus, def_end, def_end2, resist,
            blocking=False, block_factor=0x1000):
    crit = 0
    P = power
    if kind == 0:
        S = atk_str
        if sub != 3:
            crit = 1 if rng.chance(0x52 * S + atk_crit_bonus) else 0
            if crit:
                P = P + (P >> 1)
        P = (S * P * 0xCCD) >> 16
        D = def_end
    else:
        S = atk_pow
        D = def_end2
    if D >= 200:
        D = D // 4 + 150 if D >= 0 else -((-D) // 4) + 150
        if D > 220:
            D = 220
    X = ((221 - D) * P * 297) >> 16
    A = S + ((S * S * (S + 100)) >> 13)
    Y = (mult * X + 1023) >> 10
    r = rng.randn(26)
    raw = (((256 + r) * Y) >> 8) * A
    dmg = (resist * raw + 127) >> 7
    if blocking and dmg:
        dmg = (dmg * block_factor) >> 16
    return dmg, crit, raw
