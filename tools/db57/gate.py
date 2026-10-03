"""gate.py SEGDIR LOG [--min N] [--tail N]  -- copy a chapter's replayed segments into data/segments as gates.

LOG = the segments test's output for SEGDIR (lines `segment NAME: T video frames (...), matched M / T | ...`).
An exact segment (M == T) is gated whole; one that stops matching (today: a pause menu's redraw timing, the Earth
agent's open issue) is gated on its first-matched window: frames cut to M + `tail` (default 0: the file ends where the
match ends), `expect.matched` = M. Segments matching fewer than `min` frames (default 200) are left out."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import segment as sg  # noqa: E402

a = sys.argv[1:]
segdir, log = Path(a[0]), Path(a[1])
mn = int(a[a.index('--min') + 1]) if '--min' in a else 200
tail = int(a[a.index('--tail') + 1]) if '--tail' in a else 0
out = ROOT / 'data' / 'segments'
tot = [0, 0, 0]
for line in log.read_text().splitlines():
    m = re.match(r'segment (\S+): (\d+) video frames .*matched (\d+) / (\d+)', line)
    if not m:
        continue
    name, total, matched = m.group(1), int(m.group(2)), int(m.group(3))
    if matched < mn:
        print('skip', name, matched, '/', total)
        continue
    doc = json.loads((segdir / f'{name}.json').read_text())
    if matched < total:
        doc['frames'] = doc['frames'][:matched + tail]
        doc['note'] = f'first-matched window of a {total}-frame cut (the full cut: {segdir.relative_to(ROOT) if segdir.is_absolute() else segdir})'
    doc['expect'] = {'matched': matched}
    frames = doc.pop('frames')
    head = json.dumps(doc, separators=(',', ':'))
    body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
    (out / f'{name}.json').write_text(head[:-1] + ',"frames":[\n' + body + '\n]}\n')
    tot[0] += 1
    tot[1] += matched
    tot[2] += total
print('gated', tot[0], 'segments,', tot[1], 'matched frames of', tot[2])
