"""Track Super Buu's charged ball (node 0x803bf0f, ball builder 0x08021280, data 0x0872a6a8) frame by frame.
usage: bv_buu_ball.py [dx] [dy]"""
import sys
from common import *
from bv_buu_lib import *

dx = int(sys.argv[1]) if len(sys.argv) > 1 else 70
dy = int(sys.argv[2]) if len(sys.argv) > 2 else 0
e, o = setup(0x78, dx=dx, dy=dy)
p = e.r32(PLAYER)
px, py = pos(e, p)
force(e, o, 0x2b2)
ball = None
prev = None
for f in range(140):
    e.setpos = None
    setpos(e, p, px, py)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    c = e.r32(o + 0x14)
    if ball is None and 0x02000000 <= c < 0x04000000 and e.r32(c) == 0x0804d6cc:
        ball = e.r32(c + 4)
        print('ctrl', hex(c), 'ball', hex(ball), 'vt', hex(e.r32(ball)), 'timer', e.r32(c + 8))
    if ball:
        w = [e.r32(ball + 4 * k) for k in range(0x18)]
        if prev is None or w != prev:
            ch = {hex(4 * k): (s32v(w[k])) for k in range(0x18) if prev is None or w[k] != prev[k]}
            print(f, 'ball', ch, 'boss ctrl', hex(e.r32(o + 0x14)), 'hp', e.r16(REC + 0x16))
        prev = w
        if e.r32(ball) != 0x08049f60 and e.r32(ball) != prev[0]:
            pass
    e.run(1)
