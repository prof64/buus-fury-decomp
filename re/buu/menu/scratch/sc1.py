import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
st = sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss'
c = Cap(st)
for k in program(['-:2', 'SELECT:1', '-:60', 'L:1', '-:60'] + (sys.argv[2].split(',') if len(sys.argv) > 2 else [])):
    c.step(k, keep=False)
e = c.e
m = e.r32(0x03000F90)
print('mode', hex(m), hex(e.r32(m)))
n = e.r32(m + 0xC)
for i in range(n):
    a = m + 0x10 + 40 * i
    w = [e.r32(a + 4 * j) for j in range(10)]
    print(i, [hex(x) if x > 0xffff else x for x in w])
    b = w[9]
    print('   blip vt', hex(e.r32(b)), '+8', hex(e.r8(b + 8)), '+10', e.r8(b + 0x10), '+11', e.r8(b + 0x11), 'anim', hex(e.r32(b + 0x54)), 'pos', e.r32(b+0x6c), e.r32(b+0x70), '+12', e.r16(b+0x12), '+30 +38', e.r32(b+0x30), e.r32(b+0x38), 'def', hex(e.r32(b+0x58)))
for off in (0x510, 0x514, 0x518, 0x51c):
    print(hex(off), hex(e.r32(m + off)))
cur = e.r32(m + 0x51c)
print('cursor', e.r32(cur + 0xc10) / 256, e.r32(cur + 0xc14) / 256, [e.r32(cur + 0xc18 + 4 * k) for k in range(4)], 'vt', hex(e.r32(cur)))
io = lambda a: e.r16(0x04000000 + a)
print('DISPCNT %04x' % io(0), ['%04x' % io(8 + 2 * i) for i in range(4)], 'scroll', [(io(0x10 + 4 * i), io(0x12 + 4 * i)) for i in range(4)], 'BLD %04x %04x' % (io(0x50), io(0x52)))
for i in range(128):
    a0, a1, a2 = e.r16(0x07000000 + 8 * i), e.r16(0x07000002 + 8 * i), e.r16(0x07000004 + 8 * i)
    if (a0 & 0x300) == 0x200:
        continue
    print('obj', i, hex(a0), hex(a1), hex(a2), 'y', a0 & 0xff, 'x', a1 & 0x1ff, 'tile', a2 & 0x3ff, 'prio', (a2 >> 10) & 3, 'pal', a2 >> 12)
c.e.screenshot('/tmp/wfd/out/sc1.png', scale=2)
