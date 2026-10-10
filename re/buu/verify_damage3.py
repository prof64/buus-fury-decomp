"""Enemy -> player damage verification (no pinning; Goku stands still / blocks)."""
from common import *
from emutools import catch
from damage_model import compute
from rng import Rng

REC = 0x03001560
ENTRY, RET = 0x0800E9D8, 0x0800EAEC
res = []
pend = {}


def cb(e, pc):
    if pc == ENTRY:
        tgt, hit = e.reg(0), e.reg(1)
        atk = e.r32(hit + 0xc)
        ad = e.r32(hit + 8)
        pend.update(rng=(e.r32(0x03004a5c), e.r32(0x03004a60)), kind=e.r8(ad), sub=e.r8(ad + 1), power=e.r32(ad + 4),
                    mult=e.r32(hit + 4), S=e.r16(atk + 0x1b0), Pw=e.r16(atk + 0x1b2), cb=e.r32(atk + 0x1bc),
                    E=e.r16(tgt + 0x1b4), E2=e.r16(tgt + 0x1b6), res=e.r8(tgt + (0x1b8 if e.r8(ad) == 0 else 0x1b9)),
                    blk=e.r32(tgt + 0x1c8), bf=e.r16(tgt + 0x1c0), face=e.r8(tgt + 0x10), hdir=e.r8(hit))
    else:
        p = pend
        blocking = p['blk'] != 0 and (p['hdir'] ^ 1) == p['face']
        dmg, crit, raw = compute(Rng(*p['rng']), p['kind'], p['sub'], p['power'], p['mult'], p['S'], p['Pw'], p['cb'],
                                 p['E'], p['E2'], p['res'], blocking, p['bf'])
        act = e.r32(e.reg(4) + 0x18)
        res.append(dict(blocking=blocking, actual=act, predicted=dmg, crit=crit, ok=dmg == act,
                        S=p['S'], E=p['E'], mult=p['mult'], kind=p['kind'], sub=p['sub']))


for keys in (0, KEY.R):
    e = emu('buu_fight_swforest.ss')
    for f in range(500):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        catch(e, [ENTRY, RET], 1, lambda _f: keys, cb)
print(sum(r['ok'] for r in res), '/', len(res))
for r in res:
    print(r)
