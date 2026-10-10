"""Spot checks in the real game for feel-critical timings (compared with the simulator):
  punch: A pressed at frames 2 and 17/18/19 -> damage-call frames. Result: 2nd punch only at 19 (a press at 18
         ends the punch into the special-melee charge, released next frame = cancel; the simulator does the same)
  walk/dash: x position per frame while holding RIGHT, then double-tap RIGHT
usage: .venv/bin/python tools/battle/emu_check.py punch|move"""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log2'))
from common import new, KEY  # noqa: E402
from ents import enemies  # noqa: E402


def place(e, dist):
    P = e.r32(0x3001FA8)
    en = [b for b, c in enemies(e) if e.r32(b + 0x188) > 0]
    T = en[-1]
    for b in en[:-1]:
        e.w32(b + 0x158, 10 * 256)
        e.w32(b + 0x15C, 10 * 256)
    e.w32(T + 0x158, e.r32(P + 0x158) + dist * 256)
    e.w32(T + 0x15C, e.r32(P + 0x15C))
    e.w32(T + 0x188, 999999)
    e.w8(P + 0x10, 3)


def frames_with_hits(e, keys, n):
    out = []
    for f in range(n):
        e.set_keys(keys(f))
        bp = e.breakpoint(0x0800F910)
        e.run(1)
        if e.hits():
            out.append(f)
        e.clear(bp)
    return out


cmd = sys.argv[1]
if cmd == 'punch':
    for pattern in ([2, 18], [2, 19], [2, 17]):
        e = new('log2_pl_goku.ss')
        e.run(150)
        place(e, 14)
        print(pattern, frames_with_hits(e, lambda f: KEY.A if f in pattern else 0, 40))
elif cmd == 'move':
    e = new('log2_pl_goku.ss')
    e.run(150)
    P = e.r32(0x3001FA8)
    xs = []
    keys = [KEY.RIGHT] * 20 + [0] * 4 + [KEY.RIGHT] + [0] * 3 + [KEY.RIGHT] * 20
    for k in keys:
        e.set_keys(k)
        e.run(1)
        xs.append(e.r32(P + 0x158) / 256)
    print(' '.join(f'{x:.2f}' for x in xs))
