"""trim.py data/segments/NAME.json N  -- keep the first N frames of a gated segment (expect.matched = N): a long exact
cut is gated on its first window, the full cut stays local (work/db57/segs*)."""
import json
import sys

p, n = sys.argv[1], int(sys.argv[2])
d = json.load(open(p))
tot = len(d['frames'])
d['frames'] = d['frames'][:n]
d['expect'] = {'matched': n}
d['note'] = f'the first {n} frames of a {tot}-frame cut (the full cut: work/db57/segs*)'
frames = d.pop('frames')
head = json.dumps(d, separators=(',', ':'))
body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
open(p, 'w').write(head[:-1] + ',"frames":[\n' + body + '\n]}\n')
print(p, n, 'of', tot)
