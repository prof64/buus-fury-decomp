"""Character survey of the text banks + inline UTF-16 strings: non-ASCII code units and their contexts."""
import sys, struct
from collections import Counter
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/tools/extract')
from common import *
from gba import wf_decompress
T = 0x087B5B64
c = Counter()
ctx = {}
for i in range(7):
    d = wf_decompress(rom(), ra(u32(T + 4 * i)))[0]
    us = struct.unpack_from(f'<{len(d)//2}H', d)
    for k, u in enumerate(us):
        if not (0x20 <= u < 0x7f):
            c[u] += 1
            if u not in ctx:
                ctx[u] = ''.join(chr(x) if 0x20 <= x < 0x7f else '{%02x}' % x for x in us[max(0, k - 20):k + 20])
    for ch in '^@#!~|*_[]{}<>$&':
        c[ch] += sum(1 for u in us if u == ord(ch))
for u, n in sorted(c.items(), key=lambda kv: str(kv[0])):
    print(repr(u), n, ctx.get(u, ''))
