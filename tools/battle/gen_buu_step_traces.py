"""Record enemy step nodes in the emulator -> data/battle/buu_step_traces.json.

From work/states/buu_fight_swforest.ss (Southwest Forest) the 4th Majin Fighter is driven by one node update called
once per frame through re/buu/emutools.call on a node object in scratch EWRAM (+4 speed, +8 second parameter):
  backoff   0x08018C3A  the node enemy_ai.json labels StepTowardPlayer: moves AWAY from the player, facing it
  stepaway  0x0801CBA1  StepAwayFromPlayer: moves away from the player, facing away
  wander    0x0802B708  Wander (+8 = turn chance): rand-driven, the RNG state before each frame is recorded
  walkto    0x0802CDE8  WalkToPlayer (Spopovich's node; +8 = detour counter): toward the player, blocked -> detour
The fighters' own aggro test (IfPlayerWithin2, r2 = 10000) is disabled (its node instances in EWRAM and its ROM
definition patched to 0) so their AI never moves them. Per frame: enemy 24.8 position, facing, node +8, the RNG
state (0x03004A5C/60) before the call. site/tests/battle/buu_collision.test.ts replays them.

usage: .venv/bin/python tools/battle/gen_buu_step_traces.py
"""
import json
import os
import struct
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
PLAYER = 0x03001C34
NODE = 0x0203FF00
RNG = 0x03004A5C
AGGRO_DEF = 0x087043F8 + 0xC  # IfPlayerWithin2 r2 of the Majin Fighter idle state 0x14B

# (name, update fn, enemy index, player x, y, frames, speed, param2)
RUNS = [
    ('backoff', 0x08018C3A, 3, 200, 200, 240, 153, 0),
    ('backoff_tree', 0x08018C3A, 3, 330, 60, 240, 204, 0),
    ('stepaway', 0x0801CBA1, 3, 280, 180, 240, 204, 0),
    ('wander', 0x0802B708, 3, 60, 300, 600, 128, 1310),
    ('walkto', 0x0802CDE8, 3, 236, 330, 400, 256, 0),
]


def main():
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    m83 = next(m for m in sprites['maps'] if m['map'] == '8-3')
    h = bc.header_for(m83)[0]
    walk = bc.bitmap(bc.u32(h + 0x34))
    out = {'source': 'tools/battle/gen_buu_step_traces.py (emulator: node updates called every frame)', 'map': '8-3', 'runs': []}
    for name, fn, idx, px, py, frames, speed, p2 in RUNS:
        e = emu('buu_fight_swforest.ss')
        e.run(2)
        # no aggro: patch the definition and every live copy of r2 = 10000 in EWRAM
        e.w32(AGGRO_DEF, 0)
        ew = np.frombuffer(e.ewram(), dtype=np.uint32)
        for i in np.nonzero(ew == 10000)[0]:
            e.w32(0x02000000 + 4 * int(i), 0)
        p = e.r32(PLAYER)
        e.w32(p + 0x1A0, px << 8)
        e.w32(p + 0x1A4, py << 8)
        e.run(2)
        en = e.r32(ENEMIES + 4 + 4 * idx)
        start = [e.r32(en + 0x1A0), e.r32(en + 0x1A4)]
        size = [e.r8(en + 0x19C), e.r8(en + 0x19D)]
        grid = np.unpackbits(np.frombuffer(e.read(e.r32(M + 0xB0), 0x2000), dtype=np.uint8).reshape(256, 32), axis=1, bitorder='little')
        own = set()
        x0, y0 = start[0] >> 8, start[1] >> 8
        for ty in range(y0 >> 3, (y0 + size[1] + 7) >> 3):
            for tx in range(x0 >> 3, (x0 + size[0] + 7) >> 3):
                own.add((tx, ty))
        marked = [[int(tx), int(ty)] for ty, tx in np.argwhere((grid == 0) & (walk == 1)) if (int(tx), int(ty)) not in own]
        for i in range(0, 0x20, 4):
            e.w32(NODE + i, 0)
        e.w32(NODE + 4, speed)
        e.w32(NODE + 8, p2)
        rows = []
        face0 = e.r8(en + 0x10)
        for f in range(frames):
            rng = [e.r32(RNG), e.r32(RNG + 4)]
            pl = [e.r32(p + 0x1A0), e.r32(p + 0x1A4)]
            b11 = e.r8(en + 0x11)
            ret = call(e, fn, (NODE, en, 0, 0))
            rows.append({'rng': rng, 'player': pl, 'b11': b11, 'pos': [e.r32(en + 0x1A0), e.r32(en + 0x1A4)], 'face': e.r8(en + 0x10),
                         'n8': struct.unpack('<i', struct.pack('<I', e.r32(NODE + 8)))[0], 'ret': ret})
        player = [e.r32(p + 0x1A0), e.r32(p + 0x1A4)]
        psize = [e.r8(p + 0x19C), e.r8(p + 0x19D)]
        out['runs'].append({'name': name, 'fn': '0x%08X' % fn, 'enemy': idx, 'start': start, 'face': face0, 'footprint': size, 'player': player,
                            'playerFootprint': psize, 'speed': speed, 'param2': p2, 'marked': marked, 'frames': rows})
        xs = [r['pos'][0] / 256 for r in rows]
        ys = [r['pos'][1] / 256 for r in rows]
        print(name, 'start', [v / 256 for v in start], 'end', xs[-1], ys[-1], 'x', min(xs), max(xs), 'y', min(ys), max(ys),
              'faces', sorted({r['face'] for r in rows}), 'rets', sorted({r['ret'] for r in rows}))
    path = os.path.join(ROOT, 'data', 'battle', 'buu_step_traces.json')
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print('wrote', path)


if __name__ == '__main__':
    main()
