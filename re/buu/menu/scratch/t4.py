import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu/emut')
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
A = {0x0803D4A0: 'input', 0x080154D4: 'redraw', 0x08005FA8: 'focus', 0x08005FB2: 'unfocus', 0x08015516: 'event', 0x080067F4: 'step', 0x080150F4: 'update',
     0x08013F24: 'btndraw', 0x0803F922: 'blit'}
for a in A:
    e.breakpoint(a)
e.hits()
for i, k in enumerate(program(keys)):
    c0 = e.cycles()
    e.set_keys(k)
    e.run(1)
    hs = e.hits()
    if lo <= i <= hi:
        out = []
        for h in hs:
            if h['kind'] != 'break':
                continue
            nm = A[h['addr'] & ~1]
            if nm == 'blit':
                continue
            x = f"{nm}@{h['vcount']}"
            if nm == 'btndraw':
                x += f"(r0={h['regs'][0]:x})"
            if nm == 'event':
                x += f"(code={h['regs'][2]})"
            out.append(x)
        print(i, k, ' '.join(out))
