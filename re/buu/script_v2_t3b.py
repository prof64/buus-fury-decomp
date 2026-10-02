"""Round-2 tests, batch 3b: coin pickup outside the script; trigger objects identified by their event header."""
import json
from script_v2_lib import *
import script_v2_t1 as T
from script_v2_t3 import registry, REG

rec = T.rec
S = 'buu_snakeway_start.ss'
Y = 'buu_v2_yemma.ss'
NAT = lambda n: ('n', n)  # noqa: E731


def t87():
    e1, r1 = run(S, [150, 150, 1, NAT(0x87)], 60)
    e0, r0 = run(S, [150, 150, 1] + pops(3), 60)
    o1, o0 = e1.oam(), e0.oam()
    z1, z0 = e1.r32(0x0300155C), e0.r32(0x0300155C)
    shot(e1, 'coin60')
    rec(0x87, z1 > z0 or o1 != o0,
        f'drop_zenie_at(150,150,1) next to Goku (144,152); after the script ends and 60 frames of normal play: '
        f'zenie {z0} -> {z1}; OAM differs: {o1 != o0}')


def hdrs(e, objs, words):
    out = []
    for o in objs:
        w = [e.r32(o + 4 * k) for k in range(24)]
        out.append([hex(x) for x in words if x in w])
    return out


def t8a_8b():
    d0, d1 = 0x0807509C, 0x080750B0
    h0, h1 = u32(d0 + 0x10), u32(d1 + 0x10)
    e, _ = run(Y, [], 5)
    n0, o0 = registry(e)
    e1, _ = run(Y, [1, NAT(0x8a)], 10)
    n1, o1 = registry(e1)
    rec(0x8a, n1 == n0 + 1 and hdrs(e1, o1[-1:], [h0, h1]) == [[hex(h1)]],
        f'Yemma office: trigger registry ({REG:#x}) {n0} -> {n1} objects after spawn_trigger_force(1); event headers held: '
        f'before {hdrs(e, o0, [h0, h1])}, new object {hdrs(e1, o1[-1:], [h0, h1])} = list0[1] (condition flag 0x7D set, unmet)')
    e2, _ = run(Y, [0x7d, NAT(0x1d), NAT(0x8b)], 30)
    n2, o2 = registry(e2)
    rec(0x8b, hdrs(e, o0, [h0, h1]) != hdrs(e2, o2, [h0, h1]),
        f'Yemma office trigger headers before {hdrs(e, o0, [h0, h1])}; set_flag(0x7D); refresh_map_objects() -> '
        f'{hdrs(e2, o2, [h0, h1])} (registry cleared, list0 re-created by condition)')


if __name__ == '__main__':
    for t in (t87, t8a_8b):
        t()
        T.RES.write_text(json.dumps(T.res, indent=1))
