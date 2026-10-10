"""tp3.py in.ss world map x y out.ss [arg] -- teleport the player during gameplay by invoking the
area-exit trigger handler (0x080071f8) with a fake trigger whose exit data points at (world,map,x,y).
The handler is what door/edge triggers run: it fades, calls EnterAreaWithPlayer (0x08008d44) and
re-creates the player at  player_pos + (dest_anchor - src_anchor)."""
import sys
from common import *
from emutools import call

PLAYER = 0x03001c34


def teleport(e, w, m, x, y, arg=0):
    p = e.r32(PLAYER)
    px, py = e.r32(p + 0x1a0) >> 8, e.r32(p + 0x1a4) >> 8
    trig, ex = 0x0203FF00, 0x0203FF40
    for i in range(0, 0x60, 4):
        e.w32(trig + i, 0)
    e.w8(trig + 0x11, 0xff)
    e.w32(trig + 0x14, ex)
    e.w16(ex + 8, px); e.w16(ex + 0xa, py)
    e.w8(ex + 0x10, 2); e.w8(ex + 0x11, w); e.w8(ex + 0x12, m); e.w8(ex + 0x13, arg)
    e.w16(ex + 0x14, x); e.w16(ex + 0x16, y)
    e.set_keys(KEY.RIGHT)        # handler requires a held direction matching trigger mask (+0x11)
    e.run(1)
    r = call(e, 0x080071f8, (trig, p, 0, 0))
    e.set_keys(0)
    return r


if __name__ == '__main__':
    inp = sys.argv[1]
    w, m, x, y = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4], 0), int(sys.argv[5], 0)
    out = sys.argv[6]
    arg = int(sys.argv[7]) if len(sys.argv) > 7 else 0
    e = emu(inp if (STATES / inp).exists() else WORK / inp)
    teleport(e, w, m, x, y, arg)
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
    print('area ptr', hex(e.r32(0x03001980)), 'player', hex(e.r32(PLAYER)))
