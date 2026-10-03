"""mkrun.py START.ss KEYS.json OUTDIR [N]: a rec_segments.py run directory (start.ss, keys.json, end.ss) from a state and
a key log ({keys: [...]}; the first N frames only); prints the status every 5000 frames and at the end."""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status  # noqa: E402
from route import start_emu  # noqa: E402

st, kj, out = sys.argv[1], sys.argv[2], Path(sys.argv[3])
doc = json.loads(Path(kj).read_text())
keys = doc['keys']
print('log', {k: v for k, v in doc.items() if k != 'keys'}, len(keys))
if len(sys.argv) > 4:
    keys = keys[:int(sys.argv[4])]
out.mkdir(parents=True, exist_ok=True)
shutil.copyfile(st, out / 'start.ss')
e = start_emu(st)
for f, k in enumerate(keys):
    e.set_keys(k)
    e.run(1)
    if f % 5000 == 0:
        print(f, status(e), flush=True)
e.set_keys(0)
e.save_file(str(out / 'end.ss'))
(out / 'keys.json').write_text(json.dumps({'from': None, 'keys': keys}))
print(len(keys), status(e))
