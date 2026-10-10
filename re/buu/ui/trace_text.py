"""Advance the dialog in buu_menus.ss and log who writes the text OBJ tiles."""
import sys
from collections import Counter
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
e = emu('buu_menus.ss')
e.press(KEY.A, 3, 3)
w = e.watch(0x06015800, 0x06018000, 'w')
rd = None
e.run(40)
h = e.hits()
c = Counter((x['pc'], x['lr']) for x in h)
for (pc, lr), n in c.most_common(20):
    print(f'pc={pc:08x} lr={lr:08x} n={n}')
print(len(h))
e.clear(w)
e.screenshot('/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/ui/out/menus2.png', scale=1)
# read accesses to ROM during that: rerun with ROM read watch
e = emu('buu_menus.ss')
e.press(KEY.A, 3, 3)
w = e.watch(0x08000000, 0x08800000, 'r')
e.run(6)
h = e.hits()
c = Counter((x['pc'] & ~0xff, x['addr'] & ~0xfff) for x in h)
for (pc, a), n in c.most_common(40):
    print(f'pc~{pc:08x} addr~{a:08x} n={n}')
