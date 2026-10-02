"""hexd.py ADDR [LEN] ... : hex dump with u32 words"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
args = sys.argv[1:]
i = 0
while i < len(args):
    a = int(args[i], 16)
    n = 64
    if i + 1 < len(args) and not args[i + 1].startswith('0x'):
        n = int(args[i + 1]); i += 1
    i += 1
    b = rom()[ra(a):ra(a) + n]
    for o in range(0, n, 16):
        row = b[o:o + 16]
        ws = ' '.join(f'{struct.unpack_from("<I", row, k)[0]:08x}' for k in range(0, len(row) - 3, 4))
        print(f'{a + o:08x}: {row.hex(" ")}  | {ws}')
    print()
