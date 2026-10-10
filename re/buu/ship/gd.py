"""Print the persistent GameData of a save state: python3 gd.py STATE.ss"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import emu  # noqa: E402

GD = 0x03001068
NAMES = ['Goku', 'Gohan', 'Goten', 'Trunks', 'Vegeta', 'Gotenks', 'Vegito', 'Gogeta', 'Other']


def flags(e):
    b = e.read(GD + 0x55, 143)
    return [i for i in range(len(b) * 8) if b[i >> 3] >> (i & 7) & 1]


def dump(e):
    print('area', [e.r8(0x0300197C + i) for i in range(3)], 'char', e.r8(GD), 'zenie', e.r32(GD + 0x4F4))
    print('flags', flags(e))
    sup = e.read(GD + 2, 83)
    print('supplies', {i: c for i, c in enumerate(sup) if c})
    n = e.r16(GD + 0xE4)
    print('equipment', n, [hex(e.r32(GD + 0xE8 + 4 * i)) for i in range(min(n, 250))])
    for c in range(9):
        r = 0x03001560 + 0x68 * c
        b = e.read(r, 0x68)
        lvl = None
        print(NAMES[c], 'b0', b[0], 'form', b[0x14], 'hp', int.from_bytes(b[0x16:0x18], 'little'), '/',
              int.from_bytes(b[0x18:0x1A], 'little'), 'xp', int.from_bytes(b[0x24:0x28], 'little'),
              'skills', list(b[0xD:0xD + b[0xC]]), 'lv', list(b[0x2E:0x3C]), 'stat', list(b[0x28:0x2C]),
              'eq', [hex(int.from_bytes(b[o:o + 4], 'little')) for o in (0x3C, 0x40, 0x44, 0x48)])


if __name__ == '__main__':
    e = emu(sys.argv[1])
    dump(e)
