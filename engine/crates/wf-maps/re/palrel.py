import sys, struct
from r import R
pr = open(sys.argv[1], 'rb').read()
for i in list(range(0, 24)) + list(range(256, 264)):
    c = struct.unpack_from('<H', pr, 2 * i)[0]
    r, g, b = R.bytes(0x080701b0 + 3 * i, 3)
    print(i, (r >> 3, g >> 3, b >> 3), (c & 31, (c >> 5) & 31, c >> 10))
