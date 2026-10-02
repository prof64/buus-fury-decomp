"""Bio Mech ranged attack: count frames in the approach node 0x0803D34B (update 0x0803D110), shot node entry
(0x08037E2C), projectile spawns and damage, with the player held at a given distance after aggro."""
import sys
from collections import Counter
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER
from emutools import cur_pc

REC = 0x03001560
t = int(sys.argv[1], 0) if len(sys.argv) > 1 else 5
dist = int(sys.argv[2]) if len(sys.argv) > 2 else 110
dy = int(sys.argv[3]) if len(sys.argv) > 3 else 0
e, o, ok = spawn(t, px=120, py=120)
e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
p = e.r32(PLAYER)
ox, oy = e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256
W = {0x0803D110: 'approach', 0x08037E2C: 'shot', 0x0801191A: 'spawn', 0x0800AA4A: 'goto'}
cnt = Counter()
shot_seen = False
track = []
ev = []
for f in range(3000):
    near = f < 60
    cx, cy = e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256
    if not shot_seen:
        setpos(e, p, cx + (50 if near else dist), cy + (0 if near else dy))
    else:
        track.append((f, round(e.r32(p + 0x1a0) / 256 - cx, 1), round(e.r32(p + 0x1a4) / 256 - cy, 1), hex(e.r32(p + 0x14)), e.r16(REC + 0x16)))
        if len(track) > 120:
            break

    st = e.save()
    bids = [e.breakpoint(a) for a in W]
    e.run(1)
    hs = e.hits()
    for b in bids:
        e.clear(b)
    if hs:
        e.load(st)
        s0 = e.frame
        while e.frame == s0:
            pc = cur_pc(e)
            if pc in W:
                k = W[pc]
                cnt[k] += 1
                if k == 'spawn':
                    ev.append((f, 'spawn', hex(e.r32(e.reg(1))), hex(e.reg(1))))
                elif k == 'goto':
                    ev.append((f, 'state', hex(e.reg(1)), hex(e.reg(0)), e.r32(o + 0x1a0) // 256, e.r32(o + 0x1a4) // 256))
                elif k == 'shot':
                    ev.append((f, 'shot'))
                    shot_seen = True
            e.step()
print(dict(cnt))
for x in ev[-6:]:
    print(x)
for x in track[::3]:
    print(x)
