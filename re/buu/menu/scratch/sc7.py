import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import u32
for i in (115, 19, 78, 61):
    a = 0x0874D380 + 16 * i
    print(i, 'def id', hex(u32(a + 4)))
for p in (0x86af578, 0x86a9da0, 0x86ae0a4, 0x86ad30c):
    print(hex(p), [hex(u32(p + 4 * k)) for k in range(4)])
