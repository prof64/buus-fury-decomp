"""gate.py DUNGEON [--cap N] [--min N]: turn a dungeon's local cuts (work/db/<DUNGEON>/segs) into gates in data/segments.

Replays the cuts on wf-core (cargo test -p wf-import --test segments), then per segment: matched to its end -> the
whole recording is the gate (or its first CAP frames when it is longer: "expect": {"matched": CAP}); a divergence ->
the first matched frames (minus a 4-frame margin, at most CAP), if there are at least MIN of them. A gate file holds
only the gated frames (data stays small); the full cuts stay local in work/db. Prints the table for segments.md."""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401  (paths)
import segment as sg  # noqa: E402

name = sys.argv[1]
cap = int(sys.argv[sys.argv.index('--cap') + 1]) if '--cap' in sys.argv else 600
mn = int(sys.argv[sys.argv.index('--min') + 1]) if '--min' in sys.argv else 150
src = ROOT / 'work' / 'db' / name / 'segs'
env = dict(os.environ, SEGMENT_DIR=str(src), SEGMENT_STRICT='0')
out = subprocess.run(['cargo', 'test', '-p', 'wf-import', '--test', 'segments', '--', '--nocapture'], cwd=ROOT / 'engine',
                     env=env, capture_output=True, text=True)
res = {}
for ln in (out.stdout + out.stderr).split('\n'):
    m = re.match(r'segment (\S+): (\d+) video frames \((\d+) logic\), matched (\d+) / \d+ \| first divergence (.*?) \| import', ln)
    if m:
        res[m.group(1)] = (int(m.group(2)), int(m.group(4)), m.group(5))
old = sg.OUT
sg.OUT = ROOT / 'data' / 'segments'
tot = [0, 0, 0, 0]
for p in sorted(src.glob('*.json')):
    seg = p.stem
    if seg not in res:
        print(seg, 'NOT RUN')
        continue
    frames, matched, first = res[seg]
    exact = first == 'None'
    n = min(frames if exact else max(0, matched - 4), cap)
    tot[0] += frames
    tot[1] += matched if not exact else frames
    tot[2] += exact
    gate = sg.OUT / f'{seg}.json'
    if n < mn and not exact:
        if gate.exists():
            gate.unlink()
        print(f'| `{seg}` | {frames} | {matched} | - | {first[:90]} |')
        continue
    doc = json.loads(p.read_text())
    doc['frames'] = doc['frames'][:n]
    # a window cut out of a longer recording can end inside a multi-frame stretch (a load's or a menu's lag frames)
    # whose rows only match once it completes: such a gate expects 32 frames less than it holds
    doc['expect'] = {'matched': n if n == frames else n - 32}
    doc['full'] = {'frames': frames, 'matched': frames if exact else matched}
    sg.save(seg, doc)
    tot[3] += n
    print(f'| `{seg}` | {frames} | {"exact" if exact else matched} | {n} | {"" if exact else first[:90]} |')
sg.OUT = old
print(f'{name}: {len(res)} segments, {tot[0]} frames; {tot[2]} exact; {tot[1]} frames matched; {tot[3]} frames gated')
