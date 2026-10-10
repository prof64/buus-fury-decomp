"""Second damage verification pass: crits (forced high crit bonus), defense >= 200 clamp, enemy->player hits
and blocking (holding R)."""
from common import *
from emutools import catch
from damage_model import compute
from rng import Rng
from combat import setup, pin, REC
import verify_damage as vd

ENTRY, RET = vd.ENTRY, vd.RET
vd.results.clear()


def run(keys, frames, mod):
    e = emu('buu_fight_swforest.ss')
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    p, en = setup(e)
    e.w8(p + 0x10, 0)
    for f in range(frames):
        pin(e, en[0], p, 0, 22)
        mod(e, p, en[0])
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        catch(e, [ENTRY, RET], 1, lambda _f, f=f: keys(f), vd.cb)


def mod_crit_def(e, p, o):
    e.w32(p + 0x1bc, 30000)
    e.w16(o + 0x1b4, 250); e.w16(o + 0x1b6, 300)


run(lambda f: KEY.A if f % 8 < 4 else 0, 300, mod_crit_def)
run(lambda f: 0, 400, lambda e, p, o: None)                 # enemy attacks a passive player
run(lambda f: KEY.R, 400, lambda e, p, o: None)             # player blocks (R held)
ok = sum(r[-1] for r in vd.results)
print(f'{ok}/{len(vd.results)} exact')
for r in vd.results:
    if not r[-1] or r[9]:
        print(r)
