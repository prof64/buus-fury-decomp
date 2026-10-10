import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
import capture
name, lo, hi = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
state, setup, pre, keys, *rest = capture.SCENARIOS[name]
c = Cap(state)
if setup:
    setup(c.e)
for k in program(pre):
    c.step(k, keep=False)
inv = None
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    e = c.e
    m = e.r32(0x03000F90)
    if e.r32(m) == 0x08049D48:
        inv = m
    if lo <= i <= hi and inv:
        cur = e.r32(inv + 0x100 + 0x30)
        print(i, k, 'lf', e.r32(0x03000F94), 'mode', hex(e.r32(m)), 'cursor t', e.r32(cur + 8), 'sel', e.r32(inv + 0x10C), 'top/cur', e.r32(inv + 0x120), e.r32(inv + 0x124))
