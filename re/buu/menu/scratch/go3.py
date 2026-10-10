"""words at an address: go3.py ADDR N"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import rom
r = rom()
a, n = int(sys.argv[1], 16), int(sys.argv[2])
for k in range(n):
    o = a - 0x08000000 + 4 * k
    print(hex(a + 4 * k), hex(struct.unpack('<I', r[o:o + 4])[0]))
