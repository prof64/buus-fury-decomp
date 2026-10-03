"""mkcuts.py MASTER OUTDIR [--min N]: cut a master recording (segment.py record) into segments at its area entries that
are not inside an event (depth 0 at the cut), each running to the next such entry (at least N frames)
-> OUTDIR/sb_<n>_<w>_<a>.json + work/segments/<name>.ss. Prints the list."""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from segment import load_doc, area_cuts  # noqa: E402

master, out = sys.argv[1], Path(sys.argv[2])
mn = int(sys.argv[sys.argv.index('--min') + 1]) if '--min' in sys.argv else 200
head, frames = load_doc(master)
cuts = [(k, a, n) for k, a, n in area_cuts(frames) if frames[k]['depth'] == 0 and frames[min(k + 1, len(frames) - 1)]['depth'] == 0]
sel = []
for i, (k, a, n) in enumerate(cuts):
    nxt = next((c[0] for c in cuts[i + 1:] if c[0] - k >= mn), len(frames) - 1)
    sel.append((k, a, n, nxt - k))
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SEGMENT_DIR=str(out))
done = set()
for i, (k, a, n, ln) in enumerate(sel):
    if any(k < d < k + ln for d in done):
        continue
    name = f'sb_{i:03d}_{a[0]:02d}_{a[1]:02d}'
    spec = f'area:{a[0]}-{a[1]}#{n}'
    r = subprocess.run([str(ROOT / '.venv/bin/python'), str(ROOT / 'tools/segments/segment.py'), 'cut', '--from', master,
                        '--at', spec, '--frames', str(ln), '--name', name], env=env, capture_output=True, text=True)
    print(name, spec, k, ln, (r.stdout.strip().split('\n') or [''])[-1][-60:], r.stderr.strip()[-200:], flush=True)
    done.add(k)
