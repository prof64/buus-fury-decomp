"""Diagnostics: replay the playthrough's keys and report every write to the loot block 0x03001010..0x0300101F (pc,
value) with the enemies' positions (object +0x1A0 / +0x1A4) and footprint fields, in a frame range.

usage: python3 tools/story_playthrough/lootwatch.py FROM TO"""
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

LOOT = 0x03001010


def main():
    a, b = int(sys.argv[1]), int(sys.argv[2])
    doc = json.loads((ROOT / 'data' / 'story_playthrough' / 'opening.json').read_text())
    rows = doc['frames']
    f0 = doc['skip']
    e = start_emu('reset')
    for f in range(f0):
        e.set_keys(intro_keys(f))
        e.run(1)
    wid = None
    for k in range(min(b + 1, len(rows))):
        if k == a:
            wid = e.watch(LOOT, LOOT + 16, 'w')
            e.hits()
        before = [o for o in objects(e) if o[0] == 'enemy'] if k >= a else None
        e.set_keys(rows[k]['keys'])
        e.run(1)
        if k >= a:
            hs = e.hits()
            if hs:
                print(k, 'enemies before', before)
                for h in hs:
                    print('   ', {kk: (hex(v) if isinstance(v, int) else v) for kk, v in h.items()})
                print('   loot now', [(e.r32(LOOT + 4 * i) ^ 0x80000000) - 0x80000000 for i in range(4)])
    if wid is not None:
        e.clear(wid)


if __name__ == '__main__':
    main()
