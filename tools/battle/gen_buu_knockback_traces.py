"""Record enemy knockback (KnockbackSlide 0x08018A08 enter / 0x08018A92 update) in the emulator ->
data/battle/buu_knockback_traces.json.

work/states/buu_fight_swforest.ss: the 4th Majin Fighter is placed next to Goku (the fighters' aggro test is patched
off, the other three parked in a corner unless a case uses one), Goku attacks, and every call of the knockback node
is caught (re/buu/emutools.catch: the frame is replayed instruction by instruction, so memory is read exactly at the
call): at the enter the node's K (+0x0C), the hit dir byte obj+0x290, the crit flag obj+0x294 (= hit struct +0x10,
written by ComputeDamage), obj+0x298 (attack sub id 3), blocking obj+0x1C8, the RNG state; at every update the
24.8 position. Cases: a punch in each direction, an Energy Blast, a punch into a tree, a critical punch (Goku's crit
chance forced to 100 %) in the open and into a tree with another fighter standing there (wall splat hit).

usage: .venv/bin/python tools/battle/gen_buu_knockback_traces.py
"""
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
sys.path.insert(0, HERE)
from common import emu, KEY  # noqa: E402
from emutools import catch  # noqa: E402
import buu_collision as bc  # noqa: E402

M = 0x030019A4
ENEMIES = 0x03001BB0
PLAYER = 0x03001C34
REC = 0x03001560
RNG = 0x03004A5C
AGGRO_DEF = 0x087043F8 + 0xC
ENTER, UPDATE = 0x08018A08, 0x08018A92
DIRK = {'R': KEY.RIGHT, 'L': KEY.LEFT, 'U': KEY.UP, 'D': KEY.DOWN}
OFF = {'R': (18, 0), 'L': (-18, 0), 'U': (0, -10), 'D': (0, 10)}

# (name, player x, y, facing key, attack ('A' punch, 'B' Energy Blast), crit, second enemy at (x, y) or None)
CASES = [
    ('punch_right', 200, 200, 'R', 'A', False, None),
    ('punch_left', 200, 200, 'L', 'A', False, None),
    ('punch_up', 200, 200, 'U', 'A', False, None),
    ('punch_down', 200, 200, 'D', 'A', False, None),
    ('energy_blast', 200, 200, 'R', 'B', False, None),
    # the big tree's trunk: walk tiles blocked from x = 176 on this row (tile column 22)
    ('punch_into_tree', 140, 256, 'R', 'A', False, None),
    ('punch_near_tree', 134, 256, 'R', 'A', False, None),
    ('punch_near_tree2', 130, 252, 'R', 'A', False, None, 5),
    ('crit_punch', 200, 200, 'R', 'A', True, None),
    ('crit_into_tree', 140, 256, 'R', 'A', True, (166, 236)),
    ('crit_splat', 132, 256, 'R', 'A', True, (162, 270)),
]


