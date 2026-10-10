import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
from strtab import S
T = 0x083bd2f8
for i in [0, 1, 2, 0x19, 0x1c, 0x1e, 0x1f, 0x20, 0x25, 0x27, 0x29, 0x4d, 0x4f, 0x52]:
    a = T + 0x34 * i
    ids = [u16(a + 2 + 2 * k) for k in range(4)]
    print(hex(i), u16(a), [S(x) for x in ids], hex(u32(a + 0x2c)), hex(u32(a + 0x30)))
