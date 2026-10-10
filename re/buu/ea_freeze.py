"""ea_freeze.py type scn frame -- replay an ea_rec scenario up to `frame`, then log which actor-update functions run
for the enemy on that frame and the next (to find why a state tick is skipped)."""
import sys
import ea_rec
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER, enemies

t, scn, F = int(sys.argv[1], 0), sys.argv[2], int(sys.argv[3])
# re-run the recorder loop but stop at F
orig_run = None
e, o, ok = spawn(t, settle=0)
p = e.r32(PLAYER)
s32v = ea_rec.s32v
bx, by = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
dx = {'far': -110, 'near': -36, 'blast': -90, 'move': -60}.get(scn, -22)
px, py = bx + dx, by
setpos(e, p, px, py)
e.w8(p + 0x10, 3)
ids = [e.breakpoint(a) for a in ea_rec.EV]   # same breakpoints -> same timing
for f in range(F + 3):
    e.w16(0x03001560 + 0x16, e.r16(0x03001560 + 0x18))
    if scn in ('far', 'near', 'blast'):
        setpos(e, p, px, py)
    e.set_keys(0)
    if f >= F - 1:
        bs = [e.breakpoint(a) for a in (0x0800EF08, 0x0800DE6C, 0x0800DEB0, 0x08011A1E, 0x08006E3C)]
        e.run(1)
        hs = e.hits()
        for b in bs:
            e.clear(b)
        print('frame', f, 'tick', e.r16(o + 0x12), 'o+0x18', hex(e.r32(o + 0x18)), hex(e.r32(e.r32(o + 0x18))))
        for h in hs:
            if True:
                print('   %08x r0 %08x r1 %08x lr %08x' % (h['addr'], h['regs'][0], h['regs'][1], h['lr']))
    else:
        e.run(1)
        e.hits()
print('enemy', hex(o), 'player', hex(p))
