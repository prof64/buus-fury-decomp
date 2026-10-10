"""findbl.py ADDR [ADDR..]: every Thumb BL in the ROM's code that calls one of the addresses; with -f, the start of
the function containing each address instead (the nearest push before it)"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent / 're' / 'buu'))
from common import rom, u16  # noqa: E402

args = [a for a in sys.argv[1:] if a != '-f']
if '-f' in sys.argv:
    for s in args:
        a = int(s, 16) & ~1
        b = a
        while b > a - 0x600 and u16(b) & 0xFE00 != 0xB400:
            b -= 2
        print(s, 'func', hex(b))
    sys.exit()
r = rom()
targets = [int(x, 16) & ~1 for x in args]
for a in range(0, min(len(r), 0x80000) - 4, 2):
    hi = r[a] | r[a + 1] << 8
    lo = r[a + 2] | r[a + 3] << 8
    if hi & 0xF800 == 0xF000 and lo & 0xF800 == 0xF800:
        off = ((hi & 0x7FF) << 12) | ((lo & 0x7FF) << 1)
        if off & 0x400000:
            off -= 0x800000
        t = 0x08000000 + a + 4 + off
        if t in targets:
            print(hex(0x08000000 + a), '->', hex(t))
