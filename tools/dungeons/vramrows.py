"""vramrows.py SEG.json F0 F1: the game's used sprite blocks (start, size) per frame of a recording with `vram` rows,
printed where they change (blocks added / removed)"""
import json
import sys

p, f0, f1 = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
L = open(p).read().split('\n')
fr = [json.loads(x.rstrip(',')) for x in L[1:] if x.startswith('{')]
prev = None
cur = None
for i, f in enumerate(fr[:f1 + 1]):
    if f.get('vram') is not None:
        cur = f['vram']
    if i < f0 or cur is None:
        continue
    used = sorted((e[1], e[2]) for e in cur[2:] if isinstance(e, list) and len(e) >= 3 and e[0] == 1)
    if prev is not None and used != prev:
        add = [u for u in used if u not in prev]
        gone = [u for u in prev if u not in used]
        print(i, 'lf', f['lf'], 'area', f['area'], 'depth', f['depth'], '+', add, '-', gone)
    prev = used
