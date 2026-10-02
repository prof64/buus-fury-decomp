"""vramw.py STATE LO HI [keys...] : who writes VRAM [LO,HI) (distinct pc/lr, first regs)"""
import sys
from collections import Counter
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui')
from common import *
from cap import drive
e = emu(sys.argv[1])
lo, hi = int(sys.argv[2], 16), int(sys.argv[3], 16)
w = e.watch(lo, hi, 'w')
seen = {}
cnt = Counter()
for k in sys.argv[4:]:
    drive(e, [k])
    for h in e.hits():
        key = (h['pc'], h['lr'])
        cnt[key] += 1
        if key not in seen:
            seen[key] = h
for key, h in seen.items():
    print(f'pc={key[0]:08x} lr={key[1]:08x} n={cnt[key]} first addr={h["addr"]:08x} frame={h["frame"]} regs={" ".join(hex(r) for r in h["regs"][:13])}')
print('DMA3 SAD', hex(e.r32(0x040000D4)), 'DAD', hex(e.r32(0x040000D8)))
