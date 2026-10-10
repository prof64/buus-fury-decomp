"""script_trace_index.py -- list the traces in work/buu/script/traces with event counts and scenes/scripts seen."""
import json
from common import *

for p in sorted((WORK / 'script' / 'traces').glob('*.json')):
    d = json.loads(p.read_text())
    ev = d['events']
    scenes = sorted({e['scene'] for e in ev if e['ev'] in ('scene', 'step') and e.get('scene') is not None})
    natives = sorted({e['id'] for e in ev if e['ev'] == 'native'})
    blocking = sorted({e['id'] for e in ev if e['ev'] == 'native' and e.get('frame_after', e['frame']) - e['frame'] > 1})
    print(f"{p.name}: {len(ev)} events {d['summary']['counts']}")
    print(f"   scenes {scenes} scripts {sorted(set(x for x in d['summary']['scripts_run'] if x is not None))}")
    print(f"   natives {[hex(n) for n in natives]}")
    print(f"   blocking(>1f) {[hex(n) for n in blocking]}")
    print(f"   how: {d['meta'].get('how', d['meta'])}")
