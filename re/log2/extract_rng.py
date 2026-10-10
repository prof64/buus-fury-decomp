"""Describe LoG2 RNG + enumerate every BL caller -> data/log2/rng.json"""
import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from common import *

RNG, RAND, CHANCE = 0x080213fc, 0x0802141c, 0x0802142e

def bl_callers(targets, end=0x30000):
    out = []
    for off in range(0, end, 2):
        h1 = ROM[off] | ROM[off + 1] << 8
        if h1 >> 11 == 0b11110:
            h2 = ROM[off + 2] | ROM[off + 3] << 8
            if h2 >> 11 == 0b11111:
                hi = h1 & 0x7ff
                if hi & 0x400: hi -= 0x800
                t = 0x08000000 + off + 4 + (hi << 12) + ((h2 & 0x7ff) << 1)
                if t in targets: out.append((0x08000000 + off, t))
    return out

def arg_before(addr):
    """immediate loaded into r0 just before the call (movs r0,#imm) if any"""
    import re
    lines = dis(addr - 8, 4).split('\n')
    for l in reversed(lines):
        m = re.search(r'movs r0, #(0x[0-9a-f]+|\d+)', l)
        if m: return int(m.group(1), 0)
    return None

KNOWN = {
    0x08000c1c: 'title/intro screen manager: advances RNG once per frame while on the title screens (the only per-frame advance)',
    0x08003c80: 'level-up stat growth: rand((max-min+1)*n) per stat (0x08003c6e)',
    0x08003d04: 'level-up: maxHP growth rand(0x51f)',
    0x08003d1c: 'level-up: maxEP growth rand(3)',
    0x0800755a: 'INFERRED entity: random animation timer rand(255)',
    0x0800758e: 'INFERRED entity: random animation timer rand(255)',
    0x08007e42: 'player melee hit: random hit sound variant rand(3) (sfx 0x22..0x24)',
    0x08007e98: 'player hit reaction: random state/anim variant rand(3)+0x28',
    0x080084d2: 'player: random sound variant rand(3)+0x22',
    0x08009572: 'script VM sub-op 0x11: top = rand(top)  (used by enemy drop scripts: rand(100))',
    0x0800958a: 'script VM sub-op 0x12: top = chance(top percent)',
    0x0800cb94: 'INFERRED map ambient object: chance(param) to trigger',
    0x0800cba8: 'INFERRED map ambient object: rand(5) variant',
    0x0800ddea: 'INFERRED screen shake / jitter: raw rng()',
    0x0800ddfc: 'INFERRED screen shake / jitter: raw rng()',
    0x0800e17e: 'enemy: random sound variant rand(2)+0x25',
    0x0800e91c: 'enemy: random sound variant rand(2)+0xd',
    0x0800ea16: 'enemy hurt: random sound variant rand(3)+0x22',
    0x0800f086: 'INFERRED hit spark/particle offset rand(5)-2',
    0x0800f08e: 'INFERRED hit spark/particle offset rand(3)-1',
    0x0800f25e: 'INFERRED debris/particle velocity rand(0x1000)',
    0x0800f930: 'DAMAGE: melee critical chance(0xa4*STR) -> double power',
    0x0800f968: 'DAMAGE: variance rand(26) -> x(256..281)/256',
    0x0800fa40: 'enemy death explosion/particles rand(0x1000)',
    0x0801a7b2: 'enemy death: random death sound rand(2)+0x1f',
    0x08021420: 'rand(n) itself', 0x08021432: 'chance(p) itself',
}
def label(a):
    if a in KNOWN: return KNOWN[a]
    if 0x08011c00 <= a < 0x08011e00: return 'visual effect particles (random offsets)'
    if 0x08013300 <= a < 0x08016d00: return 'player special-move / projectile code (spread, jitter, particle effects) - see player agent'
    if 0x0801a000 <= a < 0x0801f000: return 'enemy AI behaviour-tree nodes (random wander direction/timers/attack choice) - see enemy agent'
    return 'unlabelled'

def main():
    calls = bl_callers({RNG, RAND, CHANCE})
    rows = [dict(call_site=hex(a), target={RNG: 'rng', RAND: 'rand', CHANCE: 'chance'}[t], r0_imm=arg_before(a), purpose=label(a)) for a, t in calls]
    out = dict(
        source='LoG2 ROM; extract_rng.py',
        state=dict(addr='0x03003890', words=['a (u32 @0x03003890)', 'b (u32 @0x03003894)'],
                   initial='0,0 (IWRAM is zero-cleared by crt0 at 0x0800010c); no explicit seeding - VERIFIED no writes during boot logos',
                   seeding='entropy comes only from the number of frames spent on the title/intro screens (0x08000c1c calls rng() every frame there); '
                           'in gameplay the RNG is NOT advanced per frame, only on demand (VERIFIED: 120 idle frames in-map -> 0 calls). '
                           'State is not saved to EEPROM (not part of the 0x164-byte party block) - INFERRED'),
        rng=dict(addr=hex(RNG), algorithm=[
            'r1 = (a + 0x02cbbc71) ^ b',
            'r0 = (rol32(b, 3) + 0x0632d80f) ^ r1',
            'b = r0; a = r1',
            'return (r1 - r0) mod 2^32'], python=(
            'def rng(s):\n a,b=s\n r1=((a+0x2cbbc71)&M)^b\n r0=((((b<<3)|(b>>29))&M)+0x632d80f)&M\n r0^=r1\n s[:]=[r1,r0]\n return (r1-r0)&M')),
        rand=dict(addr=hex(RAND), formula='rand(n) = ((rng() & 0xffff) * n) >> 16   -> uniform-ish in [0, n)'),
        chance=dict(addr=hex(CHANCE), formula='chance(p) = 1 if (rng() & 0xffff) < p else 0   -> probability p/65536'),
        verified='Damage formula predictions using this RNG matched the emulator 14/14 hits; level-up prediction matched (it_lvl.py).',
        callers=rows)
    path = os.path.join(ROOT, 'data/log2/rng.json')
    json.dump(out, open(path, 'w'), indent=1)
    print('wrote', path, len(rows), 'callers', sum(r['purpose'] == 'unlabelled' for r in rows), 'unlabelled')

if __name__ == '__main__':
    main()
