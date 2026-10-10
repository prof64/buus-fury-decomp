"""bv_fighters_death.py type -- does the per-spawn callback (record +0x14 -> 0x0800424F run_script) fire on
death?  Boss HP set to 1, player punches; log callback / state changes / boss presence; screenshots."""
import sys
from bv_fighters_lib import *
from forcespawn import etype
from ents import enemies

t = int(sys.argv[1], 0)
e, o, ok = spawn2(t)
e.w32(o + 0x28c, 1 << 10)
p = e.r32(PLAYER)
bid = e.breakpoint(0x0800424e)
bid2 = e.breakpoint(0x08012588)
S = Shots(e, f'death_{t:02x}')
for f in range(600):
    e.w16(REC + 0x16, 9999)
    if f < 200:
        bx, by = pos(e, o)
        setpos(e, p, bx - 18, by)
        e.w8(p + 0x10, 3)
    e.set_keys(KEY.A if f % 10 < 2 else 0)
    e.run(1)
    if f % 75 == 0:
        S.snap()
S.write()
hs = e.hits()
print('callback hits', [(h['frame'], hex(h['pc']), hex(h['regs'][0])) for h in hs][:10])
print('boss still in enemy list', any(x == o for x in enemies(e)), 'hp', e.r32(o + 0x28c) >> 10)
