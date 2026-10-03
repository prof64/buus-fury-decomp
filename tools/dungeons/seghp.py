"""seghp.py SEGDIR NAME [F0 F1]: replay a segment on the emulator and print the enemies' (type, x, y, HP/1024, AI
states) whenever an HP or the enemy count changes (the recording's rows do not carry enemy HP)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
f0 = int(sys.argv[3]) if len(sys.argv) > 3 else 0
f1 = int(sys.argv[4]) if len(sys.argv) > 4 else len(keys) - 1
e = start_emu(str(d / 'states' / f'{name}.ss'))
prev = None
for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    cur = [(o[5], o[4]) for o in enemies(e)]
    if f >= f0 and cur != prev:
        print(f, [(o[5], o[1], o[2], round(o[4] / 1024, 2), [hex(x) for x in ai_state(e, o[0])]) for o in enemies(e)])
    prev = cur
