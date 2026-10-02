"""Regenerate data/log2/formulas.json from the ROM.

Every constant is read back out of the ROM instruction stream / literal pools at the
address where the game uses it (asserted), so a different ROM revision fails loudly.
Formulas were derived by disassembly and verified hit-by-hit in the emulator
(re/log2/verify_dmg.py: 32/32 random-stat hits matched exactly, melee + energy, incl. crits).
"""
import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from common import ROM, u32, u16, _ct, ROOT

def insn(addr):
    off = addr & 0x1ffffff
    i = next(_ct.disasm(ROM[off:off+4], addr))
    return i.mnemonic, i.op_str

def imm_of(addr):
    m, ops = insn(addr)
    assert '#' in ops, (hex(addr), m, ops)
    return int(ops.split('#')[-1].rstrip(']'), 0)

def lit_of(addr):
    """value loaded by `ldr rX,[pc,#imm]` at addr"""
    m, ops = insn(addr)
    assert m == 'ldr' and 'pc' in ops, (hex(addr), m, ops)
    imm = int(ops.split('#')[-1].rstrip(']'), 0)
    return u32(((addr + 4) & ~3) + imm)

def s32(v): return v - (1 << 32) if v & 0x80000000 else v

# ---- damage (0x0800f910) -------------------------------------------------------
CRIT_K   = imm_of(0x0800f92c)          # movs r0,#0xa4 ; muls r0,STR
RAND_N   = imm_of(0x0800f954)          # movs r0,#0x1a ; rand(26)
CUBIC_C1 = s32(lit_of(0x0800f956))     # -701
CUBIC_C0 = lit_of(0x0800f95c)          # 115618
CUBIC_A  = imm_of(0x0800f954)          # 26 (same register value reused: movs r0,#0x1a ; muls r0,r5)
assert (CRIT_K, RAND_N, CUBIC_C1, CUBIC_C0) == (0xa4, 26, -701, 115618)

def stat_curve(S):
    return (((CUBIC_A * S + CUBIC_C1) * S + CUBIC_C0) * S) >> 8

# ---- exp award (0x0800e4d2) ----------------------------------------------------
assert insn(0x0800e50e) == ('asrs', 'r1, r0, #7')
assert insn(0x0800e518) == ('asrs', 'r0, r0, #1')
assert imm_of(0x0800e4e2) == 5 and imm_of(0x0800e4fa) == 0x32

# ---- level-up (0x08003ca0 / 0x08003c6e) --------------------------------------------
HP_RAND = lit_of(0x08003d02)   # 0x51f
HP_BASE = lit_of(0x08003d08)   # 0xe14
EP_RAND = imm_of(0x08003d1a)   # 3  -> rand(3)
assert insn(0x08003d22) == ('adds', 'r0, #2')
STAT_CAP = imm_of(0x08003c92) << imm_of(0x08003c94)   # 0x19<<10 = 0x6400
assert (HP_RAND, HP_BASE, EP_RAND, STAT_CAP) == (0x51f, 0xe14, 3, 0x6400)

EXP_TBL = 0x081d9834
exp_table = [u32(EXP_TBL + 4 * i) for i in range(50)]
GROWTH = 0x081d95f0
names = ['Gohan', 'Piccolo', 'Vegeta', 'Trunks', 'Goku', 'Hercule']
growth = {}
for c, n in enumerate(names):
    v = [u16(GROWTH + 12 * c + 2 * k) for k in range(6)]
    growth[n] = {'STR': {'min': v[0], 'max': v[1]}, 'POW': {'min': v[2], 'max': v[3]}, 'END': {'min': v[4], 'max': v[5]}}

