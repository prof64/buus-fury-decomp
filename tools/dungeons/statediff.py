"""statediff.py A.ss B.ss: what the play between two states changed in GameData (flags, supplies, equipment list,
zenie, current character, levels / stat points / HP per party record)."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import level_of  # noqa: E402
from route import start_emu  # noqa: E402

GD, REC = 0x03001068, 0x03001560


def snap(path):
    e = start_emu(os.path.abspath(path))
    fl = e.read(GD + 0x55, 143)
    return {
        'flags': {f for f in range(143 * 8) if fl[f >> 3] >> (f & 7) & 1},
        'supplies': list(e.read(GD + 2, 83)),
        'equipment': e.read(GD + 0xE4, 4 + 4 * 12).hex(),
        'zenie': e.r32(GD + 0x4F4), 'char': e.r8(GD),
        'party': {c: (level_of(e, c), e.r16(REC + 0x68 * c + 0x16), e.r16(REC + 0x68 * c + 0x18),
                      e.r16(REC + 0x68 * c + 0x2C), e.read(REC + 0x68 * c + 0x3C, 16).hex()) for c in range(5)},
    }


a, b = snap(sys.argv[1]), snap(sys.argv[2])
print('flags set', sorted(b['flags'] - a['flags']), 'cleared', sorted(a['flags'] - b['flags']))
print('supplies', {i: (x, y) for i, (x, y) in enumerate(zip(a['supplies'], b['supplies'])) if x != y})
for k in ('equipment', 'zenie', 'char'):
    print(k, a[k], '->', b[k])
for c in range(5):
    print('char', c, '(level, hp, maxhp, points, slots)', a['party'][c], '->', b['party'][c])
