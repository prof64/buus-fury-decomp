"""segrows.py SEGMENT.json [A B] [FIELDS]  -- print a segment's recorded rows A..B whenever the given fields change
(default fields: depth, snd, steps, io, hud, area, p mode) or a key is pressed"""
import json
import sys

L = open(sys.argv[1]).read().splitlines()
a = int(sys.argv[2]) if len(sys.argv) > 2 else 0
b = int(sys.argv[3]) if len(sys.argv) > 3 else 10 ** 9
fields = sys.argv[4].split(',') if len(sys.argv) > 4 else ['depth', 'snd', 'steps', 'io', 'hud', 'area']
rows = []
for ln in L[1:]:
    ln = ln.strip().rstrip(',')
    try:
        rows.append(json.loads(ln))
    except ValueError:
        pass
print(len(rows), 'rows; fields of row 0:', sorted(rows[0].keys()))
prev = None
for i, r in enumerate(rows[a:b], a):
    k = {x: r.get(x) for x in fields}
    if 'p' in r and r['p']:
        k['pm'] = r['p'][3]
    s = json.dumps(k)
    if s != prev or r.get('keys'):
        print(i, r.get('lf'), 'keys', r.get('keys'), s[:260])
        prev = s
