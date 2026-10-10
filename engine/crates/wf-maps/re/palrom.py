import sys, struct
from r import R
pr = open(sys.argv[1], 'rb').read()
pal = [struct.unpack_from('<H', pr, 2 * i)[0] for i in range(512)]
base = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0x080701b0
bad = []
for i in range(512):
    r, g, b = R.bytes(base + 3 * i, 3)
    c = (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
    if c != pal[i]:
        bad.append(i)
print(len(bad), 'mismatches', bad[:40], bad[-10:] if bad else '')
