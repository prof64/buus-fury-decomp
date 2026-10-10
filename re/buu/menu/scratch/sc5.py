import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
for k in program(['-:2']):
    c.step(k, keep=False)
e = c.e
prev = (e.r32(0x03004A5C), e.r32(0x03004A60)); print("start", prev)
for i, k in enumerate(program(['SELECT:1', '-:60', 'L:1', '-:60', 'L:1', '-:20', 'R:1', '-:60', 'SELECT:1', '-:70'])):
    c.step(k, keep=False)
    s = (e.r32(0x03004A5C), e.r32(0x03004A60))
    if s != prev:
        print(i, k, hex(s[0]), hex(s[1]))
    prev = s
