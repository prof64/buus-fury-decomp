"""Compact area summary (content objects.json): python3 areas_sum.py W A0 A1"""
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CONTENT = ROOT / 'engine/target/tmp/wf-core-field-content'
AREAS = json.loads((CONTENT / 'world/areas.json').read_text())['areas']


def r(o):
    q = o.get('rect')
    return f"({q['x']},{q['y']} {q['w']}x{q['h']})" if q else str(o.get('pos'))


def cond(c):
    if not c or c.get('always'):
        return ''
    return '[' + ','.join(f'{k}={v}' for k, v in c.items() if k != 'note') + ']'


w, a0, a1 = map(int, sys.argv[1:4])
for a in AREAS:
    if a['world'] != w or not a0 <= a['area'] <= a1:
        continue
    for vi, m in enumerate(a['maps'], 1):
        o = json.loads((CONTENT / 'maps' / m / 'objects.json').read_text())
        print(f"== {a['key']} v{vi} {m} start {o.get('start_pos')} name {a.get('name')}")
        for t in o.get('map_triggers', []):
            to = t.get('to') or {}
            print('   mapexit', r(t), '->', to.get('key'), to.get('x'), to.get('y'), t.get('push'), cond(t.get('condition')))
        for ar in o.get('areas', []):
            if ar['key'] != a['key']:
                continue
            ens = Counter()
            for t in ar['objects']:
                ty = t['type']
                if ty == 'enemy':
                    ens[(t.get('sprite_index'), json.dumps(cond(t.get('condition'))))] += 1
                    continue
                if ty in ('breakable', 'map_sprite', 'region'):
                    continue
                extra = t.get('event') or t.get('script') or ''
                if ty == 'exit':
                    to = t.get('to') or {}
                    extra = f"-> {to.get('key')} {to.get('x')},{to.get('y')}"
                if t.get('talk'):
                    extra = t['talk'].get('event')
                ws = ' '.join(f"{x['to']}@{x['x']},{x['y']}" for x in t.get('warps', []))
                print('  ', ty, r(t), extra, cond(t.get('condition')), (t.get('activation') or {}).get('mode', ''), ws)
            for (sp, c), n in ens.items():
                print('   enemy x%d sprite %s %s' % (n, sp, json.loads(c)))
