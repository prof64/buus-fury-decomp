"""Profile one gameplay frame: function entries (push {..lr}) with count and min sp."""
import sys
from collections import defaultdict
from common import *
from emutools import cur_pc
e = emu(sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss')
e.run(2)
start = e.frame
cnt = defaultdict(int)
sps = {}
n = 0
while e.frame == start and n < 3_000_000:
    p = cur_pc(e)
    if 0x08000000 <= p < 0x08800000 and (e.reg(16) & 0x20):
        h = u16(p)
        if (h & 0xff00) == 0xb500 or (h & 0xff00) == 0xb400 and False:
            cnt[p] += 1
            sp = e.reg(13)
            sps[p] = max(sps.get(p, 0), sp)
    e.step()
    n += 1
print('steps', n)
for p, c in sorted(cnt.items(), key=lambda kv: -sps[kv[0]])[:40]:
    print(f'{p:08x} n={c} sp={sps[p]:08x}')
