"""Charge-scaling probe: for a charge skill, vary hold time and record EP spent vs damage multiplier."""
import json
import sys
from common import *
from atk_measure import measure
sk = int(sys.argv[1], 0)
levels = [int(x) for x in sys.argv[2].split(',')] if len(sys.argv) > 2 else [0]
holds = [int(x) for x in sys.argv[3].split(',')] if len(sys.argv) > 3 else [30, 61, 90, 120, 180, 240, 300]
dist = int(sys.argv[4]) if len(sys.argv) > 4 else 60
res = []
for lv in levels:
    for h in holds:
        r = measure(sk, lv, h, dist, frames=h + 200)
        hits = r['hits']
        row = dict(level=lv, hold=h, press=r['cost_on_press'], ep=r['ep_total'], nhits=len(hits),
                   first=hits[0][0] if hits else None, mults=sorted(set(x[3] for x in hits)),
                   dmg=round(sum(x[4] for x in hits), 2), idle=r['idle_frame'])
        print(row, flush=True)
        res.append(row)
(WORK / f'atk_charge_{sk}.json').write_text(json.dumps(res))
