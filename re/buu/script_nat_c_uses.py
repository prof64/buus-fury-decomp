"""script_nat_c_uses.py [lo hi] -- per-native usage counts + example contexts from work/buu/scripts_all.txt."""
import re
import sys
from collections import defaultdict
from common import *

lo = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0xa8
hi = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0xf7
nex = int(sys.argv[3]) if len(sys.argv) > 3 else 3
lines = (WORK / 'scripts_all.txt').read_text().split('\n')
uses = defaultdict(int)
scripts = defaultdict(set)
ex = defaultdict(list)
cur = None
prev = []
for l in lines:
    if l.startswith('==='):
        cur = int(l.split('#')[1].split()[0])
        prev = []
        continue
    s = l.split(': ', 1)[1] if ': ' in l else l
    m = re.match(r'native 0x([0-9a-f]+)', s)
    if m:
        n = int(m.group(1), 16)
        uses[n] += 1
        scripts[n].add(cur)
        if len(ex[n]) < nex:
            ex[n].append(f'#{cur}: ' + ' | '.join(prev[-4:]))
    prev.append(s.strip())
for n in range(lo, hi + 1):
    print(f'0x{n:02x} uses={uses[n]} scripts={sorted(scripts[n])[:12]}')
    for e in ex[n]:
        print('     ', e)
