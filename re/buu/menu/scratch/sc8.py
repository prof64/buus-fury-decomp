import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
for k in program(['-:2']):
    c.step(k, keep=False)
e = c.e
tabs = [e.r32(0x0874D360 + 4 * k) for k in range(8)]
for i, k in enumerate(program(['SELECT:1', '-:60', 'L:1', '-:50'])):
    c.step(k, keep=False)
    if i < 75:
        continue
    m = e.r32(0x03000F90)
    if e.r32(m) != 0x0804A570:
        print(i, 'mode', hex(e.r32(m)))
        continue
    b = e.r32(m + 0x10 + 36)
    fx = e.r32(b + 0x18)
    print(i, 'lf', e.r32(0x03000F94), 'fx', hex(fx), 'c', e.r8(fx + 4) if fx else '-', 'remap', tabs.index(e.r32(b + 0x38)) if e.r32(b + 0x38) in tabs else hex(e.r32(b + 0x38)),
          'cel', e.r8(b + 0x11), 'tick', e.r16(b + 0x12))
