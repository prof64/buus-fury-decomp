"""Record the enemy path follower in the emulator -> data/battle/buu_path_traces.json.

From work/states/buu_fight_swforest.ss (Southwest Forest, 4 idle Majin Fighters) one fighter is driven by the game's
own path update 0x0803BB48 (called once per frame through re/buu/emutools.call) on a path object written to scratch
EWRAM (+4 speed, +8 line budget 50, +0x18 re-plan counter 0, +0x1C/+0x20 goal), toward goals behind trees, so the
plan 0x0803B6EA has to wall-follow (0x0803B4FC). Per frame: the enemy's 24.8 position and the path waypoint.
The tiles other objects marked on the enemy grid (map object +0xB0: the other fighters, NPCs) are stored as well, so
site/tests/battle/buu_collision.test.ts can rebuild the same grid and replay the frames with path.ts.

usage: .venv/bin/python tools/battle/gen_buu_path_traces.py
"""
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
sys.path.insert(0, HERE)
from common import emu  # noqa: E402
from emutools import call  # noqa: E402
import buu_collision as bc  # noqa: E402

M = 0x030019A4
ENEMIES = 0x03001BB0
PATH = 0x0203FF00
# (name, enemy index in the list, goal x, y px, frames, speed)
RUNS = [
    ('forest_tree_down', 3, 236, 330, 360, 153),
    ('forest_tree_left', 3, 60, 300, 420, 256),
    ('forest_around_up', 3, 330, 40, 300, 204),
]


def main():
    out = {'source': 'tools/battle/gen_buu_path_traces.py (emulator: path update 0x0803BB48 called every frame)', 'map': '8-3', 'runs': []}
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    m83 = next(m for m in sprites['maps'] if m['map'] == '8-3')
    h = bc.header_for(m83)[0]
    walk = bc.bitmap(bc.u32(h + 0x34))
    for name, idx, gx, gy, frames, speed in RUNS:
        e = emu('buu_fight_swforest.ss')
        e.run(2)
        en = e.r32(ENEMIES + 4 + 4 * idx)
        start = [e.r32(en + 0x1A0), e.r32(en + 0x1A4)]
        size = [e.r8(en + 0x19C), e.r8(en + 0x19D)]
        # the enemy grid now: tiles marked by everything but this enemy
        grid = np.unpackbits(np.frombuffer(e.read(e.r32(M + 0xB0), 0x2000), dtype=np.uint8).reshape(256, 32), axis=1, bitorder='little')
        own = set()
        x0, y0 = start[0] >> 8, start[1] >> 8
        for ty in range(y0 >> 3, (y0 + size[1] + 7) >> 3):
            for tx in range(x0 >> 3, (x0 + size[0] + 7) >> 3):
                own.add((tx, ty))
        marked = [[int(tx), int(ty)] for ty, tx in np.argwhere((grid == 0) & (walk == 1)) if (int(tx), int(ty)) not in own]
        for i in range(0, 0x24, 4):
            e.w32(PATH + i, 0)
        e.w32(PATH + 4, speed)
        e.w32(PATH + 8, 50)
        e.w32(PATH + 0x1C, gx << 8)
        e.w32(PATH + 0x20, gy << 8)
        pos, wp = [], []
        for f in range(frames):
            call(e, 0x0803BB48, (PATH, en, 0, 0))
            pos.append([e.r32(en + 0x1A0), e.r32(en + 0x1A4)])
            wp.append([e.r32(PATH + 0xC), e.r32(PATH + 0x10)])
        out['runs'].append({'name': name, 'enemy': idx, 'start': start, 'footprint': size, 'goal': [gx, gy], 'speed': speed,
                            'marked': marked, 'pos': pos, 'waypoint': wp})
        print(name, 'start', [v / 256 for v in start], 'end', [v / 256 for v in pos[-1]], 'goal', gx, gy, 'distinct waypoints',
              len({tuple(w) for w in wp}), 'other marks', len(marked))
    path = os.path.join(ROOT, 'data', 'battle', 'buu_path_traces.json')
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print('wrote', path)


if __name__ == '__main__':
    main()
