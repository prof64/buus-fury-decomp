"""Which natives do the loot / item-use / NPC-talk scripts use (picking test cases for wf-script-core)."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
c = json.loads((ROOT / 'engine/annotations/script_catalog.json').read_text())
S = {s['id']: s for s in c['scripts']}


def nat(s):
    out = set()
    for n in s.get('nodes', []):
        out |= set(n.get('natives', []))
    out |= set(s.get('natives', []))
    return sorted(hex(x) for x in out)


for i in ['s497', 's496', 's499', 's500', 's043', 's002']:
    s = S.get(i)
    print(i, s and s.get('purpose'), s and nat(s))
items = [s for s in c['scripts'] if any('item' in r['kind'] for r in s.get('referenced_by', []))]
for s in items[:30]:
    print(s['id'], [r['where'] for r in s['referenced_by']][:1], nat(s))
