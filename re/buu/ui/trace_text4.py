import sys, struct
from collections import Counter
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
e = emu('buu_menus.ss')
w = e.watch(0x02032410, 0x02032410 + 0x2800, 'w')
e.press(KEY.A, 3, 3)
e.run(30)
h = e.hits()
seen = Counter()
for x in h:
    k = (x['pc'], x['lr'])
    seen[k] += 1
    if seen[k] <= 2:
        print(hex(x['pc']), hex(x['lr']), 'addr', hex(x['addr']), 'frame', x['frame'], ' '.join(hex(r) for r in x['regs'][:13]))
# who calls IWRAM code? break at 0x030001f4-ish callers: dump the stack words at sp for first hit
x = h[0]
sp = x['regs'][13]
print('stack', [hex(e.r32(sp + 4 * i)) for i in range(32)])
