"""Scan every area's actor lists for enemy spawn records (ctor 0x0800FD55).

Area table 0x0808E2E0 (0x1C4 entries x 0x38 bytes). Entry: +0 u8 world, +1 u8 map, +4..+7 u8 counts
of the lists at +0x10, +0x14, +0x18, +0x1C, +0x20 (count bytes: b3=+3 for +0x10, +4 for +0x14,
+5 for +0x18, +6 for +0x1c, +7 for +0x20), +0xC u32 area-name string id.
Enemy spawn record (0x20 bytes): +0 ctor 0x0800FD55, +4 spawn condition (ptr to bytecode or inline),
+8 u32 enemy type (index into 0x08711274), +0xC s16 x, +0xE s16 y, +0x10 u32 graphics id
(-> 0x086B6BB4 table), +0x14 ptr (per-spawn data), +0x18 u32, +0x1C ptr.
"""
from collections import defaultdict
from common import *
from strtab import S

AREAS = 0x0808E2E0
NAREA = 0x1C4
ENEMY_CTOR = 0x0800FD55


def area_lists(a):
    counts = [u8(a + 3), u8(a + 4), u8(a + 5), u8(a + 6), u8(a + 7)]
    ptrs = [u32(a + 0x10), u32(a + 0x14), u32(a + 0x18), u32(a + 0x1c), u32(a + 0x20)]
    return list(zip(counts, ptrs))


def scan():
    out = []
    for i in range(NAREA):
        a = AREAS + 0x38 * i
        world, mp = u8(a), u8(a + 1)
        name = S(u32(a + 0xc)) if u32(a + 0xc) else ''
        for li, (cnt, lp) in enumerate(area_lists(a)):
            if not (0x08000000 <= lp < 0x08800000) or cnt == 0 or cnt > 100:
                continue
            for k in range(cnt):
                rec = u32(lp + 4 * k)
                if not (0x08000000 <= rec < 0x08800000):
                    continue
                if u32(rec) != ENEMY_CTOR:
                    continue
                typ = u32(rec + 8)
                if typ > 0x95:
                    continue
                cond = u32(rec + 4)
                out.append(dict(area_index=i, world=world, map=mp, area=name, list=li, record=hex(rec), type=typ,
                                x=s16(rec + 0xc), y=s16(rec + 0xe), gfx=u32(rec + 0x10),
                                cond=hex(cond), extra=hex(u32(rec + 0x14))))
    return out


if __name__ == '__main__':
    sp = scan()
    print(len(sp), 'enemy spawns')
    by = defaultdict(set)
    for s in sp:
        by[s['type']].add((s['area'], s['world'], s['map']))
    for t in sorted(by):
        print(hex(t), sorted(by[t])[:6])
