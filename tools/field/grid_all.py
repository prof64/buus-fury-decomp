"""grid_all.py [OUT] [START] [STOP]: grid_probe.check over every area and map variant (headless); one JSON line per
(area, variant) appended to OUT (default engine/target/tmp/field/grid_all.jsonl)."""
import json
import sys
import traceback
import grid_probe as g

out = sys.argv[1] if len(sys.argv) > 1 else str(g.ROOT / 'engine/target/tmp/field/grid_all.jsonl')
start = int(sys.argv[2]) if len(sys.argv) > 2 else 0
stop = int(sys.argv[3]) if len(sys.argv) > 3 else 10 ** 9
areas = json.load(open(g.CONTENT / 'world' / 'areas.json'))['areas']
jobs = [(a['world'], a['area'], v + 1) for a in areas for v in range(len(a['maps']))]
for k, (w, a, v) in enumerate(jobs[start:stop], start):
    try:
        r = g.check(w, a, v)
        if r is None:
            r = {'area': f'{w}-{a}', 'variant': v, 'error': 'warp failed'}
        else:
            r['walk_n'] = len(r['walk'])
            r['shot_n'] = len(r['shot'])
            r['walk'] = r['walk'][:50]
            r['shot'] = r['shot'][:20]
    except Exception as ex:  # noqa
        r = {'area': f'{w}-{a}', 'variant': v, 'error': repr(ex)[:200]}
    r['job'] = k
    r['want_variant'] = v
    with open(out, 'a') as f:
        f.write(json.dumps(r) + '\n')
    print(k, r.get('area'), r.get('variant'), r.get('walk_n'), r.get('shot_n'), r.get('error', ''), flush=True)
