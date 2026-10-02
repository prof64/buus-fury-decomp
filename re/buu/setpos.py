"""setpos.py in.ss x y out.ss [frames] [keys] -- set player position (pixels) and run."""
import sys
from common import *
PLAYER_PTR = 0x03001b94   # IWRAM: pointer to the player character object


def player(e):
    return e.r32(PLAYER_PTR)


def setpos(e, x, y):
    p = player(e)
    e.w32(p + 0x1a0, x << 8)
    e.w32(p + 0x1a4, y << 8)


if __name__ == '__main__':
    inp = sys.argv[1]
    x, y = int(sys.argv[2], 0), int(sys.argv[3], 0)
    out = sys.argv[4]
    frames = int(sys.argv[5]) if len(sys.argv) > 5 else 200
    keys = 0
    if len(sys.argv) > 6:
        for ch in sys.argv[6].split('+'):
            keys |= getattr(KEY, ch)
    e = emu(inp if (STATES / inp).exists() else WORK / inp)
    print('player', hex(player(e)))
    setpos(e, x, y)
    S = Shots(e, 'grid')
    e.set_keys(keys)
    for i in range(8):
        e.run(frames // 8)
        S.snap()
    S.write()
    e.save_file(WORK / out)
    print('area', hex(e.r32(0x03001980)), 'pos', hex(e.r32(player(e) + 0x1a0)), hex(e.r32(player(e) + 0x1a4)))