def main():
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    m83 = next(m for m in sprites['maps'] if m['map'] == '8-3')
    walk = bc.bitmap(bc.u32(bc.header_for(m83)[0] + 0x34))
    out = {'source': 'tools/battle/gen_buu_knockback_traces.py (emulator, KnockbackSlide calls caught)', 'map': '8-3', 'runs': []}
    for case in CASES:
        name, px, py, dk, atk, crit, other = case[:7]
        wait = case[7] if len(case) > 7 else 0
        e = emu('buu_fight_swforest.ss')
        e.run(2)
        e.w32(AGGRO_DEF, 0)
        ew = np.frombuffer(e.ewram(), dtype=np.uint32)
        for i in np.nonzero(ew == 10000)[0]:
            e.w32(0x02000000 + 4 * int(i), 0)
        p = e.r32(PLAYER)
        n = e.r32(ENEMIES)
        ens = [e.r32(ENEMIES + 4 + 4 * i) for i in range(n)]
        for i, o in enumerate(ens):
            if i != 3:
                e.w32(o + 0x1A0, (40 + 24 * i) << 8)
                e.w32(o + 0x1A4, 40 << 8)
        if other:
            e.w32(ens[2] + 0x1A0, other[0] << 8)
            e.w32(ens[2] + 0x1A4, other[1] << 8)
        en = ens[3]
        e.w32(p + 0x1A0, px << 8)
        e.w32(p + 0x1A4, py << 8)
        ox, oy = OFF[dk]
        e.w32(en + 0x1A0, (px + ox) << 8)
        e.w32(en + 0x1A4, (py + oy) << 8)
        if crit:
            e.w8(REC + 0x66, 100)
            e.w32(p + 0x1BC, 100 * 656)
        e.press(DIRK[dk], 1, 1)
        e.run(3 + wait)
        # let the marks settle on the new positions, then snapshot the other objects' marks on the enemy grid
        grid = np.unpackbits(np.frombuffer(e.read(e.r32(M + 0xB0), 0x2000), dtype=np.uint8).reshape(256, 32), axis=1, bitorder='little')
        size = [e.r8(en + 0x19C), e.r8(en + 0x19D)]
        x0, y0 = e.r32(en + 0x1A0) >> 8, e.r32(en + 0x1A4) >> 8
        own = {(tx, ty) for ty in range(y0 >> 3, (y0 + size[1] + 7) >> 3) for tx in range(x0 >> 3, (x0 + size[0] + 7) >> 3)}
        marked = [[int(tx), int(ty)] for ty, tx in np.argwhere((grid == 0) & (walk == 1)) if (int(tx), int(ty)) not in own]
        # tiles freed on top of terrain walls (an enemy leaving a footprint that overlapped a wall sets them passable)
        holes = [[int(tx), int(ty)] for ty, tx in np.argwhere((grid == 1) & (walk == 0))]
        other_hp0 = e.r32(ens[2] + 0x28C)
        calls = []
        key = KEY.A if atk == 'A' else KEY.B

        def cb(e2, pc):
            c = {'pc': 'enter' if pc == ENTER else 'update', 'frame': e2.frame, 'rng': [e2.r32(RNG), e2.r32(RNG + 4)],
                 'pos': [e2.r32(en + 0x1A0), e2.r32(en + 0x1A4)], 'node': e2.reg(0), 'enemy': e2.reg(1),
                 'otherHp': e2.r32(ens[2] + 0x28C)}
            if pc == ENTER:
                nd = e2.reg(0)
                c.update({'K': e2.r32(nd + 0xC), 'hitDir': e2.r8(en + 0x290), 'crit': e2.r32(en + 0x294), 'sub3': e2.r32(en + 0x298),
                          'blocking': e2.r32(en + 0x1C8), 'face': e2.r8(en + 0x10)})
            calls.append(c)

        catch(e, [ENTER, UPDATE], 70, keyfn=lambda f: key if f < 2 else 0, cb=cb)
        allcalls = calls
        calls = [c for c in calls if c['enemy'] == en]
        first = next(i for i, c in enumerate(calls) if c['pc'] == 'enter')
        # one knockback: the first enter and its updates (until the next enter)
        run = [calls[first]]
        for c in calls[first + 1:]:
            if c['pc'] == 'enter':
                break
            run.append(c)
        after = [e.r32(en + 0x1A0), e.r32(en + 0x1A4)]
        # the other fighter's HP right after the splat: at its knockback enter that follows the main slide's last update
        hp_splat = None
        if other:
            last = allcalls.index(run[-1])
            nxt = [c for c in allcalls[last + 1:] if c['pc'] == 'enter' and c['enemy'] == ens[2]]
            hp_splat = nxt[0]['otherHp'] if nxt else None
        out['runs'].append({'name': name, 'start': run[0]['pos'], 'footprint': size, 'marked': marked, 'holes': holes, 'enter': run[0],
                            'updates': [{'frame': c['frame'], 'pos': c['pos'], 'rng': c['rng'], 'otherHp': c['otherHp']} for c in run[1:]],
                            'other': {'at': list(other), 'pos': [e.r32(ens[2] + 0x1A0), e.r32(ens[2] + 0x1A4)], 'hp_enter': run[0]['otherHp'], 'hp_before': other_hp0, 'hp_after_splat': hp_splat} if other else None,
                            'frameAfter': after})
        ent = run[0]
        print(name, 'K', ent['K'], 'dir', ent['hitDir'], 'crit', ent['crit'], 'sub3', ent['sub3'], 'block', ent['blocking'], 'updates', len(run) - 1,
              'frames', run[1]['frame'], '..', run[-1]['frame'], 'pos', [v / 256 for v in run[0]['pos']], '->', [v / 256 for v in run[-1]['pos']],
              'other hp', (other_hp0, run[-1]['otherHp'], hp_splat) if other else '')
    path = os.path.join(ROOT, 'data', 'battle', 'buu_knockback_traces.json')
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print('wrote', path)


if __name__ == '__main__':
    main()
