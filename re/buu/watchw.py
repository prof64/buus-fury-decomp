"""watchw.py state start end [mode] [frames] [keys]  -- list unique writer PCs."""
import sys
from collections import Counter
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
a = int(sys.argv[2], 16); b = int(sys.argv[3], 16)
mode = sys.argv[4] if len(sys.argv) > 4 else 'w'
frames = int(sys.argv[5]) if len(sys.argv) > 5 else 30
keys = 0
if len(sys.argv) > 6:
    for ch in sys.argv[6].split('+'):
        keys |= getattr(KEY, ch)
e.set_keys(keys)
e.watch(a, b, mode)
e.run(frames)
hs = e.hits()
c = Counter()
ex = {}
for h in hs:
    k = (h['pc'], h['lr'], h['addr'], h['access'], h['width'])
    c[k] += 1
    ex.setdefault(k, h)
for k, n in sorted(c.items(), key=lambda kv: kv[0][2]):
    h = ex[k]
    print(f"pc={k[0]:08x} lr={k[1]:08x} addr={k[2]:08x} {k[3]}{k[4]} n={n} old={h['old']:x} new={h['new']:x} r0-7={[hex(x) for x in h['regs'][:8]]}")
