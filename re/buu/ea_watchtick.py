"""ea_watchtick.py type scn frame -- replay an ea_rec scenario (same setup / inputs / breakpoints) and on video frame
`frame` watch writes to the target enemy's tick (obj+0x12): who wrote it (pc, lr)."""
import sys
import ea_rec
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER, enemies

t, scn, F = int(sys.argv[1], 0), sys.argv[2], int(sys.argv[3])
e, o, ok = spawn(t, settle=0)
p = e.r32(PLAYER)
s32v = ea_rec.s32v
bx, by = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
dx = {'far': -110, 'near': -36, 'mid': -64, 'blast': -90, 'move': -60}.get(scn, -22)
px, py = bx + dx, by
setpos(e, p, px, py)
e.w8(p + 0x10, 3)
ids = [e.breakpoint(a) for a in ea_rec.EV]
for f in range(F + 1):
    e.w16(0x03001560 + 0x16, e.r16(0x03001560 + 0x18))
    if scn in ('far', 'near', 'mid', 'blast'):
        setpos(e, p, px, py)
    keys = 0
    if scn in ('punch', 'kill') and o in enemies(e):
        ex, ey = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
        setpos(e, p, ex - 20, ey)
        e.w8(p + 0x10, 3)
        keys = KEY.A if f % 24 < 2 else 0
    elif scn == 'blast':
        e.w8(p + 0x10, 3)
        keys = KEY.B if f % 48 < 2 else 0
    elif scn == 'move':
        keys = [KEY.RIGHT, KEY.DOWN, KEY.LEFT, KEY.UP][(f // 90) % 4]
    e.set_keys(keys)
    w = e.watch(o + 0x12, o + 0x14, 'w') if f == F else None
    bq = e.breakpoint(0x0800DFBE) if f == F else None
    e.run(1)
    hs = e.hits()
    if w is not None:
        e.clear(w)
        e.clear(bq)
        for h in hs:
            if h['kind'] == 'break' and h['addr'] & ~1 == 0x0800DFBE:
                print('idle enter r0 %08x r1 %08x lr %08x' % (h['regs'][0], h['regs'][1], h['lr']))
        for h in hs:
            if h['kind'] == 'watch':
                print('frame', f, 'write %d -> %d pc %08x lr %08x' % (h['old'], h['new'], h['pc'], h['lr']))
        print('tick now', e.r16(o + 0x12))
