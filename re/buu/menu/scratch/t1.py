import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu/emut')
from emut import emut
from cap import program
import capture

name = sys.argv[1] if len(sys.argv) > 1 else 'status_tabs'
state, setup, pre, keys = capture.SCENARIOS[name]
e = emut(state)
if setup:
    setup(e)
for k in program(pre):
    e.set_keys(k)
    e.run(1)
REDRAW, COPY, AFTER, VB = 0x080154D4, 0x08049150, 0x0801550A, 0x0803D810
bps = [e.breakpoint(a) for a in (REDRAW, COPY, AFTER, VB)]
e.hits()
FRAME = 280896
for i, k in enumerate(program(keys)):
    c0 = e.cycles()
    e.set_keys(k)
    e.run(1)
    hs = [h for h in e.hits() if h['kind'] == 'break']
    ev = []
    for h in hs:
        a = h['addr'] & ~1
        nm = {REDRAW: 'draw', COPY: 'copy', AFTER: 'done', VB: 'vb'}[a]
        ev.append(f"{nm}@{h['cycles'] - c0}/{h['vcount']}")
    if any(not x.startswith('vb') for x in ev) or i < 3:
        print(i, k, c0 % FRAME, ' '.join(ev))
