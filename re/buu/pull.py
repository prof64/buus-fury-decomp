"""pull.py in.ss out.ss [dx] [dy] -- move all enemies next to the player (offset dx,dy px), run a bit."""
import sys
from common import *
from ents import enemies, PLAYER, info
inp, out = sys.argv[1], sys.argv[2]
dx = int(sys.argv[3]) if len(sys.argv) > 3 else 40
dy = int(sys.argv[4]) if len(sys.argv) > 4 else 0
e = emu(inp if (STATES / inp).exists() else WORK / inp)
p = e.r32(PLAYER)
px, py = e.r32(p + 0x1a0), e.r32(p + 0x1a4)
for i, o in enumerate(enemies(e)):
    e.w32(o + 0x1a0, px + ((dx + 24 * i) << 8))
    e.w32(o + 0x1a4, py + (dy << 8))
S = Shots(e, 'grid')
for i in range(8):
    e.run(15)
    S.snap()
S.write()
e.save_file(WORK / out)
for o in enemies(e):
    print(info(e, o))
