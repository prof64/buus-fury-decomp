"""Record golden test vectors from the real game (headless mGBA) for the battle simulator's unit tests.

Writes data/battle/log2_vectors.json with
  damage[]  : every call of computeDamage 0x0800F910 (RNG state before, inputs, raw/final damage out)
  levelup[] : level-up 0x08003CA0 calls (RNG state, record before, record after)

usage: .venv/bin/python tools/battle/gen_vectors.py
"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log2'))
from common import new, u16, u32, KEY, ST  # noqa: E402
from runto import run_to  # noqa: E402
from ents import enemies  # noqa: E402


def place(e, dist):
    """Put the last live enemy `dist` px right of the player (player faces right), park the others."""
    P = e.r32(0x3001FA8)
    R = 0x3000E90 + 0xC + 28 * e.r8(0x3000E90)
    e.w16(R + 4, 999)
    e.w16(R + 6, 999)
    e.w8(R + 9, 3)  # basic energy blast slot
    en = [b for b, c in enemies(e) if e.r32(b + 0x188) > 0]
    if not en:
        return
    T = en[-1]
    for b in en[:-1]:
        e.w32(b + 0x158, 10 * 256)
        e.w32(b + 0x15C, 10 * 256)
    e.w32(T + 0x158, e.r32(P + 0x158) + dist * 256)
    e.w32(T + 0x15C, e.r32(P + 0x15C))
    e.w32(T + 0x188, 999999)
    e.w8(P + 0x10, 3)


def damage_vectors(state, n, btn, period=24, dist=None):
    e = new(state)
    if dist is not None:
        e.run(150)
        place(e, dist)
    out = []

    def keys(f):
        return btn if (f % period) < 2 else 0
    for _ in range(n):
        if not run_to(e, 0x0800F910, 900, keys):
            break
        this = e.reg(0)
        msg = e.reg(1)
        A = e.r32(msg + 8)
        att = e.r32(msg + 0xC)
        scale = e.r32(msg + 4)
        atype = e.r8(A)
        power = e.r32(A + 4)
        S = e.r8(att + 0x160 + (0 if atype == 0 else 1))
        dd = e.r8(this + 0x162)
        dm = e.r8(this + 0x163 + (0 if atype == 0 else 1))
        a, b = e.r32(0x3003890), e.r32(0x3003894)
        run_to(e, 0x0800F988, 5)
        out.append(dict(state=state, rng=[a, b], type=atype, id=e.r8(A + 1), power=power, S=S, DEF=dd, MULT=dm,
                        scale=scale, crit=e.r32(msg + 0x10), raw=e.r32(msg + 0x14), dmg=e.r32(msg + 0x18)))
    return out


def levelup_vectors(levels):
    EXP = [u32(0x81D9834 + 4 * i) for i in range(50)]
    out = []
    for lv in levels:
        e = new('log2_map12b.ss')
        P = 0x3000E90
        idx = e.r8(P)
        R = P + 0xC + 28 * idx
        e.w32(R + 0x10, EXP[lv] - 10)
        Pl = e.r32(0x3001FA8)
        en = [b for b, c in enemies(e) if e.r32(b + 0x188) > 0 and e.r32(b) == 0x80252A4]
        T = en[0]
        e.w32(T + 0x188, 1)
        e.w32(Pl + 0x158, e.r32(T + 0x158) - 20 * 256)
        e.w32(Pl + 0x15C, e.r32(T + 0x15C))

        def keys(f):
            return KEY.RIGHT if f % 20 == 0 else (KEY.A if f % 20 < 3 else 0)
        if not run_to(e, 0x08003CA0, 800, keys):
            continue

        def rec():
            return dict(hp=e.r16(R), max_hp=e.r16(R + 2), ep=e.r16(R + 4), max_ep=e.r16(R + 6),
                        STR=e.r16(R + 0xA), POW=e.r16(R + 0xC), END=e.r16(R + 0xE))
        before = rec()
        oldL, newL = e.reg(2), e.reg(3)
        a, b = e.r32(0x3003890), e.r32(0x3003894)
        run_to(e, 0x08003D96, 5)
        out.append(dict(char=idx, old_level=oldL, new_level=newL, rng=[a, b], before=before, after=rec()))
    return out


def main():
    res = {'source': 'tools/battle/gen_vectors.py (headless mGBA, LoG2 USA)', 'damage': [], 'levelup': []}
    for st, btn, n, dist in [('log2_map12.ss', KEY.A, 3, None), ('log2_pl_goku.ss', KEY.A, 14, 14),
                             ('log2_pl_goku.ss', KEY.B, 8, 40), ('log2_pl_vegeta.ss', KEY.A, 12, 14),
                             ('log2_pl_trunks.ss', KEY.B, 6, 40), ('log2_pl_gohan.ss', KEY.A, 10, 14),
                             ('log2_pl_hercule.ss', KEY.A, 8, 14), ('log2_pl_piccolo.ss', KEY.B, 6, 40)]:
        v = damage_vectors(st, n, btn, dist=dist)
        print(st, btn, len(v), file=sys.stderr)
        res['damage'] += v
    res['levelup'] = levelup_vectors([40, 41, 42])
    os.makedirs(os.path.join(ROOT, 'data/battle'), exist_ok=True)
    path = os.path.join(ROOT, 'data/battle/log2_vectors.json')
    json.dump(res, open(path, 'w'), indent=1)
    print('wrote', path, len(res['damage']), 'damage', len(res['levelup']), 'levelup')


main()
