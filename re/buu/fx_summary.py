"""fx_summary.py skill_json -- per-frame compact listing grouped by fx id (frame ranges of each record)."""
import sys
import json
from collections import defaultdict
L = json.load(open(sys.argv[1]))
seen = defaultdict(list)
raw = defaultdict(list)
for fr in L:
    for o in fr['objs']:
        if 'fx' in o:
            seen[(o['fx'], o['i'])].append((fr['f'], o['rx'], o['ry'], o['mode'], o['oam']))
        else:
            if o['gfx'] != '0x80725f0':
                raw[(o['gfx'], o['W'], o['H'])].append((fr['f'], o['sx'], o['sy']))
for k in sorted(seen, key=lambda k: seen[k][0][0]):
    v = seen[k]
    print(k[0], k[1], 'frames', v[0][0], '-', v[-1][0], 'n', len(v), 'first@', v[0][1:3], 'last@', v[-1][1:3], 'mode', v[0][3])
for k, v in raw.items():
    print('RAW', k, v[0][0], '-', v[-1][0], len(v), v[0][1:])
