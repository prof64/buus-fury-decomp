"""Round-2 tests, batch 1c: level_up_to above the current level (60 in Snake Way), unequip a non-default item."""
from script_v2_t1 import *  # noqa: F401,F403
import script_v2_t1 as T


def tf3():
    r0 = 0x03001560
    e0, b0 = run(S, [0, 80] + pops(2) + [('n', 0xc8), ('n', 0xc3)], 5)
    e, r = run(S, [0, 80, ('n', 0xf3), ('n', 0xc8), ('n', 0xc3)], 5)
    a, b = e0.read(r0, 0x68), e.read(r0, 0x68)
    d = [(hex(i), a[i], b[i]) for i in range(0x68) if a[i] != b[i]]
    rec(0xf3, r[2][0] == 80 and b0[2][0] == 60,
        f'level/xp before {b0[2]}, after level_up_to(0,80) {r[2]} (get_level/get_xp); record bytes changed {d[:12]}')


def tf1():
    r0 = 0x03001560
    slot, item = 2, 78

    def pre(e):
        e.w32(r0 + 0x3C + 4 * slot, item)
        e.w32(GD + 0xE4, 1)
        e.w32(GD + 0xE8, (1 << 28) | item)
    e, r = run(S, [0, slot, ('n', 0xf1)], 5, pre=pre)
    rec(0xf1, e.r32(r0 + 0x3C + 4 * slot) == u32(0x080555B8 + 4 * slot) and e.r32(GD + 0xE8) == item,
        f'Goku slot {slot} = equipment {item}, inventory entry {(1 << 28) | item:#x} (owner Goku): after unequip_slot(0,{slot}) '
        f'slot = {e.r32(r0 + 0x3C + 4 * slot):#x} (default {u32(0x080555B8 + 4 * slot):#x}), entry = {e.r32(GD + 0xE8):#x} (owner nibble cleared)')


if __name__ == '__main__':
    for t in [tf3, tf1]:
        try:
            t()
        except Exception as ex:  # noqa
            print(t.__name__, 'ERROR', ex)
        T.RES.write_text(json.dumps(T.res, indent=1))
