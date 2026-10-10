"""Round-2 tests, batch 4: return_to_title (long run), entity_call, map item pickups, sleep mode."""
import json
import sys
from script_v2_lib import *
import script_v2_t1 as T
from script_nat_b_lib import run_code

rec = T.rec
S = 'buu_snakeway_start.ss'
NAT = lambda n: ('n', n)  # noqa: E731


def t8e():
    e = emu(S)
    e.run(3)
    r = run_code(e, asm([NAT(0x8e)]), max_frames=10)
    modes = []
    for k in range(12):
        e.run(150)
        modes.append(hex(e.r32(e.r32(0x03000F90)) if e.r32(0x03000F90) else 0))
        if k in (2, 5, 11):
            shot(e, f'title_{(k + 1) * 150}')
    rec(0x8e, r[0] is not None, f'return_to_title(): script returned after {r[0]} f; current mode vtable every 150 f: '
        f'{modes} (v2_title_*.png)')


def t29():
    e = emu(S)
    e.run(3)
    o = e.r32(0x03001990)
    vt = e.r32(o)
    m = vt + 0xC
    fn = (m + e.r32(m)) & 0xffffffff if False else e.r32(m)
    T.res['0x29_vt'] = f'slot0 obj {o:#x} vt {vt:#x} [+0xC] {fn:#x}'
    print(T.res['0x29_vt'])
    e1, r1 = run(S, [0, 0, 0, NAT(0x29)], 20)
    e0, r0 = run(S, [0, 0, 0] + pops(3), 20)
    ch = changed(ram(e0), ram(e1), SKIP)
    rec(0x29, r1[0] is not None, f'entity_call(0,0,0) (slot 0 = {o:#x}, vtable {vt:#x}): returned after {r1[0]} f; '
        f'RAM diff {fmt(ch, 6)}')


def find_item_area():
    for i in range(0x1c4):
        a = 0x0808E2E0 + 0x38 * i
        if u8(a + 6):
            return i, u8(a), u8(a + 1), u8(a + 6)
    return None


def ta9_aa():
    i, w, m, n = find_item_area()
    T.res['item_area'] = f'area {i} = ({w},{m}) with {n} item pickups'
    print(T.res['item_area'])
    go = [w, m, 0, 120, 120, NAT(0x77)]
    e1, r1 = run(S, go + [0, 130, 130, NAT(0xa9)], 150)
    e0, r0 = run(S, go, 150)
    ch = changed(ram(e0), ram(e1), SKIP)
    shot(e1, 'map_item_at')
    rec(0xa9, len(ch) > 0, f'in area ({w},{m}) spawn_map_item_at(0,130,130): {len(ch)} RAM words vs none ({fmt(ch, 4)}) (v2_map_item_at.png)')
    e2, r2 = run(S, go + [0, NAT(0xaa)], 150)
    ch = changed(ram(e0), ram(e2), SKIP)
    shot(e2, 'map_item')
    rec(0xaa, len(ch) > 0, f'spawn_map_item(0): {len(ch)} RAM words vs none (v2_map_item.png)')


def td4():
    e = emu(S)
    e.run(3)
    # wake combo: try L+R+Select held after 30 frames
    keys = lambda f: 0 if f < 30 else (KEY.L | KEY.R | KEY.SELECT)  # noqa: E731
    r = run_code(e, asm([NAT(0xd4)]), max_frames=200, keys=keys)
    rec(0xd4, r[0] is not None, f'sleep_mode(): with L+R+Select held from frame 30 the call returned after {r[0]} f '
        f'(None = still asleep after 200 f)')


TESTS = [t8e, t29, ta9_aa, td4]
if __name__ == '__main__':
    want = sys.argv[1:]
    for t in TESTS:
        if want and t.__name__ not in want:
            continue
        try:
            t()
        except Exception:  # noqa
            import traceback
            traceback.print_exc(limit=3)
        T.RES.write_text(json.dumps(T.res, indent=1))
