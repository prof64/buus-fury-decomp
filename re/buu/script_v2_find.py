"""script_v2_find.py HEX... -- catalog scripts that call any of the given natives."""
import json
import sys
from common import *

want = {int(x, 16) for x in sys.argv[1:]}
c = json.loads((ROOT / 'engine' / 'annotations' / 'script_catalog.json').read_text())
for s in c['scripts']:
    nat = set(s.get('natives', []))
    hit_nodes = []
    for n in s.get('nodes', []):
        if set(n.get('natives', [])) & want:
            hit_nodes.append(n['index'])
        nat |= set(n.get('natives', []))
    if nat & want:
        print(s['id'], s.get('purpose'), 'nodes', hit_nodes, s.get('header'), s.get('summary', '')[:80],
              [r['kind'] for r in s.get('referenced_by', [])][:3])
