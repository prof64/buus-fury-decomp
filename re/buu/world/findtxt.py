import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
r = rom()
for w in sys.argv[1:]:
    for enc in ('utf-16-le', 'latin-1'):
        pat = w.encode(enc)
        i = r.find(pat)
        n = 0
        while i >= 0 and n < 8:
            print(w, enc, hex(0x08000000 + i))
            n += 1
            i = r.find(pat, i + 1)
