import sys
from collections import Counter
from common import *
REC = 0x03001560
st = sys.argv[1] if len(sys.argv) > 1 else 'buu_swforest3.ss'
mode = sys.argv[2] if len(sys.argv) > 2 else 'idle'
e = emu(st)
e.w16(REC + 0x1a, 0)
e.w16(REC + 0x16, 10)
e.watch(REC + 0x16, REC + 0x1c, 'w')
vals = []
for f in range(600):
    if mode == 'punch':
        e.set_keys(KEY.A if f % 8 < 3 else 0)
    elif mode == 'walk':
        e.set_keys(KEY.RIGHT if (f // 60) % 2 == 0 else KEY.LEFT)
    e.run(1)
    vals.append((e.r16(REC + 0x16), e.r16(REC + 0x1a)))
c = Counter()
for h in e.hits():
    c[(h['pc'], h['addr'] - REC)] += 1
print(c)
print('HP,EP every 30 frames:', vals[::30])
