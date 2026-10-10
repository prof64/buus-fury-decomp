"""script_cat_areas.py -- census of map actor records (area table 0x0808E2E0, 5 lists) and the script
callbacks they reference. For each record: ctor, and every word at +0..+0x3C that points to a callback pair
{0x0800421F raw-code | 0x0800424F event} or directly to an event header. Prints a summary per ctor/offset."""
import json
import sys
from collections import defaultdict, Counter
from common import *

AREAS, NAREA = 0x0808E2E0, 0x1C4
SCRIPTS = 0x0879B704
HDR = {u32(SCRIPTS + 4 * i): i for i in range(522)}
RAW, EVT = 0x0800421F, 0x0800424F


def isrom(v):
    return 0x08000000 <= v < 0x08800000


def classify(v):
    if not isrom(v) or v & 3:
        return None
    w = u32(v)
    if w == RAW:
        return ('raw', u32(v + 4))
    if w == EVT:
        return ('event', HDR.get(u32(v + 4), hex(u32(v + 4))))
    if v in HDR:
        return ('event_hdr', HDR[v])
    return None


def records():
    for i in range(NAREA):
        a = AREAS + 0x38 * i
        for li in range(5):
            cnt = u8(a + 3 + li)
            lp = u32(a + 0x10 + 4 * li)
            if not isrom(lp) or cnt == 0 or cnt > 200:
                continue
            for k in range(cnt):
                rec = u32(lp + 4 * k)
                if isrom(rec):
                    yield i, li, k, rec


if __name__ == '__main__':
    per = defaultdict(Counter)
    ctor_n = Counter()
    rows = []
    for ai, li, k, rec in records():
        ctor = u32(rec)
        ctor_n[(li, ctor)] += 1
        for off in range(4, 0x40, 4):
            c = classify(u32(rec + off))
            if c:
                per[(li, ctor)][(off, c[0])] += 1
                rows.append(dict(area=ai, list=li, idx=k, rec=hex(rec), ctor=hex(ctor), off=off, kind=c[0],
                                 target=c[1] if isinstance(c[1], (int, str)) else c[1]))
    for key, n in sorted(ctor_n.items()):
        li, ctor = key
        print(f'list{li} ctor {ctor:08x} n={n} cb={dict(per[key])}')
    (WORK / 'script').mkdir(exist_ok=True)
    (WORK / 'script' / 'area_cb.json').write_text(json.dumps(rows, indent=0, default=str))
    print(len(rows), 'callback refs')
