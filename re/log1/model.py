"""Python model of LoG1 formulas (verified against the emulator by verify_*.py)."""
M32 = 0xFFFFFFFF


class Rng:
    """Main game RNG at 0x08005CA8; state words at 0x0300098C (a) and 0x03000990 (b)."""

    def __init__(self, a, b):
        self.a, self.b = a & M32, b & M32

    def next(self, n):
        """rand(n): returns an integer in [0, n] (inclusive)."""
        a = ((self.a + 0x02CBBC71) & M32) ^ self.b
        b = ((((self.b << 3) | (self.b >> 29)) & M32) + 0x0632D80F) & M32
        b ^= a
        self.a, self.b = a, b
        prod = (a * b) & 0xFFFF
        v = (prod * (n + 1)) & M32          # muls (32-bit wrap)
        if v & 0x80000000:                   # asrs #16 is arithmetic: values >= 2^31 go negative
            v -= 1 << 32
        return v >> 16                       # NOTE: for n >= 32768 the result can be negative (game bug)


def apply_damage(rng, dmg, defense, hp, def_team=2, flag_a=0, flag_b=0, godmode=0):
    """0x08007352: returns (result, new_hp). result 0 = no damage, 1 = killed, 2 = damaged.
    def_team: defender char+0x9A; flag_a/flag_b: defender char bytes +0x0A/+0x0B;
    godmode: word at 0x03000C18 (player cannot lose HP when nonzero)."""
    if def_team != 1:
        if flag_a != 0:
            return 0, hp
        if flag_b == 2 and defense != 0:
            return 0, hp
    if defense != 0:
        r = rng.next(10000)
        if r > 9000:
            dmg = dmg - defense if dmg > defense else 0   # keeps 0 when dmg <= defense
        elif r > 7500:
            d = defense >> 1
            dmg = dmg - d if dmg >= d else 0
        elif r > 5000:
            d = defense >> 2
            dmg = dmg - d if dmg >= d else 0
        elif r > 4800:
            return 0, hp
        # else full damage
    if dmg >= hp:
        if godmode and def_team == 1:
            return 2, hp
        return 1, 0
    if dmg == 0:
        return 0, hp
    if godmode and def_team == 1:
        return 2, hp
    return 2, hp - dmg


class Rng2(Rng):
    """Secondary RNG 0x0800A6F4 (state 0x03003D24/28). rand2(n) in [0, n-1]."""

    def raw(self):
        a = ((self.a + 0x02CBBC71) & M32) ^ self.b
        b = (((((self.b << 3) | (self.b >> 29)) & M32) + 0x0632D80F) & M32) ^ a
        self.a, self.b = a, b
        return (a * b) & M32

    def rand2(self, n):
        return (((self.raw() >> 16) * n) & M32) >> 16


def tdiv(n, d):
    """ARM division routine 0x0800E568: truncates toward zero."""
    q = abs(n) // abs(d)
    return q if (n >= 0) == (d >= 0) else -q


def player_punch(max_hp, rng2):
    """0x08005676 (VERIFIED)."""
    return (max_hp * (0x180 + rng2.rand2(0x80))) >> 9


def ki_blast_damage(owner_max_hp):
    """0x0800B596, player and enemy ki blasts (VERIFIED)."""
    return tdiv(owner_max_hp, 3)


def enemy_melee(cur_hp, rng):
    """0x0800F7EC (VERIFIED; can be negative for cur_hp >= 65536)."""
    return (cur_hp >> 2) + rng.next(cur_hp >> 1)


def enemy_damage_cap(player_max_hp):
    """0x08006D70: per-hit cap for enemy attacks on the player."""
    return tdiv(player_max_hp, 3) + 1


def charge_cost(max_ki):
    """0x0800B50A: Ki taken every 8 frames while charging."""
    return (max_ki + 15) >> 4


def solar_flare_duration(charge, max_ki):
    """0x0800B692 (VERIFIED)."""
    return 60 + tdiv(min(charge, max_ki) * 300, max_ki)
