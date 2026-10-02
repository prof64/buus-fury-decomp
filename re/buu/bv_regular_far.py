"""Aggro an enemy at close range, then hold the player far away (ranged attacks that need distance):
bv_regular_far.py type near_dx far_dx frames"""
import sys
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER
from bv_regular_trace import trace, summary, REC

t = int(sys.argv[1], 0)
near = int(sys.argv[2]) if len(sys.argv) > 2 else 50
far = int(sys.argv[3]) if len(sys.argv) > 3 else 110
frames = int(sys.argv[4]) if len(sys.argv) > 4 else 1500
e, o, ok = spawn(t, px=120, py=120)
e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
p = e.r32(PLAYER)
ox, oy = e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256
for f in range(60):
    setpos(e, p, ox + near, oy)
    e.run(1)
log = trace(t, frames, far, 0, e=e, o=o)
summary(log)
