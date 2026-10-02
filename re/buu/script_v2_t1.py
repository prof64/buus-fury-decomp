"""Round-2 tests, batch 1: variables / immediate natives checked by RAM effect. Results merged into
work/buu/script/v2_results.json ({native: {status, evidence}})."""
import json
import sys
from script_v2_lib import *

RES = OUT / 'v2_results.json'
res = json.loads(RES.read_text()) if RES.exists() else {}
S = 'buu_snakeway_start.ss'


def rec(n, ok, ev):
    res[f'0x{n:02x}'] = {'status': 'VERIFIED' if ok else 'INFERRED', 'evidence': ev}
    print(f'0x{n:02x}', 'OK ' if ok else 'NO ', ev)


def t11():
    def pre(e):
        e.w8(0x03002BB0, 5)
    e, r = run(S, [('n', 0x11), 7, ('s', 0x11)], 5, pre=pre)
    rec(0x11, r[2] == [5] and e.r8(0x03002BB0) == 7,
        f'byte 0x03002BB0 poked 5 -> native pushed {r[2]}; setter 0x11(7) -> byte = {e.r8(0x03002BB0)}')


def t14():
    am = e0 = None
    e, r = run(S, [('n', 0x12), ('n', 0x13), ('n', 0x14)], 5)
    a = e.r32(0x08054FBC + 0) if False else None
    # area manager = [0x08054FBC + 0xC] -> 0x0300197C
    w, m, v = e.r8(0x0300197C), e.r8(0x0300197D), e.r8(0x0300197E)

    def pre(e):
        e.w8(0x0300197E, 3)
    e2, r2 = run(S, [('n', 0x14)], 5, pre=pre)
    rec(0x14, r[2] == [w, m, v] and r2[2] == [3],
        f'get_world/get_map/get_map_variant pushed {r[2]} = area manager bytes 0x0300197C..E {[w, m, v]}; '
        f'byte +2 poked 3 -> pushed {r2[2]}')


def t18():
    def pre(e):
        e.w32(0x03002A60, 2)
    e, r = run(S, [('n', 0x18)], 5, pre=pre)
    rec(0x18, r[2] == [2], f'word 0x03002A60 poked 2 -> pushed {r[2]} (choice-menu result; see trace of event 34)')


def tca():
    def pre(e):
        for i in range(9):
            rec_ = 0x03001560 + 0x68 * i
            e.w16(rec_ + 0x16, 3)
            e.w16(rec_ + 0x1A, 1)
    e, r = run(S, [('n', 0xca)], 5, pre=pre)
    hp = [(e.r16(0x03001560 + 0x68 * i + 0x16), e.r16(0x03001560 + 0x68 * i + 0x18),
           e.r16(0x03001560 + 0x68 * i + 0x1A), e.r16(0x03001560 + 0x68 * i + 0x1C)) for i in range(9)]
    ok = all(h == mh and ep == mep for h, mh, ep, mep in hp)
    rec(0xca, ok, f'all 9 records HP=3, EP=1 poked; after heal_party (hp,max,ep,max) = {hp[:3]}...')


def tc4c5():
    e, r = run(S, [('n', 0xc4)], 5)
    a = e.r8(GD + 1)
    e2, r2 = run(S, [('n', 0xc4), ('n', 0xc5)], 5)
    b = e2.r8(GD + 1)
    rec(0xc4, a & 0x20 != 0, f'GameData byte 1 after set_game_bit_20 = {a:#04x}')
    rec(0xc5, b & 0x20 == 0, f'after set+clear = {b:#04x}')


def tdb():
    ch, r1, r0, e1, e0 = diff(S, [1, ('n', 0xdb)], [1] + pops(1), 10)
    rec(0xdb, len(ch) >= 1, f'diff vs baseline: {fmt(ch)}')


def tea():
    # Goku record skill list rec+0x2E (id, level) pairs
    r0 = 0x03001560
    sk = [(e_ := None)]
    e, r = run(S, [0, 0, ('n', 0xea)], 5)
    pairs = [(e.r8(r0 + 0x2E + 2 * k), e.r8(r0 + 0x2F + 2 * k)) for k in range(6)]
    sid, lv = pairs[0]
    e2, r2 = run(S, [0, sid, ('n', 0xea), 0, sid, 3, ('n', 0xe9), 0, sid, ('n', 0xea)], 5)
    rec(0xea, r2[2] is not None and r2[2][0] == lv and r2[2][1] == 3,
        f'Goku skills (id,lvl) {pairs}; get_skill_level(0,{sid}) = {r2[2][0] if r2[2] else None} (expected {lv}); '
        f'after set_skill_level(0,{sid},3) -> {r2[2][1] if r2[2] else None}')


