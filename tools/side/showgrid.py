"""showgrid.py W A [V]  -- print an area's walk grid ('.' walkable) with its exits (letters) and objects (digits)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from grid import Grid, map_of  # noqa: E402

w, a = int(sys.argv[1]), int(sys.argv[2])
v = int(sys.argv[3]) if len(sys.argv) > 3 else 1
m = map_of(w, a, v)
g = Grid(m)
rows = [['.' if g.walk[ty * g.w + tx] else '#' for tx in range(g.w)] for ty in range(g.h)]
o = json.loads((ROOT / 'work' / 'field_content' / 'maps' / m / 'objects.json').read_text())
leg = []
def mark(r, ch):
    for ty in range(max(0, r['y'] // 8), min(g.h, (r['y'] + r['h'] + 7) // 8)):
        for tx in range(max(0, r['x'] // 8), min(g.w, (r['x'] + r['w'] + 7) // 8)):
            rows[ty][tx] = ch
for i, t in enumerate(o['map_triggers']):
    ch = chr(ord('A') + i)
    mark(t['rect'], ch)
    leg.append(f"{ch} {t['type']} -> {t.get('to', {}).get('key')} push {t.get('push')}")
n = 0
for ar in o['areas']:
    for t in ar['objects']:
        ch = str(n % 10)
        if t.get('rect'):
            mark(t['rect'], ch)
        elif t.get('pos'):
            mark({'x': t['pos'][0], 'y': t['pos'][1], 'w': 8, 'h': 8}, ch)
        leg.append(f"{ch} {t['type']} {t.get('enemy_type', '')} {t.get('script') or t.get('event') or ''} {t.get('condition')}")
        n += 1
for ty, r in enumerate(rows):
    if '.' in r or any(c.isalnum() for c in r):
        print('%3d %s' % (ty * 8, ''.join(r).rstrip('#')))
print('\n'.join(leg))
