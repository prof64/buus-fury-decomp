"""Diagnostics for the story playthrough: replay the recorded keys of data/story_playthrough/opening.json in the
emulator (headless, muted) and dump the field object list (tools/field/rec_objects.objects: coins with position,
velocity frames, pickup box) plus the loot block (0x03001010: x, y, vx, vy) for a frame range.

usage: python3 tools/story_playthrough/objdump.py FROM TO [KIND,...]"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'script_lockstep'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from rec_objects import objects  # noqa: E402
from route import start_emu, intro_keys  # noqa: E402
from record import first_step_frame  # noqa: E402

LOOT = 0x03001010


def main():
    a, b = int(sys.argv[1]), int(sys.argv[2])
    kinds = set(sys.argv[3].split(',')) if len(sys.argv) > 3 else None
    doc = json.loads((ROOT / 'data' / 'story_playthrough' / 'opening.json').read_text())
    rows = doc['frames']
    f0 = doc['skip']
    e = start_emu('reset')
    for f in range(f0):
        e.set_keys(intro_keys(f))
        e.run(1)
    import os
    changes = os.environ.get('CHANGES')
    prev = None
    for k in range(min(b + 1, len(rows))):
        e.set_keys(rows[k]['keys'])
        e.run(1)
        if k >= a:
            s = lambda off: (e.r32(LOOT + off) ^ 0x80000000) - 0x80000000
            objs = [o for o in objects(e) if kinds is None or o[0] in kinds]
            if changes:
                key = ([s(0), s(4), s(8), s(12)], len(objs))
                if key == prev:
                    continue
                prev = key
            print(k, 'lf', e.r32(0x03000F94), 'keys', rows[k]['keys'], 'loot', [s(0), s(4), s(8), s(12)], json.dumps(objs))


if __name__ == '__main__':
    main()
