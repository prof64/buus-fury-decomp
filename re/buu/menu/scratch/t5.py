import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu/emut')
from emut import emut
from cap import program
import capture
name = sys.argv[1]
lo, hi = int(sys.argv[2]), int(sys.argv[3])
state, setup, pre, keys = capture.SCENARIOS[name]
e = emut(state)
if setup:
    setup(e)
for k in program(pre):
    e.set_keys(k)
    e.run(1)
e.breakpoint(0x08013F24)
e.breakpoint(0x0803F922)
e.hits()
for i, k in enumerate(program(keys)):
    e.set_keys(k)
    e.run(1)
    hs = [h for h in e.hits() if h['kind'] == 'break']
    if lo <= i <= hi:
        last = None
        for h in hs:
            a = h['addr'] & ~1
            if a == 0x08013F24:
                last = h['regs'][0]
            elif a == 0x0803F922 and h['lr'] & ~1 == 0x08013F56 and last:
                o = last
                base, size = e.r32(o + 0x38), e.r32(o + 0x34)
                fr = (h['regs'][1] - base) // size if size else -1
                par = e.r32(o + 4)
                print(i, hex(o), 'frame', fr, 'enabled', e.r32(o + 8), 'focus_now', hex(e.r32(par + 0x108)), 'x,y', e.r32(o + 0x24), e.r32(o + 0x28))
