"""world_lock.py REPORT [--dry]: set "world_exact": true in every gate (data/segments/*.json with "expect") whose
SEGMENT_WORLD report line says it is clean ("world NAME: N checks, 0 differ", N > 0) and that has its tracked world state
(data/segments/world/NAME.json) - from then on the segment test fails that gate on any world-state difference (wf-import world.rs).
REPORT: the segment test's stderr run with SEGMENT_WORLD=1 (lines "    world NAME: ...")."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SEG = ROOT / 'data' / 'segments'
clean = set()
for ln in open(sys.argv[1]):
    m = re.match(r'\s+world (\S+): (\d+) checks, 0 differ', ln)
    if m and int(m.group(2)) > 0:
        clean.add(m.group(1))
n = 0
for p in sorted(SEG.glob('*.json')):
    s = p.read_text()
    nl = s.index('\n')
    head = s[:nl]
    name = re.search(r'"name":"([^"]+)"', head).group(1)
    if p.stem not in clean and name not in clean:
        continue
    if '"expect"' not in head or '"world_exact"' in head or not (SEG / 'world' / f'{name}.json').exists():
        continue
    n += 1
    if '--dry' not in sys.argv:
        p.write_text('{"world_exact":true,' + head[1:] + s[nl:])
print(n, 'gates locked' if '--dry' not in sys.argv else 'gates would be locked')
