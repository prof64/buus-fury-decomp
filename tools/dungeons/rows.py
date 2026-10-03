"""rows.py SEG.json F0 F1 [key ...]: print the recording's rows F0..F1 (given keys, default: others, snd, p, depth)."""
import json
import sys
from pathlib import Path

doc = json.loads(Path(sys.argv[1]).read_text())
rows = doc['frames']
f0, f1 = int(sys.argv[2]), int(sys.argv[3])
ks = sys.argv[4:] or ['others', 'snd', 'p', 'depth']
if 'objs' in ks:
    print('objs', json.dumps(doc.get('objs'))[:3000])
for f in range(f0, min(f1 + 1, len(rows))):
    print(f, {k: rows[f].get(k) for k in ks if k != 'objs'})
