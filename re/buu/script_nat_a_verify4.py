"""Emulator checks: spawn map object (0x36) world-count delta, collision grid natives 0x2b..0x2e (which grid,
bit polarity), sfx-on-track 0x4b, count/find 0x37 after spawns."""
from script_nat_a_lib import asm, run_code
from script_nat_a_emu import *

e = emu('buu_snakeway_start.ss')
e.run(2)
base = e.save()
SCN = 0x030019A4
grids = [e.r32(SCN + 0xA8), e.r32(SCN + 0xAC), e.r32(SCN + 0xB0)]
print('grid ptrs', [hex(g) for g in grids])
area = e.r32(0x0300197C + 4)
tbl = e.r32(area + 0x14)
print('area obj', hex(area), 'obj table', hex(tbl), [hex(e.r32(tbl + 4 * k)) for k in range(6)])


def bit(g, tx, ty):
    return (e.r32(g + 32 * ty + 4 * (tx >> 5)) >> (tx & 31)) & 1


P = e.r32(PLAYER_PTR)
px, py = map(int, pos(e, P))
tx, ty = px // 8, py // 8
print('player tile', tx, ty, 'bits', [bit(g, tx, ty) for g in grids])
for nat in (0x2b, 0x2c, 0x2d, 0x2e):
    e.load(base)
    run_code(e, asm([px - 8, py - 8, px + 8, py + 8, ('n', nat)]))
    print(f'native {nat:#x}: bits after', [bit(g, tx, ty) for g in grids])
for k in range(4):
    e.load(base)
    n0 = e.r32(WORLD + 4)
    n, r = run_blocking(e, asm([k, ('n', 0x36)]), 30)
    e.run(2)
    new = [o for o in world_objects(e)][n0:]
    print('spawn_map_object', k, 'frames', n, 'world', n0, '->', e.r32(WORLD + 4), [(hex(o), hex(e.r32(o)), pos(e, o)) for o in new])
e.load(base)
r = run_code(e, asm([41, 10, 10, ('n', 0x35), 41, 20, 20, ('n', 0x35), 41, ('n', 0x37), 42, ('n', 0x37)]))
print('count after 2 spawns of 41', r[2][-2:])
