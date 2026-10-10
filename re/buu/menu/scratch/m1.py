import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
c = Cap('buu_snakeway_start.ss')
keys = program(['-:2', 'SELECT:1', '-:50'] + (sys.argv[1].split(',') if len(sys.argv) > 1 else []))
for k in keys:
    c.step(k, keep=False)
e = c.e
io = lambda a: e.r16(0x04000000 + a)
print('DISPCNT %04x' % io(0), 'BG0-3CNT', ['%04x' % io(8 + 2 * i) for i in range(4)])
print('scroll', [(io(0x10 + 4 * i), io(0x12 + 4 * i)) for i in range(4)])
print('BLDCNT %04x BLDALPHA %04x BLDY %04x' % (io(0x50), io(0x52), io(0x54)))
for i in range(128):
    a0, a1, a2 = e.r16(0x07000000 + 8 * i), e.r16(0x07000002 + 8 * i), e.r16(0x07000004 + 8 * i)
    if (a0 & 0x300) == 0x200:
        continue
    print('obj', i, hex(a0), hex(a1), hex(a2), 'y', a0 & 0xff, 'x', a1 & 0x1ff, 'tile', a2 & 0x3ff, 'prio', (a2 >> 10) & 3, 'pal', a2 >> 12)
m = e.r32(0x03000F90)
print('mode', hex(m), 'vt', hex(e.r32(m)), 'scroll', e.r32(m + 0x802c), e.r32(m + 0x8030), 'rect', [e.r32(m + 0x801c + 4 * k) for k in range(4)], [e.r32(m + 0x800c + 4 * k) for k in range(4)], 'rec', hex(e.r32(m + 0x8040)))
