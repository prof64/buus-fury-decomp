"""sndrows.py SEG.json [F0 F1] [ID]: the recording's frames with sounds (optionally only sound ID), with lf / hp."""
import json
import sys
from pathlib import Path

doc = json.loads(Path(sys.argv[1]).read_text())
rows = doc['frames']
f0 = int(sys.argv[2]) if len(sys.argv) > 2 else 0
f1 = int(sys.argv[3]) if len(sys.argv) > 3 else len(rows) - 1
sid = int(sys.argv[4]) if len(sys.argv) > 4 else None
for f in range(f0, min(f1 + 1, len(rows))):
    r = rows[f]
    s = [x for x in r.get('snd', []) if sid is None or (len(x) > 1 and x[1] == sid)]
    if s:
        print(f, 'lf', r.get('lf'), 'hp', r.get('hp'), 'depth', r.get('depth'), s)
