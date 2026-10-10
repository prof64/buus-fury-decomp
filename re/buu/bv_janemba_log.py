"""bv_janemba_log.py type frames [hpfrac] [dx] -- force-spawn an enemy, keep the player alive and still,
log AI state changes per enemy object (0x0800AA4A replace / 0x0800AA9C push, obj = r0 - 0x238 or 0x228),
object spawns (0x0801191A, r1 = object), and the enemy roster (type, pos, hp) every 30 frames.
hpfrac: set the boss HP to this fraction of max right after spawn (to trigger IfHpBelow phases)."""
import sys
from common import *
from forcespawn import spawn, etype
from ents import enemies, PLAYER

REC = 0x03001560
t = int(sys.argv[1], 0)
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 900
hpfrac = float(sys.argv[3]) if len(sys.argv) > 3 else None
dx = int(sys.argv[4]) if len(sys.argv) > 4 else None
e, o, ok = spawn(t)
print('spawned', hex(o) if o else None, ok)
if not o:
    sys.exit()
maxhp = u32(0x08711274 + 28 * t + 4)
if hpfrac is not None:
    e.w32(o + 0x28c, int(maxhp * hpfrac) << 10)
p = e.r32(PLAYER)


def sv(v):
    return v - (1 << 32) if v & 0x80000000 else v


def pos(x):
    return round(sv(e.r32(x + 0x1a0)) / 256, 1), round(sv(e.r32(x + 0x1a4)) / 256, 1)


if dx is not None:
    bx, by = pos(o)
    e.w32(p + 0x1a0, int((bx + dx) * 256) & 0xffffffff); e.w32(p + 0x1a4, int(by * 256) & 0xffffffff)
ids = {e.breakpoint(0x0800AA4A): 'goto', e.breakpoint(0x0800AA9C): 'push', e.breakpoint(0x0801191A): 'spawn',
       e.breakpoint(0x0800E9D8): 'dmg'}
S = Shots(e, f'jlog_{t:02x}')
known = {}
php0 = e.r16(REC + 0x16)
for f in range(frames):
    e.run(1)
    e.w16(REC + 0x16, e.r16(REC + 0x18))       # keep player alive
    for h in e.hits():
        kind = ids.get(h['id'], '?') if 'id' in h else None
        pc = h['addr']
        r = h['regs']
        if pc == 0x0800AA4A or pc == 0x0800AA9C:
            ob = r[0] - 0x238 if (r[0] - 0x238) in known or True else r[0]
            print(f'{f:5d} {"goto" if pc == 0x0800AA4A else "push"} obj {r[0]:#x}-0x238={r[0] - 0x238:#x} state {r[1]:#x}')
        elif pc == 0x0801191A:
            print(f'{f:5d} spawn obj {r[1]:#x} vt {e.r32(r[1]):#x} list {r[0]:#x}')
        elif pc == 0x0800E9D8:
            print(f'{f:5d} dmg target {r[0]:#x} hit {r[1]:#x} {"PLAYER" if r[0] == p else ""}')
    if f % 30 == 0:
        ros = []
        for x in enemies(e):
            ros.append(f'{etype(e, x):#x}@{x:#x} {pos(x)} hp{e.r32(x + 0x28c) >> 10}')
        print(f'{f:5d} ROSTER player {pos(p)} | ' + ' | '.join(ros))
    if f % 60 == 0 and f < 60 * 16:
        S.snap()
S.write()
