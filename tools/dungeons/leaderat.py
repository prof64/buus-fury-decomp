import json, glob
for p in sorted(glob.glob('/Users/prof/source/buus-fury-decomp/work/db/lp/segs/*.json')):
    L = open(p).read().split('\n')
    fr = [json.loads(x.rstrip(',')) for x in L[1:] if x.startswith('{')]
    first = next((i for i, f in enumerate(fr) if f.get('p')), None)
    print(p.split('/')[-1], 'frames', len(fr), 'leader at', first, fr[first]['p'] if first is not None else '', 'depth', [fr[i]['depth'] for i in (0, first or 0)], 'area', fr[0]['area'])
