"""area_lock.py REPORT [--dry]: set "area_exact": true in every gate (data/segments/*.json with "expect") whose
SEGMENT_AREA report line says it is clean ("area NAME: N checks, 0 differ", N > 0) and that has its tracked area-local state
(data/segments/area/NAME.json) - from then on the segment test fails that gate on any area-local difference (wf-import area.rs).
REPORT: the segment test's stderr run with SEGMENT_AREA=1 (lines "    area NAME: ...")."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SEG = ROOT / 'data' / 'segments'
clean = set()
for ln in open(sys.argv[1]):
    m = re.match(r'\s+area (\S+): (\d+) checks, 0 differ', ln)
    if m and int(m.group(2)) > 0:
        clean.add(m.group(1))
n = 0
for p in sorted(SEG.glob('*.json')):
    s = p.read_text()
    nl = s.index('\n')
    head = s[:nl]
    m = re.search(r'"name":"([^"]+)"', head)
    if not m:
        continue
    name = m.group(1)
    if p.stem not in clean and name not in clean:
        continue
    if '"expect"' not in head or '"area_exact"' in head or not (SEG / 'area' / f'{name}.json').exists():
        continue
    n += 1
    if '--dry' not in sys.argv:
        p.write_text('{"area_exact":true,' + head[1:] + s[nl:])
print(n, 'gates locked' if '--dry' not in sys.argv else 'gates would be locked')
