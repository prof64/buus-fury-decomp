"""Memory-effect checks for natives 0xA8..0xF7 (non-visual): run snippet, diff GameData/char records/eq block."""
from common import *
from script_nat_c_lib import asm, start

GD = 0x03001068
REC = GD + 0x4f8


def snap(e):
    return e.read(GD, 0x4f8 + 9 * 0x68)


def diff(a, b, label):
    out = []
    for i in range(len(a)):
        if a[i] != b[i]:
            out.append(f'GD+{i:#x}: {a[i]:02x}->{b[i]:02x}')
    print(label, out[:40])


def test(label, items, state='buu_snakeway_start.ss'):
    e = emu(state)
    e.run(2)
    a = snap(e)
    done, r, st = start(e, asm(items))
    b = snap(e)
    print('stack', st, 'done', done)
    diff(a, b, label)
    return e


cur = None
e = emu('buu_snakeway_start.ss'); e.run(2)
print('cur char', e.r8(GD), 'eq slots', [hex(e.r32(REC + 0x3c + 4 * k)) for k in range(4)],
      'level/xp', e.r32(REC + 0x24), 'statpts', e.r16(REC + 0x2c))
test('f1 unequip(0,1)', [0, 1, ('n', 0xf1)])
test('c8 level', [('n', 0xc8)])
test('c3 xp', [('n', 0xc3)])
test('dd xp sum', [('n', 0xdd)])
test('c2 give_xp 1000', [1000, ('n', 0xc2)])
test('f3 level_to(0,60)', [0, 60, ('n', 0xf3)])
test('ca heal_all', [('n', 0xca)])
test('ee add_stat_pts(0,5)', [0, 5, ('n', 0xee), 0, ('n', 0xed)])
test('e9 set_skill(0,sk,3)', [0, 1, 3, ('n', 0xe9), 0, 1, ('n', 0xea)])
test('f2 perm(0,1,2,3,4)', [0, 1, 2, 3, 4, ('n', 0xf2)])
test('f2 timed(30,25..)', [30, 25, 25, 25, 25, ('n', 0xf2)])
test('f7 timed flag 10', [10, ('n', 0xf7)])
test('d0 form 1', [1, ('n', 0xd0)])
test('ba get flags(13,3)', [13, 3, ('n', 0xba)])
test('bb set flags(293,13,1234)', [293, 13, 1234, ('n', 0xbb), 293, 13, ('n', 0xba)])
test('a8 fuse(5,0,2,3,300)', [5, 0, 2, 3, 300, ('n', 0xa8)])
test('b3 sin(1000,64)', [1000, 64, ('n', 0xb3), 1000, 64, ('n', 0xb4), 1000, 0, ('n', 0xb4)])
test('d3/de strings', [0, ('n', 0xd3), 541, ('n', 0xde)])
test('d2/f6 flags', [('n', 0xd2), ('n', 0xf6)])
test('eb bit40', [('n', 0xeb)])
