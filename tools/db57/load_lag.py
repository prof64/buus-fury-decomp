"""load_lag.py OUT.json REC.json...  -- the GBA's load length per destination area, measured on real recordings:
the video frames of an area change during which the logic counter does not advance (the app runs a load in one
step). Writes {"default": median, "areas": {"W-A": median}} for wf-app's "original pacing" presentation mode."""
import json
import statistics
import sys
from collections import defaultdict

out, recs = sys.argv[1], sys.argv[2:]
per = defaultdict(list)
for r in recs:
    rows = json.load(open(r))['frames']
    for k in range(1, len(rows)):
        if rows[k]['area'] != rows[k - 1]['area']:
            lf = rows[k]['lf']
            j = k
            while j < len(rows) and rows[j]['lf'] == lf:
                j += 1
            n = j - k            # rows of the new area before its first logic frame advances
            if 0 < n < 400:
                a = rows[k]['area']
                per[f'{a[0]}-{a[1]}'].append(n)
allv = [v for vs in per.values() for v in vs]
doc = {'source': recs, 'default': int(statistics.median(allv)) if allv else 30,
       'areas': {k: int(statistics.median(v)) for k, v in sorted(per.items())}}
json.dump(doc, open(out, 'w'), indent=0)
print(len(allv), 'loads, default', doc['default'], 'areas', len(doc['areas']))
