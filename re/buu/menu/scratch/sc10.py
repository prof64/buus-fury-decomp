import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
for k in program(['-:2']):
    c.step(k, keep=False)
e = c.e
keys = ['SELECT:1', '-:50', 'L:1', '-:50', 'A:1', '-:60', 'DOWN:40']
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    if i < 120 or (i > 135 and i % 10):
        continue
    W = 0x03001C4C
    for j in range(e.r32(W + 4)):
        o = e.r32(W + 8 + 4 * j)
        if e.r32(o) == 0x0804CE7C:
            print(i, 'dir', e.r8(o + 0x10), 'cel', e.r8(o + 0x11), 'tick', e.r16(o + 0x12), 'state', hex(e.r32(o + 0x14)), 'pos', e.r32(o + 0x6C), e.r32(o + 0x70), 'c', e.r32(o + 0x194), 'anim', hex(e.r32(o + 0x50)))
