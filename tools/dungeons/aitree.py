"""aitree.py HEX...: print the extracted AI state trees (content ai/state_XXX.json) as an indented list"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
D = ROOT / 'engine' / 'target' / 'tmp' / 'wf-import-segment-content' / 'ai'


def walk(n, ind=0):
    if isinstance(n, dict):
        k = n.get('node') or n.get('kind') or n.get('name') or n.get('type')
        p = {x: y for x, y in n.items() if x not in ('children', 'kids', 'node', 'kind', 'name', 'type')}
        print('  ' * ind + str(k), json.dumps(p)[:170])
        for c in n.get('children', []) or n.get('kids', []):
            walk(c, ind + 1)
    elif isinstance(n, list):
        for c in n:
            walk(c, ind)


for h in sys.argv[1:]:
    d = json.loads((D / f'state_{h}.json').read_text())
    print('== state', h, [k for k in d.keys()])
    walk(d.get('tree', d.get('root', d)))
