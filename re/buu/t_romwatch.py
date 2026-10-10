from collections import Counter
from common import *
from warp import warp
e = emu('buu_snakeway_start.ss')
e.watch(0x087119e4, 0x08711a00, 'r')
warp(e, 8, 3, 0x80, 0x80)
for _ in range(10):
    e.set_keys(KEY.A); e.run(4); e.set_keys(0); e.run(4)
c = Counter()
for h in e.hits():
    c[(h['pc'], h['addr'] - 0x087119e4, h['width'])] += 1
for k, n in sorted(c.items()):
    print(hex(k[0]), 'off', hex(k[1]), 'w', k[2], 'n', n)
