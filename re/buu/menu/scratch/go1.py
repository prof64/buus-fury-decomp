"""game over RE: who references script #0's header (0x0877C5A4) / the event table entry 0"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import rom
r = rom()
for v in [int(a, 16) for a in sys.argv[1:]] or [0x0877C5A4]:
    b = struct.pack('<I', v)
    i = r.find(b)
    while i >= 0:
        print(hex(v), 'at', hex(0x08000000 + i))
        i = r.find(b, i + 1)
