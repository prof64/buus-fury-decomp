"""findptr.py VALUE... : 4-aligned occurrences of a u32 in the ROM"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
r = rom()
for v in sys.argv[1:]:
    pat = struct.pack('<I', int(v, 16))
    i = r.find(pat)
    out = []
    while i >= 0:
        if i % 4 == 0:
            out.append(hex(0x08000000 + i))
        i = r.find(pat, i + 1)
    print(v, len(out), out[:40])
