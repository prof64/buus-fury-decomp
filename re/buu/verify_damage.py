"""Verify the damage model against the game: at ComputeDamage entry snapshot RNG + every input,
predict, then compare with the value the game stores at hit+0x18 (0x0800EABE)."""
from common import *
from emutools import catch
from damage_model import compute
from rng import Rng
from combat import setup, pin, REC

ENTRY, RET = 0x0800E9D8, 0x0800EABE
results = []
pending = {}


def cb(e, pc):
    if pc == ENTRY:
        tgt, hit = e.reg(0), e.reg(1)
        atk = e.r32(hit + 0xc)
        ad = e.r32(hit + 8)
        pending.update(rng=(e.r32(0x03004a5c), e.r32(0x03004a60)), kind=e.r8(ad), sub=e.r8(ad + 1), power=e.r32(ad + 4),
                       mult=e.r32(hit + 4), S=e.r16(atk + 0x1b0), Pw=e.r16(atk + 0x1b2), cb=e.r32(atk + 0x1bc),
                       E=e.r16(tgt + 0x1b4), E2=e.r16(tgt + 0x1b6),
                       res=e.r8(tgt + (0x1b8 if e.r8(ad) == 0 else 0x1b9)))
    else:
        actual = e.reg(0)
        p = pending
        dmg, crit, raw = compute(Rng(*p['rng']), p['kind'], p['sub'], p['power'], p['mult'], p['S'], p['Pw'], p['cb'],
                                 p['E'], p['E2'], p['res'])
        results.append((p['kind'], p['sub'], p['mult'], p['S'], p['Pw'], p['E'], p['E2'], actual, dmg, crit, actual == dmg))


for st, keys, frames, setup_fn in (
        ('buu_fight_swforest.ss', lambda f: KEY.A if f % 8 < 4 else 0, 400, None),
        ('buu_fight_swforest.ss', lambda f: KEY.B if f % 12 < 3 else 0, 300, None)):
    e = emu(st)
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    p, en = setup(e)
    e.w8(p + 0x10, 0)
    for f in range(frames):
        pin(e, en[0], p, 0, 30)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        catch(e, [ENTRY, RET], 1, lambda _f, f=f: keys(f), cb)
ok = sum(r[-1] for r in results)
print(f'{ok}/{len(results)} damage events predicted exactly')
for r in results[:30]:
    print(r)
