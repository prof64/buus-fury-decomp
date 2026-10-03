import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
for k in program(['-:2']):
    c.step(k, keep=False)
e = c.e
io = lambda a: e.r16(0x04000000 + a)
keys = ['SELECT:1', '-:50', 'L:1', '-:50', 'A:1', '-:60', 'DOWN:40', '-:10', 'UP:20', '-:10', 'B:1', '-:70']
show = set(int(x) for x in sys.argv[1].split(',')) if len(sys.argv) > 1 else set()
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    if i < 100 or i > 260:
        continue
    m = e.r32(0x03000F90)
    vt = e.r32(m)
    extra = ''
    if vt == 0x0804A52C:
        extra = 'scroll %d max %d c %d d %d' % (e.r32(m + 0x10), e.r32(m + 0x14), e.r8(m + 0xC), e.r8(m + 0xD))
        for a in (e.r32(m + 0x24), e.r32(m + 0x28)):
            extra += ' arrow[%s]' % ' '.join('%x' % e.r8(a + j) for j in range(0x4, 0x20))
    print(i, k, hex(vt), 'lf', e.r32(0x03000F94), 'DISP %04x' % io(0), 'BG %s' % ['%04x' % io(8 + 2 * j) for j in range(4)], 'BLD %04x %04x %04x' % (io(0x50), io(0x52), io(0x54)), extra)
    if i in show:
        for j in range(128):
            a0, a1, a2 = e.r16(0x07000000 + 8 * j), e.r16(0x07000002 + 8 * j), e.r16(0x07000004 + 8 * j)
            if (a0 & 0x300) == 0x200:
                continue
            print('   obj', j, hex(a0), hex(a1), hex(a2), 'y', a0 & 0xff, 'x', a1 & 0x1ff, 'tile', a2 & 0x3ff, 'prio', (a2 >> 10) & 3)
