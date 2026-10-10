"""Record player walks into map walls in the emulator -> data/battle/buu_collision_traces.json.

Each trace: warp into the map (re/buu/warp.py from buu_snakeway_start.ss: new-game Goku, footprint 16x8, walk 352,
dash 672 /256 px per frame), put the player at `start` (obj+0x1A0/+0x1A4), wait, then hold the given keys frame by
frame and record the 24.8 position after every frame. site/tests/battle/buu_collision.test.ts replays the same keys
in the simulator and requires the same positions on every frame.

usage: .venv/bin/python tools/battle/gen_buu_collision_traces.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import emu, KEY  # noqa: E402
from warp import warp  # noqa: E402

PLAYER = 0x03001C34
M = 0x030019A4  # map object: +0xA8 walk grid (player)
K = {'L': KEY.LEFT, 'R': KEY.RIGHT, 'U': KEY.UP, 'D': KEY.DOWN, 'A': KEY.A, '-': 0}

# (name, map, start x, y, [(keys, frames), ...]); keys: letters of K joined, '-' = none
TRACES = [
    # Giant Crater: the cliff juts into the floor on the left (x < 64 at y 136..240) - the user-visible bug
    ('crater_left_cliff', '8-71', 104, 184, [('-', 2), ('L', 70)]),
    ('crater_top_cliff', '8-71', 200, 96, [('-', 2), ('U', 40), ('UL', 40), ('UR', 60)]),
    ('crater_corner_diag', '8-71', 112, 120, [('-', 2), ('UL', 80), ('DL', 60)]),
    ('crater_right_cliff_dash', '8-71', 360, 200, [('-', 2), ('R', 2), ('-', 2), ('R', 70)]),
    ('crater_bottom_edge', '8-71', 240, 330, [('-', 2), ('D', 60), ('DR', 60), ('DL', 90)]),
    # Rocky Canyon: rocks standing in the floor (slide along a rock, around its corner)
    ('canyon_rock', '8-70', 120, 236, [('-', 2), ('R', 60), ('U', 30), ('R', 40), ('DR', 50), ('D', 40)]),
    # Demon World: the stone ring in the floor
    ('demon_ring', '8-68', 160, 210, [('-', 2), ('D', 50), ('DL', 50), ('L', 40), ('UR', 40)]),
]


def keymask(s):
    m = 0
    for ch in s:
        m |= K[ch]
    return m


def record(e, st0, name, mid, x, y, script):
    wd, mp = (int(v) for v in mid.split('-'))
    e.load(st0)
    assert warp(e, wd, mp, x, y), name
    e.set_keys(0)
    e.run(30)
    p = e.r32(PLAYER)
    e.w32(p + 0x1A0, x << 8)
    e.w32(p + 0x1A4, y << 8)
    e.run(4)
    assert e.r32(p + 0x1A0) == x << 8 and e.r32(p + 0x1A4) == y << 8, (name, e.r32(p + 0x1A0) / 256, e.r32(p + 0x1A4) / 256)
    pos = []
    for keys, n in script:
        for _ in range(n):
            e.set_keys(keymask(keys))
            e.run(1)
            pos.append([e.r32(p + 0x1A0), e.r32(p + 0x1A4)])
    e.set_keys(0)
    size = [e.r8(p + 0x19C), e.r8(p + 0x19D)]
    assert size == [16, 8] and e.r32(PLAYER) == p, (name, 'not the player / a cutscene took over')
    return {'name': name, 'map': mid, 'start': [x, y], 'keys': script, 'footprint': size, 'pos': pos}


def main():
    e = emu('buu_snakeway_start.ss')
    st0 = e.save()
    out = {'source': 'tools/battle/gen_buu_collision_traces.py (emulator, buu_snakeway_start.ss + warp)', 'traces': []}
    for name, mid, x, y, script in TRACES:
        t = record(e, st0, name, mid, x, y, script)
        xs = [v[0] / 256 for v in t['pos']]
        ys = [v[1] / 256 for v in t['pos']]
        print(name, mid, 'footprint', t['footprint'], 'end', xs[-1], ys[-1], 'x range', min(xs), max(xs), 'y range', min(ys), max(ys))
        out['traces'].append(t)
    path = os.path.join(ROOT, 'data', 'battle', 'buu_collision_traces.json')
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print('wrote', path)


if __name__ == '__main__':
    main()
