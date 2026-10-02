"""watch.py STATE START END MODE FRAMES [keys] : print unique (pc,lr) of accesses."""
import sys
from common import *
e = emu(sys.argv[1])
s, en = int(sys.argv[2], 0), int(sys.argv[3], 0)
mode = sys.argv[4]
fr = int(sys.argv[5])
keys = 0
if len(sys.argv) > 6:
    for n in sys.argv[6].split('+'):
        keys |= getattr(KEY, n)
e.set_keys(keys)
e.watch(s, en, mode)
e.run(fr)
seen = {}
for h in e.hits():
    k = (h['pc'], h['lr'], h['addr'], h['access'])
    if k not in seen:
        seen[k] = h
    seen[k]['n'] = seen[k].get('n', 0) + 1
for k, h in sorted(seen.items()):
    print(f"pc={h['pc']:08x} lr={h['lr']:08x} addr={h['addr']:08x} {h['access']} w{h['width']} {h['old']:#x}->{h['new']:#x} n={h['n']} f={h['frame']} r0-r7={' '.join('%x'%r for r in h['regs'][:8])}")