def tf0():
    e, r = run(S, [0, 7, ('n', 0xf0)], 5)
    p = player(e)
    rec(0xf0, e.r32(p + 0x1A8) == 7 << 10, f'player +0x1A8 = {e.r32(p + 0x1A8):#x} after set_actor_field_1a8(0,7)')


def t54():
    e, r = run(S, [0, 3, ('n', 0x54)], 10)
    p = player(e)
    rec(0x54, r[0] is not None and (e.r32(p + 0x180) == 3 << 10 or e.r32(p + 0x1AC) == 3 << 10),
        f'player +0x180 = {e.r32(p + 0x180):#x}, +0x1AC = {e.r32(p + 0x1AC):#x} after set_actor_speed(0,3); '
        f'blocked {r[0]} frames')


def t56():
    e, r = run(S, [0, 0x21, ('n', 0x56)], 10)
    p = player(e)
    rec(0x56, e.r8(p + 8) == 0x21, f'player byte +8 = {e.r8(p + 8):#x} after set_actor_byte8(0,0x21); blocked {r[0]} f')


def tf3():
    r0 = 0x03001560
    e, r = run(S, [0, 10, ('n', 0xf3)], 5)
    # level byte: find by comparing with baseline
    e0, _ = run(S, [0, 10] + pops(2), 5)
    ch = changed(e0.iwram(), e.iwram())
    rec_ch = [(0x03000000 + i, ) for i in range(0)]
    lvl = [c for c in changed(ram(e0), ram(e), SKIP) if r0 <= c[0] < r0 + 0x68]
    rec(0xf3, len(lvl) > 0, f'level_up_to(0,10): Goku record words changed {fmt(lvl)}')


def tf1():
    e0, _ = run(S, [0, 1] + pops(2), 5)
    e, r = run(S, [0, 1, ('n', 0xf1)], 5)
    ch = changed(ram(e0), ram(e), SKIP)
    gd = [c for c in ch if 0x03001068 <= c[0] < 0x03001700]
    rec(0xf1, len(gd) > 0, f'unequip_slot(0,1): GameData/record changes {fmt(gd)}')


def tcbcc():
    e, r = run(S, [0, 32, 32, ('n', 0xcb), 0, 32, 32, ('n', 0xcc)], 5)
    p = player(e)
    x, y = e.r32(p + 0x1A0) >> 8, e.r32(p + 0x1A4) >> 8
    rec(0xcb, r[2] is not None and len(r[2]) == 2, f'player at ({x},{y}); screen_rect_x/y_at_actor(0,32,32) = {r[2]}')
    rec(0xcc, r[2] is not None and len(r[2]) == 2, 'see 0xCB')


def t22():
    # play sfx 1 then stop all; compare with play only (sound driver state, no audio output)
    ch, r1, r0, e1, e0 = diff(S, [1, ('n', 0x1f), ('n', 0x22)], [1, ('n', 0x1f)], 4)
    rec(0x22, len(ch) > 0, f'play_sfx(1)+stop_all_sfx vs play_sfx(1): {fmt(ch)}')


def tc9():
    ch, *_ = diff(S, [('n', 0xc9)], [], 10)
    rec(0xc9, False, f'broadcast_1f in Snake Way: RAM diff vs no-op = {len(ch)} words {fmt(ch)} (no receiver present)')


def t34():
    # follow the player, release, then move the player: camera must not follow
    items = [0, ('n', 0x33), ('n', 0x34), 0, 40, 60, ('n', 0x5d), 10, ('n', 0x88)]
    base = [0, ('n', 0x33), 0, 40, 60, ('n', 0x5d), 10, ('n', 0x88)]
    e1, r1 = run(S, items, 30)
    e0, r0 = run(S, base, 30)
    c1 = (e1.r32(0x030019A4), e1.r32(0x030019A8))
    c0 = (e0.r32(0x030019A4), e0.r32(0x030019A8))
    rec(0x34, c1 != c0, f'camera (0x030019A4/A8) after moving the player: with release {c1}, without {c0}')
    shot(e1, 'cam_release')


TESTS = [t11, t14, t18, tca, tc4c5, tdb, tea, tf0, t54, t56, tf3, tf1, tcbcc, t22, tc9, t34]
if __name__ == '__main__':
    want = sys.argv[1:]
    for t in TESTS:
        if want and t.__name__ not in want:
            continue
        try:
            t()
        except Exception as ex:  # noqa
            print(t.__name__, 'ERROR', ex)
        RES.write_text(json.dumps(res, indent=1))
