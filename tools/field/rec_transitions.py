"""Record area-transition ground truth: data/field/transitions_<name>.json (format: see common_field.record and
the notes section "Field: area loading and transitions" in re/buu/notes.md).

usage: .venv/bin/python tools/field/rec_transitions.py [name ...] [--print]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common_field import KEY, ROOT, arm_warp, follow_warp, record  # noqa: E402

OUT = os.path.join(ROOT, 'data', 'field')
U, D, L, R, A = KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.A


def warp_to(w, m, x, y, variant=0):
    def setup(e):
        arm_warp(e, w, m, x, y, variant)
        return follow_warp
    return setup


SCENARIOS = [
    # (a) arrive in Southwest Forest 8-3 at (88, 60), walk up into the door exit (64..113 x 0..17, push up) -> 8-6 at
    # (72, 364); then 120 frames in 8-6 (idle, then walking right)
    dict(name='forest_up_exit', state='buu_snakeway_start.ss', setup=warp_to(8, 3, 88, 60),
         input=[[0, 20], [U, 140], [0, 60], [R, 60]],
         notes='warp-arrive 8-3 (88,60); UP into the 8-3 -> 8-6 door exit; idle + walk right in 8-6'),
    # (c) 8-3 -> 8-6 -> back down into 8-3 (enemies of 8-3 are created again on re-entry)
    dict(name='forest_roundtrip', state='buu_snakeway_start.ss', setup=warp_to(8, 3, 88, 60),
         input=[[U, 160], [D, 150], [0, 80]],
         notes='8-3 -> 8-6 (door, push up) -> 8-3 (door 40..113 x 376..393 in 8-6, push down, arrive (88, 28))'),
    # (b) screen-edge exit: Hercule City 3-2, the top edge (48..145 x 0..17, push up) -> 3-3 relative (48, 296)
    dict(name='hercule_edge_up', state='buu_snakeway_start.ss', setup=warp_to(3, 2, 100, 40),
         input=[[0, 10], [U, 140], [0, 60]],
         notes='warp-arrive 3-2 (100,40); UP into the top edge exit -> 3-3 at player - (48,0) + (48,296)'),
    # (d) a door whose arrival facing nibble is set: Thieves Den 9-8 (256..273 x 224..265, push right) -> 9-9 at
    # (112, 148) facing up
    dict(name='thieves_door_facing', state='buu_snakeway_start.ss', setup=warp_to(9, 8, 230, 264),
         input=[[0, 10], [R, 140], [0, 60]],
         notes='warp-arrive 9-8 (230,264); RIGHT into the door (push right, arrival facing nibble = up) -> 9-9 (112,148)'),
]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    os.makedirs(OUT, exist_ok=True)
    for s in SCENARIOS:
        if args and s['name'] not in args:
            continue
        t = record(s['name'], s['state'], s['setup'], s['input'], s['notes'])
        path = os.path.join(OUT, f"transitions_{s['name']}.json")
        with open(path, 'w') as fh:
            json.dump(t, fh, separators=(',', ':'))
        print(path, len(t['frames']), 'frames', 'loads', [(L['f'], L['to'], L['dest']) for L in t['loads']])
        if '--print' in sys.argv:
            prev = None
            for r in t['frames']:
                p = r['p']
                key = (tuple(r['area']), r['logic'], p[2] if p else None, p[3] if p else None)
                line = (f"{r['f']:4d} {'L' if r['logic'] else '-'} area {r['area']} p {p[0]/256 if p else '-'},{p[1]/256 if p else '-'}"
                        f" d {p[2] if p else '-'} {p[3] if p else '-'} rng {r['rng'][0]:08x} cam {r['cam'][0]/1024:.2f},{r['cam'][1]/1024:.2f}"
                        f" n {len(r['objs'])}{' LOAD' if 'load' in r else ''}")
                if key != prev or 'load' in r or r['f'] % 20 == 0:
                    print(line)
                prev = key


if __name__ == '__main__':
    main()
