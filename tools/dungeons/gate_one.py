"""gate_one.py SEG.json NAME [N]: a gate in data/segments from one segment file - its first N frames (default all),
expecting all of them (the segment must be exact over that window)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import segment as sg  # noqa: E402

p, name = Path(sys.argv[1]), sys.argv[2]
doc = json.loads(p.read_text())
n = min(int(sys.argv[3]), len(doc['frames'])) if len(sys.argv) > 3 else len(doc['frames'])
full = len(doc['frames'])
doc['frames'] = doc['frames'][:n]
doc['name'] = name
doc['expect'] = {'matched': n}
doc['full'] = {'frames': full, 'matched': full}
old = sg.OUT
sg.OUT = ROOT / 'data' / 'segments'
print(sg.save(name, doc), n)
sg.OUT = old
