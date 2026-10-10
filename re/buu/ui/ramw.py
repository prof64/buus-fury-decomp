"""ramw.py STATE LO HI [keys...] : writes into [LO,HI) grouped by (pc, lr); shows addr range and sample regs"""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui')
from common import *
from cap import drive
e = emu(sys.argv[1])
lo, hi = int(sys.argv[2], 16), int(sys.argv[3], 16)
keys = sys.argv[4:]
w = e.watch(lo, hi, 'w')
g = {}
for k in keys:
    drive(e, [k])
    for h in e.hits():
        key = (h['pc'], h['lr'])
        s = g.setdefault(key, [0, 1 << 40, 0, h])
        s[0] += 1; s[1] = min(s[1], h['addr']); s[2] = max(s[2], h['addr'])
for key, s in sorted(g.items(), key=lambda kv: -kv[1][0])[:60]:
    h = s[3]
    print(f'pc={key[0]:08x} lr={key[1]:08x} n={s[0]:6d} {s[1]:08x}-{s[2]:08x} f={h["frame"]} r0-7={" ".join(hex(r) for r in h["regs"][:8])}')
