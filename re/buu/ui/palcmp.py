"""palcmp.py STATEFILE... : compare palette RAM with the ROM global palette (0x080701B0, RGB888 x512) and 0x0805652C"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
G = [(u8(0x080701B0 + 3 * i), u8(0x080701B1 + 3 * i), u8(0x080701B2 + 3 * i)) for i in range(512)]
def to555(c):
    r, g, b = c
    return (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
G5 = [to555(c) for c in G]
M = [u16(0x0805652C + 2 * i) for i in range(256)]
print('global bg==obj half:', sum(G5[i] == G5[256 + i] for i in range(256)), '/256')
for f in sys.argv[1:]:
    e = emu(f)
    pr = e.palram()
    p = [struct.unpack_from('<H', pr, 2 * i)[0] for i in range(512)]
    print(f.split('/')[-1], 'bg vs global', sum(p[i] == G5[i] for i in range(256)), 'obj vs global', sum(p[256 + i] == G5[256 + i] for i in range(256)),
          'bg vs mapscr', sum(p[i] == M[i] for i in range(256)), 'obj vs mapscr', sum(p[256 + i] == M[i] for i in range(256)))
