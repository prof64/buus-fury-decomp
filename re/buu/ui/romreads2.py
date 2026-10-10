"""romreads2.py STATE START END [keys...] : for reads in [START,END) print (pc, lr, regs) distinct by lr"""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui')
from common import *
from cap import drive
e = emu(sys.argv[1])
lo, hi = int(sys.argv[2], 16), int(sys.argv[3], 16)
w = e.watch(lo, hi, 'r')
seen = {}
for k in sys.argv[4:]:
    drive(e, [k])
    for h in e.hits():
        key = (h['pc'] & ~0xff, h['lr'])
        if key not in seen:
            seen[key] = h
for (pc, lr), h in seen.items():
    sp = h['regs'][13]
    stack = [e.r32(sp + 4 * i) for i in range(40)]
    rets = [hex(s) for s in stack if 0x08000000 <= s < 0x08800000 and s & 1]
    print(f'pc={h["pc"]:08x} lr={lr:08x} addr={h["addr"]:08x} frame={h["frame"]} regs={" ".join(hex(r) for r in h["regs"][:13])}\n   stack-rets {rets[:10]}')
