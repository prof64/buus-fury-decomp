"""tp4.py in.ss world map x y out.ss -- teleport by calling EnterAreaWithPlayer (0x08008d44) directly
(the same call the exit trigger makes), bracketed by the exit's fade helpers."""
import sys
from common import *
from emutools import call

AREA_MGR = 0x0300197c
PLAYER = 0x03001c34


def teleport(e, w, m, x, y, d=2):
    pos = 0x0203FFE0
    e.w16(pos, x); e.w16(pos + 2, y)
    r = call(e, 0x08008d44, (AREA_MGR, w, m, pos), (d, 1, 0, 0))
    return r


if __name__ == '__main__':
    inp = sys.argv[1]
    w, m, x, y = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4], 0), int(sys.argv[5], 0)
    out = sys.argv[6]
    e = emu(inp if (STATES / inp).exists() else WORK / inp)
    r = teleport(e, w, m, x, y)
    print('ret', hex(r), 'cpsr', hex(e.reg(16)), 'player', hex(e.r32(PLAYER)))
    S = Shots(e, 'grid')
    for i in range(4):
        e.run(60)
        S.snap()
    e.save_file(WORK / out)
    for k in (KEY.RIGHT, KEY.DOWN, KEY.LEFT, KEY.UP):
        e.hold(k, 40)
        S.snap()
    e.set_keys(0)
    S.write()
    print('area ptr', hex(e.r32(0x03001980)), 'player', hex(e.r32(PLAYER)), 'cpsr', hex(e.reg(16)))
