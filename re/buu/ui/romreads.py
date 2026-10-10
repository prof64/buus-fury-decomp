"""romreads.py STATE [keys...] : ROM data reads (outside code) during the key sequence, clustered."""
import sys
from collections import defaultdict
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui')
from common import *
from cap import drive

e = emu(sys.argv[1])
keys = sys.argv[2:]
pre = [k for k in keys if k.startswith('pre:')]
keys = [k for k in keys if not k.startswith('pre:')]
if pre:
    drive(e, [p[4:].replace('_', ':') for p in pre[0].split(',')]) if False else None
w = e.watch(0x08056000, 0x08800000, 'r')
spans = defaultdict(lambda: [1 << 40, 0, 0, set()])
for k in keys:
    drive(e, [k])
    for h in e.hits():
        a = h['addr']
        key = a >> 10
        s = spans[key]
        s[0] = min(s[0], a); s[1] = max(s[1], a); s[2] += 1
        if len(s[3]) < 4:
            s[3].add(h['pc'])
e.clear(w)
# merge adjacent KB blocks
ks = sorted(spans)
merged = []
for k in ks:
    s = spans[k]
    if merged and k - merged[-1][4] <= 1:
        m = merged[-1]
        m[1] = max(m[1], s[1]); m[2] += s[2]; m[3] |= s[3]; m[4] = k
    else:
        merged.append([s[0], s[1], s[2], set(s[3]), k])
for m in merged:
    print(f'{m[0]:08x}-{m[1]:08x} n={m[2]:6d} pcs={",".join(hex(p) for p in sorted(m[3])[:4])}')
