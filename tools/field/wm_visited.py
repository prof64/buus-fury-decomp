"""wm_visited: dump the map-screen location table 0x0874CA24 (55 records) -> data/field/worldmap_visited_table.json.

Table: +0 u32 count, +4 -> record pointers. Record: +0x14 u32 n, +0x18.. n inline pointers to {u8 world, u8 area, u16 bit};
loading area (world, area) sets story flag 0x1C0 + bit of the FIRST record/entry that matches (0x08004A88)."""
import json
import sys
from wm_lib import *  # noqa
from common import u8, u16, u32, s16, s32, ROOT  # noqa

T = 0x0874CA24
STR = json.load(open(ROOT / 'engine/target/tmp/field-content/base/text/strings.json'))
STR = STR['strings'] if isinstance(STR, dict) else STR


def sname(i):
    if 0 <= i < len(STR):
        v = STR[i]
        return v if isinstance(v, str) else v.get('text')
    return None


n = u32(T)
lst = u32(T + 4)
recs = []
for k in range(n):
    r = u32(lst + 4 * k)
    words = [u32(r + 4 * i) for i in range(6)]
    cnt = u32(r + 0x14)
    areas = []
    for j in range(cnt):
        q = u32(r + 0x18 + 4 * j)
        areas.append({'world': u8(q), 'area': u8(q + 1), 'bit': u16(q + 2), 'flag': 0x1C0 + u16(q + 2)})
    recs.append({'index': k, 'rom': '0x%08X' % r, 'head_words': ['0x%08X' % x for x in words], 'areas': areas})
out = {'source': 'ROM 0x0874CA24 (map-screen locations, visited bits set by 0x08004A88 at LoadArea step 2)',
       'flag_rule': 'flag = 0x1C0 + bit (GameData +0x8D + (bit >> 3), bit & 7); the first record / entry matching (world, area) wins',
       'records': recs}
json.dump(out, open(ROOT / 'data/field/worldmap_visited_table.json', 'w'), indent=1)
for r in recs[:6]:
    print(r)
print(sum(len(r['areas']) for r in recs), 'area entries')
