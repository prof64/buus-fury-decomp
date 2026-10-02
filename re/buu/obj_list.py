"""obj_list.py [state] -- world object list (world = [0x03001C4C], +4 count, +8.. pointers) + trigger list"""
import sys
from common import *

WORLD_PTR = 0x03001C4C
TRIG = 0x03002A64


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def objects(e):
    w = WORLD_PTR
    n = e.r32(w + 4)
    return [e.r32(w + 8 + 4 * i) for i in range(min(n, 200))]


def triggers(e):
    n = e.r32(TRIG)
    out = []
    for i in range(min(n, 100)):
        t = e.r32(TRIG + 4 + 4 * i)
        r = [s32v(e.r16(t + 4 + 2 * k) | (0xffff0000 if e.r16(t + 4 + 2 * k) & 0x8000 else 0)) for k in range(4)]
        out.append((t, e.r32(t), r, e.r32(t + 0xC), e.r32(e.r32(t + 0xC)) if e.r32(t + 0xC) else 0))
    return out


def dump(e):
    for o in objects(e):
        vt = e.r32(o)
        words = [e.r32(o + 4 * i) for i in range(1, 8)]
        print(hex(o), 'vt', hex(vt), ' '.join(hex(x) for x in words))
    for t in triggers(e):
        print('trig', hex(t[0]), 'vt', hex(t[1]), t[2], 'test', hex(t[3]), hex(t[4]))


if __name__ == '__main__':
    e = emu(sys.argv[1] if len(sys.argv) > 1 else 'buu_swforest3.ss')
    e.run(1)
    print('area', hex(e.r32(0x03001980)))
    dump(e)
