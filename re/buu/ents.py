"""ents.py state -- list player + enemies with key fields."""
import sys
from common import *

PLAYER = 0x03001c34
ENEMY_LIST = 0x03001bb0
TYPE_TAB = 0x08711274


def enemies(e):
    n = e.r32(ENEMY_LIST)
    return [e.r32(ENEMY_LIST + 4 + 4 * i) for i in range(min(n, 64))]


def info(e, o):
    x, y, z = [e.r32(o + 0x1a0 + 4 * k) for k in range(3)]
    s = lambda v: v - (1 << 32) if v & 0x80000000 else v
    return dict(obj=hex(o), vt=hex(e.r32(o)), x=s(x) / 256, y=s(y) / 256, z=s(z) / 256)


if __name__ == '__main__':
    st = sys.argv[1]
    e = emu(st if (STATES / st).exists() else WORK / st)
    p = e.r32(PLAYER)
    print('player', info(e, p))
    for o in enemies(e):
        ent = e.r32(o + 0x224)
        t = (ent - TYPE_TAB) // 28
        print('enemy', info(e, o), 'type', hex(t), 'hp', e.r32(o + 0x28c) / 1024)
