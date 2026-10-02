"""AlignWithPlayer (0x0801CFDB) speed check: player held diagonally from the enemy (big dx, some dy) so the
enemy strafes on y to line up.  Log per-frame enemy y deltas (24.8 raw) while x does not change, and the
node update calls (0x0801CDC0) to know when the node is running."""
import sys
from collections import Counter
from common import *
from emutools import cur_pc
from forcespawn import spawn, setpos
from ents import PLAYER

REC = 0x03001560
UPD = 0x0801CDC0
for t in [int(a, 0) for a in sys.argv[1:]] or [0x4b, 0x16]:
    e, o, ok = spawn(t, px=120, py=120)
    e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
    ox, oy = e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256
    p = e.r32(PLAYER)
    steps = Counter()
    upd_frames = 0
    prev = None
    for f in range(900):
        setpos(e, p, ox + 55, oy + 30)
        e.w16(REC + 0x16, 9999)
        st = e.save()
        b = e.breakpoint(UPD)
        e.run(1)
        hit = bool(e.hits())
        e.clear(b)
        x, y = e.r32(o + 0x1a0), e.r32(o + 0x1a4)
        if prev and hit:
            upd_frames += 1
            steps[(x - prev[0], y - prev[1])] += 1
        prev = (x, y)
    print(hex(t), 'frames with AlignWithPlayer update', upd_frames, 'per-frame (dx,dy) raw 1/256 px:', steps.most_common(6))
