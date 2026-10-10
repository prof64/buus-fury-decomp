import sys; sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import *
c = Cap('buu_snakeway_start.ss')
REC = 0x03001560
c.e.w16(REC + 0x2c, 5)
c.run(['-:2', 'START:1', '-:40'])
bp = c.e.breakpoint(0x08015516)
bp2 = c.e.breakpoint(0x08013f7c)
c.e.hits()
for k in program(['A:1', '-:6']):
    c.e.set_keys(k)
    c.e.run(1)
    hs = c.e.hits()
    print(k, [(hex(h['addr']), [hex(x & 0xffffffff) for x in h['regs'][:4]]) for h in hs if h['kind'] == 'break' and h['addr'] & ~1 == 0x08015516],
          sum(1 for h in hs if h['addr'] & ~1 == 0x08013f7c))
# container state
m = c.e.r32(0x03000F90)
print('mode', hex(m), 'vt', hex(c.e.r32(m)), 'n', c.e.r32(m + 4), 'focus', hex(c.e.r32(m + 0x108)), 'sel', c.e.r32(m + 0x10c), 'drawn', c.e.r32(m + 0x110), 'alloc', c.e.r8(m + 0x130))
for i in range(c.e.r32(m + 4)):
    o = c.e.r32(m + 8 + 4 * i)
    print(i, hex(o), 'vt', hex(c.e.r32(o)), [hex(c.e.r32(o + 4 * j)) for j in range(2, 8)])
