"""segai.py SEGDIR NAME F0 F1 [INDEX]: replay a segment on the emulator and print per frame the enemies' (index in
the enemy list, x, y, facing, HP, AI state stack, anim tick +0x12 / frame +0x11); INDEX = only that enemy."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1 = int(sys.argv[3]), int(sys.argv[4])
only = int(sys.argv[5]) if len(sys.argv) > 5 else None
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0:
        out = []
        for k, o in enumerate(enemies(e)):
            if only is not None and k != only:
                continue
            out.append((k, o[5], o[1], o[2], o[3], round(o[4] / 1024, 1), [hex(x) for x in ai_state(e, o[0])],
                        e.r16(o[0] + 0x12), e.r8(o[0] + 0x11)))
        print(f, out)
