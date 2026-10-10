import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
c = Cap(sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss')
for k in program(['-:2']):
    c.step(k, keep=False)
e = c.e
W = 0x03001C4C
n = e.r32(W + 4)
for i in range(n):
    o = e.r32(W + 8 + 4 * i)
    d = e.r32(o + 0x58)
    print(i, hex(o), 'vt', hex(e.r32(o)), 'def', hex(d) if 0x08000000 <= d < 0x09000000 else '-', '+10', e.r8(o + 0x10), '+11', e.r8(o + 0x11),
          'pos', e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256)
m = 0x030019A4
print('cam', [e.r32(m + k) for k in range(0x20, 0x40, 4)])
