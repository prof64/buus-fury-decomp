"""Locate the text bank in the EWRAM dump and test text refs from dialog sections."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
ram = open('/tmp/ewram.bin', 'rb').read()
iw = open('/tmp/iwram.bin', 'rb').read()
# find text region start: walk back from 0x1cf08 while u16 looks like text/control
i = 0x1cf08
while i > 0:
    c = struct.unpack_from('<H', ram, i - 2)[0]
    if c == 0 or (c > 0x7e and c < 0xa0) or c > 0x2200:
        break
    i -= 2
print('start ~', hex(0x02000000 + i), repr(ram[i:i + 120].decode('utf-16-le', 'replace')))
j = 0x1cf08
while j < len(ram) - 2:
    c = struct.unpack_from('<H', ram, j)[0]
    if c == 0:
        break
    j += 2
print('end ~', hex(0x02000000 + j), 'size', hex(j - i))
# pointers to the region in RAM
for base in (i, i - 4, i - 8):
    v = 0x02000000 + base
    for nm, mem, b0 in (('ew', ram, 0x02000000), ('iw', iw, 0x03000000)):
        k = mem.find(struct.pack('<I', v))
        while k >= 0:
            print('ptr to', hex(v), 'at', hex(b0 + k))
            k = mem.find(struct.pack('<I', v), k + 1)
# test the refs in 0x0877df04 dialog
def txt(v):
    off = (v & 0x3FFFFF) >> 6
    n = v >> 22
    return ram[i + off:i + off + 2 * n].decode('utf-16-le', 'replace')
for v in (0x03c6f500, 0x01e6fc80, 0x10070040, 0x00c72040, 0x01c721c0):
    print(hex(v), repr(txt(v)))
