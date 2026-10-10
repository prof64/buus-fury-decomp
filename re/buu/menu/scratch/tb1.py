import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu/emut')
from emut import emut
from cap import program
import capture
name, lo, hi = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
state, setup, pre, keys, *_ = capture.SCENARIOS[name]
e = emut(state)
if setup:
    setup(e)
for k in program(pre):
    e.set_keys(k)
    e.run(1)
A = {0x0803E378: 'sfx', 0x0803D876: 'exit', 0x0803D4A0: 'input', 0x080175FC: 'skdone', 0x08015516: 'stevent', 0x08011E0E: 'restore',
     0x0803D850: 'replace', 0x080157A4: 'stdone'}
for a in A:
    e.breakpoint(a)
e.hits()
for i, k in enumerate(program(keys)):
    e.set_keys(k)
    e.run(1)
    hs = e.hits()
    if lo <= i <= hi:
        print(i, k, ' '.join(f"{A[h['addr'] & ~1]}@{h['vcount']}" + (f"(r0={h['regs'][0]},r2={h['regs'][2]})" if A[h['addr'] & ~1] in ('exit', 'stevent') else '') for h in hs if h['kind'] == 'break'))
