import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
st = sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss'
c = Cap(st)
for k in program(['-:2', 'SELECT:1', '-:60']):
    c.step(k, keep=False)
e = c.e
# objects in the world list (0x03001C4C: +4 count, +8.. ptrs) before the scouter
W = 0x03001C4C
n = e.r32(W + 4)
print('objects', n)
for i in range(n):
    o = e.r32(W + 8 + 4 * i)
    print(i, hex(o), 'vt', hex(e.r32(o)), '+10', e.r8(o + 0x10), '+11', e.r8(o + 0x11), 'def', hex(e.r32(o + 0x58)) if 0x08000000 <= e.r32(o + 0x58) < 0x09000000 else '-', 'x', e.r32(o + 0x1a0) / 256, 'y', e.r32(o + 0x1a4) / 256)
n2 = e.r32(W + 0x40c)
print('list2', n2, [hex(e.r32(W + 0x410 + 4 * i)) for i in range(n2)])
b = e.breakpoint(0x0801030E)
e.set_keys(0x200)
for f in range(40):
    e.run(1)
    e.set_keys(0)
    for h in e.hits():
        if h['kind'] == 'break':
            o = h['regs'][4]
            print('scouted obj', hex(o), 'vt', hex(e.r32(o)), 'def', hex(e.r32(o + 0x58)), '+10', e.r8(o + 0x10), '+11', e.r8(o + 0x11), 'pos', e.r32(o + 0x1a0) / 256, e.r32(o + 0x1a4) / 256, 'anim ptr', hex(e.r32(o + 0x54)), 'f30', e.r32(o+0x30), e.r32(o+0x38))
c2 = e.r32(0x08072714 - 0x08000000 + 0x08000000)
