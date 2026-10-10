import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu/emut')
from emut import emut
from cap import program
FRAME = 280896
for st in sys.argv[1:]:
    e = emut(st)
    e.run(2)
    pts = {0x0802574C: 'setup', 0x08025B1E: 'setup_end', 0x0803D7FA: 'loop', 0x08024C38: 'update', 0x0803F296: 'vblankwait', 0x080256F0: 'ctor'}
    for a in pts:
        e.breakpoint(a)
    e.hits()
    c0 = None
    for i, k in enumerate(program(['SELECT:1', '-:24'])):
        cs = e.cycles()
        e.set_keys(k)
        e.run(1)
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            nm = pts.get(h['addr'] & ~1)
            if nm in ('setup', 'setup_end', 'ctor') or (nm == 'update' and i > 16 and i < 24):
                print(st, i, nm, h['cycles'] - cs, 'line', h['vcount'])
