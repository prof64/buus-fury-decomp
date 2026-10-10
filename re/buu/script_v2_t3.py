"""Round-2 tests, batch 3: trigger list (0x8A/0x8B), breakables (0xAD), coins, attachment offset, stopwatch."""
import json
import sys
from script_v2_lib import *
import script_v2_t1 as T

rec = T.rec
S = 'buu_snakeway_start.ss'
Y = 'buu_v2_yemma.ss'
NAT = lambda n: ('n', n)  # noqa: E731
REG = u32(0x080716A0 + 0x10)   # pointer to the trigger registry {count, objs...} (IWRAM/EWRAM)


def registry(e):
    r = e.r32(REG) if not (0x02000000 <= REG < 0x04000000) else REG
    r = REG
    n = e.r32(r)
    objs = [e.r32(r + 4 + 4 * k) for k in range(min(n, 32))]
    # which area def each trigger object came from: search the object for a pointer to a list0 def
    return n, objs


def defs_in(e, objs, defs):
    out = []
    for o in objs:
        words = [e.r32(o + 4 * k) for k in range(16)]
        out.append([hex(d) for d in defs if d in words])
    return out


def t8a_8b():
    d0, d1 = 0x0807509C, 0x080750B0   # Yemma list0: [0] cond flag 0x7D clear, [1] cond flag 0x7D set
    e, _ = run(Y, [], 5)
    n0, o0 = registry(e)
    e1, r1 = run(Y, [1, NAT(0x8a)], 10)
    n1, o1 = registry(e1)
    rec(0x8a, n1 == n0 + 1 and any(defs_in(e1, o1[-1:], [d1])),
        f'Yemma office trigger registry ({REG:#x}) count {n0} -> {n1} after spawn_trigger_force(1); new object refers '
        f'to list0[1] def {d1:#x} (condition flag 0x7D set, not met) -> {defs_in(e1, o1[-1:], [d0, d1])}')
    e2, r2 = run(Y, [0x7d, NAT(0x1d), NAT(0x8b)], 30)
    n2, o2 = registry(e2)
    rec(0x8b, defs_in(e, o0, [d0, d1]) != defs_in(e2, o2, [d0, d1]),
        f'Yemma office triggers by def: before {defs_in(e, o0, [d0, d1])}; set_flag(0x7D); refresh_map_objects() -> '
        f'{defs_in(e2, o2, [d0, d1])} (registry cleared and re-filled from list0 by condition)')


def tad():
    # area 44 = world 1 map 16 has one breakable with condition "flag 0x0B clear"
    def go(flag_before, clear_then_ad):
        items = []
        if flag_before:
            items += [0x0b, NAT(0x1d)]
        items += [1, 16, 0, 120, 120, NAT(0x77)]
        if clear_then_ad:
            items += [0x0b, NAT(0x1e), NAT(0xad)]
        else:
            items += [0x0b, NAT(0x1e)]
        return run(S, items, 120)
    e1, r1 = go(True, True)
    e0, r0 = go(True, False)
    ch = changed(ram(e0), ram(e1), SKIP)
    shot(e1, 'respawn_breakables')
    rec(0xad, len(ch) > 0,
        f'area (1,16): breakable with condition "flag 0x0B clear"; entered with the flag set, then clear_flag(0x0B): '
        f'with respawn_conditional_map_objects {len(ch)} RAM words differ from without ({fmt(ch, 4)})')


def t87():
    e1, r1 = run(S, [154, 152, 1, NAT(0x87)], 4)
    e0, r0 = run(S, [154, 152, 1] + pops(3), 4)
    ch = changed(ram(e0), ram(e1), SKIP)
    shot(e1, 'coin')
    # walk the player over it
    e2, r2 = run(S, [154, 152, 1, NAT(0x87), 0, 170, 152, NAT(0x60), NAT(0x10)], 80)
    e3, r3 = run(S, [0, 170, 152, NAT(0x60), NAT(0x10)], 80)
    rec(0x87, len(ch) > 0 and r2[2] is not None and r3[2] is not None and r2[2][0] > r3[2][0],
        f'drop_zenie_at(154,152,1): coin object created ({len(ch)} RAM words, v2_coin.png); Goku walks over it: '
        f'zenie {r2[2]} vs {r3[2]} without the coin')


def t55():
    def dump(e):
        p = player(e)
        return e.read(p, 0x200)
    e1, r1 = run(S, [0, 4, 2, 5, -8, NAT(0x55)], 10)
    e0, r0 = run(S, [0, 4, 2, 0, 0, NAT(0x55)], 10)
    a, b = dump(e1), dump(e0)
    d = [(hex(i), a[i], b[i]) for i in range(0x200) if a[i] != b[i]]
    shot(e1, 'attachment')
    rec(0x55, len(d) > 0,
        f'set_attachment(0,4,2,5,-8) vs (0,4,2,0,0): player object bytes differ at {d[:8]}; +0x5C = record '
        f'0x086B759C (kind 4 variant 2); (0,0,0,0,0) detaches (+0x5C = 0)')


def tc0():
    # stopwatch: show + reset + start, wait, stop, wait; read the time words directly
    e1, r1 = run(S, [NAT(0xbc), NAT(0xbe), NAT(0xbf), 30, NAT(0x88), NAT(0xc0), 30, NAT(0x88), NAT(0xc1)], 100)
    e0, r0 = run(S, [NAT(0xbc), NAT(0xbe), NAT(0xbf), 30, NAT(0x88), 30, NAT(0x88), NAT(0xc1)], 100)
    ch = changed(ram(e0), ram(e1), SKIP)
    rec(0xc0, (e1.r8(GD + 1) & 0x10) == 0 and (e0.r8(GD + 1) & 0x10) != 0,
        f'stopwatch running: GameData byte 1 = {e0.r8(GD + 1):#x}; after stop_stopwatch() = {e1.r8(GD + 1):#x}; '
        f'get_stopwatch {r1[2]} vs {r0[2]}; RAM diff {fmt(ch, 4)}')


TESTS = [t8a_8b, tad, t87, t55, tc0]
if __name__ == '__main__':
    print('registry ptr', hex(REG))
    want = sys.argv[1:]
    for t in TESTS:
        if want and t.__name__ not in want:
            continue
        try:
            t()
        except Exception as ex:  # noqa
            import traceback
            traceback.print_exc(limit=3)
        T.RES.write_text(json.dumps(T.res, indent=1))
