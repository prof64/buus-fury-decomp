"""objwatch.py SEG.json STATE.ss A B TYPE OFF[,OFF..]  -- replay a segment's keys on the real game; for rows A..B print
the clock (0x03000F94) and the given 32-bit fields (hex offsets) of every enemy of TYPE (enemy list 0x03001BB0)"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu  # noqa: E402

rows = []
for ln in open(sys.argv[1]).read().splitlines()[1:]:
    try:
        rows.append(json.loads(ln.strip().rstrip(',')))
    except ValueError:
        pass
e = start_emu(sys.argv[2])
a, b, ty = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
offs = [int(x, 16) for x in sys.argv[6].split(',')]


def enemies():
    n = e.r32(0x03001BB0 + 4)
    out = []
    for k in range(min(n, 64)):
        o = e.r32(0x03001BB0 + 8 + 4 * k)
        if (e.r32(o + 0x224) - 0x08711274) // 28 == ty:
            out.append(o)
    return out


for i, r in enumerate(rows[:b + 1]):
    e.set_keys(r['keys'])
    e.run(1)
    if i >= a:
        print(i, 'clock', e.r32(0x03000F94), [[hex(o)] + [hex(e.r32(o + f)) for f in offs] for o in enemies()])
