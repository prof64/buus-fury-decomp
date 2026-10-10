"""objs.py KEY [KEY...] [--all] -- the objects of areas 'w-a' from the extracted content (enemies only with --all:
without it they are counted per type)"""
import glob
import json
import sys
from collections import Counter
from pathlib import Path

B = Path(__file__).resolve().parents[3] / 'engine/target/tmp/wf-core-field-content/maps'
keys = [k for k in sys.argv[1:] if not k.startswith('--')]
full = '--all' in sys.argv
for f in sorted(glob.glob(str(B / 'area_*/objects.json'))):
    d = json.load(open(f))
    for ar in d['areas']:
        if ar['key'] in keys:
            print('==', ar['key'], 'variant', ar['variant'], ar['name'], f.split('/')[-2])
            en = Counter()
            for o in ar['objects']:
                if o['type'] == 'enemy' and not full:
                    en[(o['enemy_type'], json.dumps(o.get('on_defeat')), json.dumps(o.get('condition')))] += 1
                    continue
                print('  ', {k: v for k, v in o.items() if k not in ('rom', 'list', 'behaviour')})
            for k, v in en.items():
                print('   enemy', k, 'x', v)
            for t in d.get('map_triggers', []):
                print('   map', t['type'], t['rect'], t.get('to', {}).get('key'), t.get('push'), t.get('condition'))
