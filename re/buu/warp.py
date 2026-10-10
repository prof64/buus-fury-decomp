"""warp.py world map x y out.ss [in.ss] -- reliable teleport.

Uses the Snakeway start state: the 'too far to walk' text trigger object there (vtable 0x08049bd4) is
converted into an area-exit trigger (vtable 0x08049bc4, handler 0x080071f8) whose exit data points at
(world, map, x, y); then the player walks right into it so the game performs a normal exit transition.
Exit data layout (0x18+ bytes): +8 s16 srcX, +0xa s16 srcY, +0x10 u8 facing, +0x11 world, +0x12 map,
+0x13 arg, +0x14 s16 dstX, +0x16 s16 dstY; new player pos = player + (dst - src)."""
import sys
from common import *

PLAYER = 0x03001c34
TRIG = 0x0203dfa4          # text trigger object in buu_snakeway_start.ss
EXIT = 0x0203FF40          # scratch RAM for the exit data


def warp(e, w, m, x, y, frames=400):
    assert e.r32(TRIG) == 0x08049bd4, hex(e.r32(TRIG))
    for i in range(0, 0x20, 4):
        e.w32(EXIT + i, 0)
    e.w8(EXIT + 0x10, 2); e.w8(EXIT + 0x11, w); e.w8(EXIT + 0x12, m)
    e.w16(EXIT + 0x14, x); e.w16(EXIT + 0x16, y)
    e.w32(TRIG, 0x08049bc4)
    e.w8(TRIG + 0x10, 2)
    e.w8(TRIG + 0x11, 0xff)
    e.w32(TRIG + 0x14, EXIT)
    p = e.r32(PLAYER)
    e.w32(p + 0x1a0, 0x160 << 8)
    e.w32(p + 0x1a4, 0xa0 << 8)
    area0 = e.r32(0x03001980)
    e.set_keys(KEY.RIGHT)
    for f in range(frames):
        p = e.r32(PLAYER)
        if e.r32(0x03001980) == area0 and p:
            e.w16(EXIT + 8, e.r32(p + 0x1a0) >> 8)
            e.w16(EXIT + 0xa, e.r32(p + 0x1a4) >> 8)
        e.run(1)
        if e.r32(0x03001980) != area0:
            e.set_keys(0)
    e.set_keys(0)
    e.run(60)
    return e.r32(0x03001980) != area0


if __name__ == '__main__':
    w, m, x, y = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3], 0), int(sys.argv[4], 0)
    out = sys.argv[5]
    inp = sys.argv[6] if len(sys.argv) > 6 else 'buu_snakeway_start.ss'
    e = emu(inp)
    ok = warp(e, w, m, x, y)
    S = Shots(e, 'grid')
    S.snap()
    e.save_file(WORK / out)
    for k in (KEY.RIGHT, KEY.DOWN, KEY.LEFT, KEY.UP):
        e.hold(k, 30)
        S.snap()
    e.set_keys(0)
    S.write()
    p = e.r32(PLAYER)
    print('ok', ok, 'area', hex(e.r32(0x03001980)), 'player', hex(p), 'cpsr', hex(e.reg(16)),
          'pos', e.r32(p + 0x1a0) / 256 if p else None, e.r32(p + 0x1a4) / 256 if p else None)
