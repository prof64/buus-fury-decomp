import sys, re
from collections import Counter
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from strtab import table, ustr
c = Counter()
ex = {}
for i, p in enumerate(table()):
    s = ustr(p)
    for m in re.findall(r'\{([0-9a-f]{4})\}', s):
        c[m] += 1
        ex.setdefault(m, (i, s[:100]))
for k, n in c.most_common():
    print(k, n, ex[k])
