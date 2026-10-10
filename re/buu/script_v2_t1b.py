"""Round-2 tests, batch 1b: corrected versions of the batch-1 tests that needed the right RAM layout."""
from script_v2_t1 import *  # noqa: F401,F403
import script_v2_t1 as T


def tea():
    e, r = run(S, [0, 7, ('n', 0xea), 0, 7, 3, ('n', 0xe9), 0, 7, ('n', 0xea)], 5)
    lv = list(e.read(0x03001560 + 0x2E, 2))
    rec(0xea, r[2] == [0, 3] and lv == [3, 3],
        f'Goku knows skills {list(e.read(0x03001560 + 0xD, 3))}; get_skill_level(0,7) = {r[2][0]}, after '
        f'set_skill_level(0,7,3) = {r[2][1]}; rec+0x2E/+0x2F = {lv}')


def tdb():
    def pre(e):
        e.w32(0x03001988, 0)
    e, r = run(S, [1, ('n', 0xdb)], 5, pre=pre)
    a = e.r32(0x03001988)
    e2, r2 = run(S, [1, ('n', 0xdb), 0, ('n', 0xdb)], 5, pre=pre)
    b = e2.r32(0x03001988)
    rec(0xdb, a == 1 and b == 0,
        f'area manager word 0x03001988 (=[0x08054FBC+0xC]+0xC): set_area_flag_c(1) -> {a}, then (0) -> {b}')


def tf3():
    r0 = 0x03001560
    e0, _ = run(S, [0, 30] + pops(2), 5)
    e, r = run(S, [0, 30, ('n', 0xf3)], 5)
    a, b = e0.read(r0, 0x68), e.read(r0, 0x68)
    d = [(hex(i), a[i], b[i]) for i in range(0x68) if a[i] != b[i]]
    rec(0xf3, len(d) > 0, f'level_up_to(0,30): Goku record bytes (offset, old, new) {d[:16]}')


def tf1():
    r0 = 0x03001560
    slot, item = 2, 77

    def pre(e):
        e.w32(r0 + 0x3C + 4 * slot, item)
        e.w32(GD + 0xE4, 1)
        e.w32(GD + 0xE8, (1 << 28) | item)
    e, r = run(S, [0, slot, ('n', 0xf1)], 5, pre=pre)
    rec(0xf1, e.r32(r0 + 0x3C + 4 * slot) == u32(0x080555B8 + 4 * slot) and e.r32(GD + 0xE8) == item,
        f'Goku slot {slot} = equipment {item} (inventory entry 0x1000004D): after unequip_slot(0,{slot}) slot = '
        f'{e.r32(r0 + 0x3C + 4 * slot):#x} (default {u32(0x080555B8 + 4 * slot):#x}), entry = {e.r32(GD + 0xE8):#x}')


def t22():
    ch, r1, r0, e1, e0 = diff(S, [1, ('n', 0x1f), ('n', 0x22)], [1, ('n', 0x1f)], 4,
                              skip=SKIP + [(0x02000000, 0x02001000)])
    rec(0x22, len(ch) > 0, f'play_sfx(1)+stop_all_sfx vs play_sfx(1) (mix buffer excluded): {fmt(ch)}')


if __name__ == '__main__':
    for t in [tea, tdb, tf3, tf1, t22]:
        try:
            t()
        except Exception as ex:  # noqa
            print(t.__name__, 'ERROR', ex)
        T.RES.write_text(json.dumps(T.res, indent=1))
