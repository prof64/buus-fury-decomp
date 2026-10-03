"""gate.py NAME [NAME...] -- copy chapter segments from work/ship/segs_full to data/segments with an
"expect": {"matched": <all frames>} gate (wf-import tests/segments.rs holds them to every frame)"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / 'work/ship/segs_full'
DST = ROOT / 'data/segments'
for name in sys.argv[1:]:
    text = (SRC / f'{name}.json').read_text()
    head, rest = text.split('\n', 1)
    assert head.endswith(',"frames":[')
    n = sum(1 for ln in rest.split('\n') if ln.startswith('{'))
    h = json.loads(head[:-len(',"frames":[')] + '}')
    h['expect'] = {'matched': n}
    h['source']['run'] = 'work/ship/devfull (re/buu/ship/run_route.py from work/ship/dev_8_1.ss: re/buu/ship/mkstate_dev.py)'
    out = json.dumps(h, separators=(',', ':'))[:-1] + ',"frames":[\n' + rest
    (DST / f'{name}.json').write_text(out)
    print(name, n, 'frames')
