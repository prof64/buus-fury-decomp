"""Trigger activation-mode singletons in IWRAM: print vtables."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from vtab import entries
e = emu('buu_snakeway_start.ss')
for a in range(0x03001930, 0x03001968, 4):
    v = e.r32(a)
    print(hex(a), hex(v), [hex(x) for x in entries(v, 4)] if 0x08000000 <= v < 0x08800000 else '')
print('0x08049d84', [hex(x) for x in entries(0x08049d84, 4)])
