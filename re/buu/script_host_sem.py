"""Print the decoded semantics of selected natives (wf-script-core adapter work)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
t = json.loads((ROOT / 'engine/annotations/script_natives.json').read_text())
ids = [int(x, 16) for x in sys.argv[1:]]
for e in t:
    if e['id'] in ids:
        print(hex(e['id']), e['name'], e.get('args'), '|', e.get('semantics'), '\n')
