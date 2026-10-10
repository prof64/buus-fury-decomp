"""mkgates.py RESULTS SRC_DIR [--skip name,name] [--only name,name] [--novram]: turn a `cargo test -p wf-import --test segments -- --nocapture` log
(the `segment NAME: ... matched N / M` lines of SRC_DIR's cuts) into gated segments under data/segments: a cut
smaller than 2 MB whole, a bigger one as its first matched + 120 frames (`note` says so); `expect.matched` = N.
Cuts that match nothing are left out."""
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
res, src = sys.argv[1], Path(sys.argv[2])
skip = set(sys.argv[sys.argv.index('--skip') + 1].split(',')) if '--skip' in sys.argv else set()
only = set(sys.argv[sys.argv.index('--only') + 1].split(',')) if '--only' in sys.argv else None
# --novram: drop the allocator rows (a cut whose allocator check stops before the rest does)
novram = '--novram' in sys.argv
for line in open(res):
    m = re.match(r'segment (\S+): (\d+) video.*matched (\d+) / \d+', line)
    if not m:
        continue
    name, matched = m.group(1), int(m.group(3))
    p = src / f'{name}.json'
    if matched == 0 or name in skip or not p.exists() or (only is not None and name not in only):
        continue
    d = json.load(open(p))
    if novram:
        d.get('init', {}).pop('vram', None)
        for f in d['frames']:
            f.pop('vram', None)
    d.pop('expect', None)
    d.pop('note', None)
    out = {'expect': {'matched': matched}}
    if os.path.getsize(p) >= 2_000_000 and matched + 120 < len(d['frames']):
        keep = matched + 120
        d['frames'] = d['frames'][:keep]
        out['note'] = f'first {keep} frames of the cut (the rest diverges; full cut: {src.name}/{name}.json)'
    out.update(d)
    frames = out.pop('frames')
    head = json.dumps(out, separators=(',', ':'))
    body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
    q = ROOT / 'data' / 'segments' / f'{name}.json'
    q.write_text(head[:-1] + ',"frames":[\n' + body + '\n]}\n')
    print(name, matched, len(frames), os.path.getsize(q))
