import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
from strtab import table, ustr
t = table()
print(len(t), hex(min(t)), hex(max(t)))
import random
for i in list(range(0, 30)) + list(range(700, 720)) + list(range(1400, 1534, 7)):
    print(i, hex(t[i]), ustr(t[i])[:150])
