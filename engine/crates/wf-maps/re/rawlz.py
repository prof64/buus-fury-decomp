import sys, struct
from r import R, wf_decompress, u32
a = int(sys.argv[1], 16)
buf = struct.pack('<II', 1, 0) + R.d[(a & 0x1FFFFFF):(a & 0x1FFFFFF) + 0x4000]
d, e = wf_decompress(buf, 0)
print(len(d), d[:32].hex(), 'stream bytes', e - 8, 'word before', hex(u32(a - 4)))
