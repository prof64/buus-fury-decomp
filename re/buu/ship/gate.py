"""gate.py [--src DIR] [--run TEXT] NAME [NAME...] -- copy chapter segments (default work/ship/rsegs: the real route
from work/earth/earth_end.ss) to data/segments with an "expect": {"matched": <all frames>} gate (wf-import
tests/segments.rs holds them to every frame)"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
a = sys.argv[1:]
SRC = ROOT / (a[a.index('--src') + 1] if '--src' in a else 'work/ship/rsegs')
RUN = a[a.index('--run') + 1] if '--run' in a else \
    'work/ship/real4 (re/buu/ship/run_route.py work/earth/earth_end.ss real4 --real --log: the real route)'
DST = ROOT / 'data/segments'
names = [x for i, x in enumerate(a) if not x.startswith('--') and (i == 0 or a[i - 1] not in ('--src', '--run'))]
for name in names:
    text = (SRC / f'{name}.json').read_text()
    head, rest = text.split('\n', 1)
    assert head.endswith(',"frames":[')
    n = sum(1 for ln in rest.split('\n') if ln.startswith('{'))
    h = json.loads(head[:-len(',"frames":[')] + '}')
    h['expect'] = {'matched': n}
    h['source']['run'] = RUN
    out = json.dumps(h, separators=(',', ':'))[:-1] + ',"frames":[\n' + rest
    (DST / f'{name}.json').write_text(out)
    print(name, n, 'frames')
