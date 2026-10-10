import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
c = Cap(sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss')
for k in program(['-:2', 'SELECT:1', '-:60']):
    c.step(k, keep=False)
e = c.e
W = 0x03001C4C
n = e.r32(W + 4)
for i in range(n):
    o = e.r32(W + 8 + 4 * i)
    print('world', i, hex(o), 'vt', hex(e.r32(o)), '+10', e.r8(o + 0x10), '+11', e.r8(o + 0x11), '+12', e.r16(o + 0x12), 'state', hex(e.r32(o + 0x14)))
e.breakpoint(0x08027420)
e.breakpoint(0x080273be)
e.breakpoint(0x0803FAAC)
e.set_keys(0x200)
on = False
for f in range(30):
    e.run(1)
    e.set_keys(0)
    for h in e.hits():
        a = h['addr'] & ~1
        if a == 0x080273be:
            on = True
        elif a == 0x0803FAAC and on:
            print('  rand call lr', hex(h['lr']), 'r0', h['regs'][0])
        elif a == 0x08027420:
            on = False
            o = h['regs'][4]
            print('blip', hex(o), '+10', e.r8(o + 0x10), '+11', e.r8(o + 0x11), '+12', e.r16(o + 0x12), 'state', hex(e.r32(o + 0x14)))
