"""The sprite frame records' boxes -> data/battle/buu_frame_boxes.json.

Every animation step of data/buu/sprites.json names its ROM frame record (`rom`). The record's bytes +4..+7 are a box
relative to the actor's position: s8 ox, s8 oy, u8 w, u8 h. The game reads it through 0x0800FE34 ([obj+0x54] = the
shown frame record; box = (x >> 8) + ox, (y >> 8) + oy, + w, + h) for aiming and contact tests that use the picture
rather than the body box (the Kamikaze ghosts' homing 0x08038F2C and IfContactExplode 0x080390A0).

usage: .venv/bin/python tools/battle/gen_buu_frame_boxes.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import s8, u8  # noqa: E402


def main():
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    roms = set()
    for e in sprites['entities']:
        for f in e.get('frames') or []:
            if f.get('rom'):
                roms.add(int(f['rom'], 16))
        for a in (e.get('animations') or {}).values():
            for steps in (a.get('directions') or {}).values():
                for st in steps or []:
                    if st.get('rom'):
                        roms.add(int(st['rom'], 16))
    boxes = {}
    for r in sorted(roms):
        if not 0x08000000 <= r < 0x08800000:
            continue
        boxes['%08X' % r] = [s8(r + 4), s8(r + 5), u8(r + 6), u8(r + 7)]
    out = {'source': 'tools/battle/gen_buu_frame_boxes.py (ROM frame records +4..+7: s8 ox, s8 oy, u8 w, u8 h)', 'boxes': boxes}
    path = os.path.join(ROOT, 'data', 'battle', 'buu_frame_boxes.json')
    with open(path, 'w') as f:
        json.dump(out, f, separators=(',', ':'))
    print(len(boxes), 'boxes ->', path)


if __name__ == '__main__':
    main()
