import sys, struct
from collections import Counter
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
e = emu('buu_menus.ss')
w = e.watch(0x02032410, 0x02032410 + 0x2800, 'w')
r = e.watch(0x0806e05c, 0x0806ef00, 'r')
e.press(KEY.A, 3, 3)
e.run(30)
h = e.hits()
c = Counter((x['kind'], x['access'], x['pc'], x['lr']) for x in h)
for k, n in c.most_common(30):
    print(k[0], k[1], hex(k[2]), hex(k[3]), n)
rd = [x for x in h if x['access'] == 'r']
print('reads addr range', hex(min(x['addr'] for x in rd)) if rd else None, hex(max(x['addr'] for x in rd)) if rd else None)
