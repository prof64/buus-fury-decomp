"""npc_enemyprobe.py W M X Y: do field enemies run their map behaviour (def +0x18)? catch the behaviour runner."""
import sys
from npc_lib import *

w, m, x, y = (int(a, 0) for a in sys.argv[1:5])
e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, w, m, x, y)
area0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
for f in range(600):
    fix()
    e.run(1)
    if e.r32(AREA_PTR) != area0:
        break
e.set_keys(0)
e.run(40)
ENEMY_LIST = 0x03001BB0
n = e.r32(ENEMY_LIST)
ens = [e.r32(ENEMY_LIST + 4 + 4 * i) for i in range(min(n, 64))]
print('enemies', [(hex(o), e.r32(o + 0x1A0) >> 8, e.r32(o + 0x1A4) >> 8) for o in ens])
seen = {}


def cb(e, pc):
    a = e.reg(1)
    r0 = e.reg(0)
    seen.setdefault(hex(a), [hex(r0), hex(e.reg(14))])


emutools.catch(e, [0x0800A95A], 3, cb=cb)
print('runner calls (actor -> runner, lr):', seen)
for o in ens:
    r = o + 0x22C
    cur = e.r32(r)
    print(hex(o), 'runner', hex(cur), hex(e.r32(cur)) if cur else None, 'idx', e.r8(r + 4))
pos0 = [(e.r32(o + 0x1A0), e.r32(o + 0x1A4)) for o in ens]
e.run(300)
print('moved', [((e.r32(o + 0x1A0) - p[0]) / 256, (e.r32(o + 0x1A4) - p[1]) / 256) for o, p in zip(ens, pos0)])
