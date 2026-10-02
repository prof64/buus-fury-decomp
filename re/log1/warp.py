"""Warp helper: call load_map(map, entrance) by intercepting a natural load_map call.

warp(e, map_idx, entrance_ptr=None): e must be in a state where a map load will happen
within `budget` frames of the given input (e.g. a door). Implemented generically by
stepping to the load_map entry (0x08001FA8) and replacing r0/r1.
"""
import sys
from common import *

LOAD_MAP = 0x08001FA8
MAP_TABLE = 0x081CA09C


def map_rec(i):
    return u32(MAP_TABLE + 4 * i)


def default_entrance(i):
    return u32(map_rec(i) + 0x14) + 0x28


def run_until_loadmap(e, keys, budget=600):
    """Returns True with CPU stopped exactly at load_map entry."""
    bp = e.breakpoint(LOAD_MAP)
    for f in range(budget):
        st = e.save()
        k = keys if (f // 4) % 2 == 0 else 0
        e.set_keys(k)
        e.run(1)
        if e.hits():
            e.clear(bp)
            e.load(st)
            e.set_keys(k)
            for _ in range(2_000_000):
                e.step()
                if e.reg(15) == LOAD_MAP + 4 and (e.reg(16) & 0x20):
                    return True
            return False
    e.clear(bp)
    return False


SCRATCH = 0x0203FFF0


def warp(e, m, entrance=None, keys=0, budget=3000, xy=None):
    if not run_until_loadmap(e, keys, budget):
        raise RuntimeError('no load_map call')
    if xy is not None:
        e.w16(SCRATCH, xy[0]); e.w16(SCRATCH + 2, xy[1])
        entrance = SCRATCH
    e.set_reg(0, m)
    e.set_reg(1, entrance if entrance is not None else default_entrance(m))
    e.set_keys(0)
    e.run(120)


if __name__ == '__main__':
    inp, m, outp = sys.argv[1], int(sys.argv[2], 0), sys.argv[3]
    keyname = sys.argv[4] if len(sys.argv) > 4 else 'A'
    xy = (int(sys.argv[5]), int(sys.argv[6])) if len(sys.argv) > 6 else None
    e = emu(inp)
    warp(e, m, keys=getattr(KEY, keyname), xy=xy)
    e.save_file(STATES / outp)
    e.screenshot(SHOTS / (outp + '.png'), scale=1)
    print('ok frame', e.frame)
