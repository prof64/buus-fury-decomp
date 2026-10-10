"""Re-measure close-range / special skills at a short distance (enemy 20px below)."""
import json
from common import *
from measure_skills import measure, SK
from strtab import S
out = []
for sk, lvls in ((8, (0, 1, 2)), (9, (0,)), (13, (0,)), (5, (0,)), (17, (2,)), (12, (0,))):
    for lv in lvls:
        for pat in ('tap', 'hold60', 'hold150', 'mash'):
            for dist in (20, 40):
                r = measure(sk, lv, pat, frames=360, dist=dist)
                r['name'] = S(u16(SK + 24 * sk + 12))
                r['dist'] = dist
                out.append(r)
                print(sk, r['name'], 'lv', lv, pat, 'dist', dist, 'hits', r['hits'], 'first', r['first_hit'], 'mults', r['mults'],
                      'subs', r['subs'], 'dmg', r['total_dmg'], 'EP', r['ep_total'], 'ep_ev', r['ep_events'][:6])
(WORK / 'skills_measured2.json').write_text(json.dumps(out, indent=1))
