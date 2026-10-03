"""Record the HFIL route's legs as segment masters and cut them at the area entries.

Each leg of `hfil.py --route` (mark -> mark) has its start state work/side/h_<from>.ss and its key log
work/side/h_<mark>.keys.json; a leg is recorded from its own start state (segment.py record) into
work/side/seg/sc_hfil_<mark>.json and cut (tools/superbuu/mkcuts.py) into work/side/hseg2/<mark>/.

    .venv/bin/python tools/side/hfil_record.py [MARK ...]      (default: statues vegeta veg1 gate h13; --recut: cut the
    existing masters again)
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
D = ROOT / 'work' / 'side'
PY = str(ROOT / '.venv' / 'bin' / 'python')
marks = [a for a in sys.argv[1:] if not a.startswith('--')] or ['statues', 'vegeta', 'veg1', 'gate', 'h13']
for m in marks:
    leg = json.loads((D / f'h_{m}.keys.json').read_text())
    kf = D / f'k_leg_{m}.json'
    kf.write_text(json.dumps({'keys': leg['keys']}))
    env = dict(os.environ, SEGMENT_DIR=str(D / 'seg'))
    name = f'sc_hfil_{m}'
    r = None
    if '--recut' not in sys.argv:
      r = subprocess.run([PY, str(ROOT / 'tools/segments/segment.py'), 'record', '--state', str(D / leg['from']),
                        '--keys', str(kf), '--frames', str(len(leg['keys'])), '--name', name],
                       env=env, cwd=ROOT, capture_output=True, text=True)
    print(m, (r.stdout.strip().splitlines() or [r.stderr[-300:]])[-1] if r else 'recut')
    out = D / 'hseg2' / m
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob('*'):
        old.unlink()
    r = subprocess.run([PY, str(ROOT / 'tools/superbuu/mkcuts.py'), str(D / 'seg' / f'{name}.json'), str(out),
                        '--prefix', 'hf_' + m],
                       env=env, cwd=ROOT, capture_output=True, text=True)
    print(m, 'cuts', len(list(out.glob('*.json'))), r.stderr[-300:])
