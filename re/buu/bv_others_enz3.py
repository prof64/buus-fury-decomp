"""Enzyme latch rects (0x0800FE78: rect = pos + s32 table[obj+0x58 + 16*facing + 0x10..0x1C]).
Sweep player offsets with the enzyme facing pinned to 0 (down, the only facing with a non-empty rect)
and record when the latch (drain enter 0x080298D8) fires; then measure the HP drain while latched."""
from common import *
from bv_buu_lib import setup, pos, REC
from forcespawn import setpos
from ents import PLAYER


def rect(e, ob):
    t = e.r32(ob + 0x58) + 16 * e.r8(ob + 0x10) + 0x10
    x, y = pos(e, ob)
    w = [e.r32(t + 4 * i) for i in range(4)]
    w = [v - (1 << 32) if v & 0x80000000 else v for v in w]
    return [int(x) + w[0], int(y) + w[1], int(x) + w[2], int(y) + w[3]]


ok = []
base = None
for dx in range(-10, 4, 2):
    for dy in range(-24, 12, 4):
        e, o = setup(0x18, dx=40, dy=0)
        p = e.r32(PLAYER)
        bx, by = pos(e, o)
        e.breakpoint(0x080298D8)
        hit = None
        for f in range(6):
            e.w8(o + 0x10, 0)
            setpos(e, p, bx + dx, by + dy)
            e.run(1)
            if e.hits():
                hit = f
                break
        if hit is not None:
            ok.append((dx, dy))
            if base is None:
                base = (dx, dy, e.save())
print('latched at offsets (player - enzyme):', ok)
if base:
    dx, dy, st = base
    e, o = setup(0x18, dx=40, dy=0)
    e.load(st)
    p = e.r32(PLAYER)
    log = []
    for f in range(400):
        e.set_keys(0)
        e.run(1)
        log.append((f, e.r16(REC + 0x16), e.r16(REC + 0x1a), pos(e, p), pos(e, o)))
    ch = [b for a, b in zip(log, log[1:]) if a[1:3] != b[1:3]]
    print('HP/EP changes while latched:', [(c[0], c[1], c[2]) for c in ch][:20])
    print('pos f0', log[0][3:], 'f399', log[-1][3:])