out = {
    "_source": "re/log2/extract_formulas.py (constants asserted against ROM). Units: stats in record are 8.8 fixed point; entity stat bytes are integers.",
    "rng": {
        "see": "data/log2/rng.json",
        "next": "a'=(a+0x2CBBC71)^b; b'=(rol(b,3)+0x632D80F)^a'; return (a'-b') mod 2^32   [0x080213FC, state a@0x03003890 b@0x03003894]",
        "rand(n)": "((next() & 0xFFFF) * n) >> 16   -> 0..n-1   [0x0802141C]",
        "chance(p)": "(next() & 0xFFFF) < p   -> 1/0   [0x0802142E]",
        "status": "VERIFIED (disasm + reproduces in-game damage rolls)"
    },
    "damage": {
        "function": "0x0800F910 (EntityBase::computeDamage(this=defender, msg)); called from onMessage type 0/2 hit tests (enemy 0x0800E5B8, player 0x08008944)",
        "inputs": {
            "atk": "attack descriptor (msg+8): u8 type (0=melee/physical, else energy), u32 power at +4 (table 0x086DB2CC, 8 bytes/entry)",
            "scale": "msg+4, 0x400 = 1.0 (charged/special attacks pass other values)",
            "S": "attacker stat byte: STR (entity+0x160) for melee, POW (entity+0x161) for energy",
            "DEF": "defender END byte (entity+0x162)",
            "MULT": "defender damage-taken multiplier /128: entity+0x163 for melee, entity+0x164 for energy (players: always 0x80)"
        },
        "steps": [
            "if type==0: crit = chance(0xA4*S); power = power*(1+crit); msg+0x10 = crit   (crit doubles power; P(crit) = 164*S/65536 = S*0.25%)",
            "else: crit = 0",
            "b = (power*(128-DEF)) >> 7",
            "f = (((26*S - 701)*S + 115618)*S) >> 8",
            "r = rand(26) + 256          (random factor 256..281 /256 = 1.000..1.098)",
            "t = ((r*b) >>u 8) * f >>u 18   (unsigned shifts on 32-bit products)",
            "t = (scale*t) >> 10           -> stored msg+0x14 (pre-multiplier damage)",
            "dmg = (t*MULT) >> 7           -> stored msg+0x18 (HP removed)"
        ],
        "python": "def dmg(type,power,S,DEF,MULT,scale,rng):\n  crit=rng.chance(0xa4*S) if type==0 else 0\n  power*=1+crit\n  b=(power*(128-DEF))>>7\n  f=(((26*S-701)*S+115618)*S)>>8\n  r=rng.rand(26)+256\n  t=(((r*b)&0xffffffff)>>8)*f\n  t=(t&0xffffffff)>>18\n  t=(scale*t)>>10\n  return (t*MULT)>>7",
        "rng_call_order": "melee: chance() then rand(26); energy: rand(26) only",
        "apply": "enemy: HP(entity+0x188) -= dmg, <=0 -> death (0x0800E484). player: party record HP -= dmg clamped at 0 (0x08008784); 0 -> death state 0x21 + game-over event after 200 frames",
        "status": "VERIFIED: 32/32 hits with randomized attacker/defender stats matched exactly (re/log2/verify_dmg.py + setup_poke.py), melee+energy, crits included"
    },
    "stat_curve": {
        "expr": "f(S) = (((26*S - 701)*S + 115618)*S) >> 8",
        "meaning": "attack-stat multiplier; damage ~= power*(128-DEF)/128 * f(S)/2^18 * scale/1024 * MULT/128 * U(1.0,1.098)",
        "table_S_0_to_100": [stat_curve(s) for s in range(101)],
        "damage_per_1024_power_vs_DEF0": [round(stat_curve(s) / 256, 3) for s in range(101)]
    },
    "crit": {"expr": "P(crit) = (0xA4*STR)/65536 per melee hit (STR byte of attacker)", "effect": "power x2 before defense", "melee_only": True, "status": "VERIFIED"},
    "defense": {"expr": "power*(128-END)>>7; END is the defender's integer END stat", "notes": "END>=128 would zero/negate damage; player END capped at 100 (stat cap 0x6400)", "status": "VERIFIED"},
    "player_stat_bytes": {
        "expr": "entity+0x160..0x162 = record STR/POW/END (8.8) >> 8; +0x163/+0x164 = 0x80",
        "where": "normal-form apply 0x08016FDA (form controller object vtable[0]); refreshed only when a form is applied (map load / transform), not per frame",
        "status": "VERIFIED (disasm + RAM: 0x2872/0x37B3/0x35C0 -> 40/55/53)"
    },
    "exp_award": {
        "function": "0x0800E4D2 (on enemy death)",
        "expr": "E = class.exp; if E==0: none; if E>5 and L<50: D = EXP[L] - EXP[L-1] (span of active char's current level L, 1-based table index); E = max(E, D>>7); E = min(E, D>>1). L==50 -> no exp. Granted via a 1-frame delayed event (vtable 0x080245B0) -> 0x08003D58(party, char, E)",
        "status": "VERIFIED (Lv40 Piccolo killing a 13-exp snake gained exactly (3335510-2727775)>>7 = 4747)"
    },
    "level": {
        "exp_table_addr": hex(EXP_TBL),
        "exp_table": exp_table,
        "level_of(exp)": "largest L in 1..50 with EXP[L-1] <= exp  (0x08004214)",
        "max_level": 50
    },
    "level_up": {
        "function": "0x08003CA0(party, char, oldLv, newLv)",
        "stats": "for STR/POW/END (8.8): stat += min*n + rand((max-min+1)*n), n = levels gained; clamp to 0x6400 (100.0)  [0x08003C6E]",
        "growth_table_addr": hex(GROWTH),
        "growth_per_level_1_256ths": growth,
        "max_hp": "per level: maxHP += (maxHP * (rand(0x51F) + 0xE14)) >> 16   (+5.50%..+7.50% compounding), then HP = maxHP",
        "max_ep": "per level: maxEP += rand(3) + 2   (+2..+4), then EP = maxEP",
        "stat_cap": STAT_CAP,
        "status": "disasm VERIFIED; level-up outcome check in data/log2/levels.json"
    }
}
dst = os.path.join(ROOT, 'data/log2/formulas.json')
with open(dst, 'w') as f:
    json.dump(out, f, indent=1)
print('wrote', dst)
