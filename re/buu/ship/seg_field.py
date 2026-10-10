"""seg_field.py SEGDIR NAME OFF [W] -- replay a segment; print, whenever it changes, the u32 at +OFF (or u8 with W=1)
of every enemy (enemy list 0x03001BB0) with its type and AI state"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name, off = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3], 0)
w = int(sys.argv[4]) if len(sys.argv) > 4 else 4
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
last = {}
for f in range(len(keys)):
    e.set_keys(keys[f])
    e.run(1)
    for o in enemies(e):
        v = e.r8(o[0] + off) if w == 1 else e.r32(o[0] + off)
        if last.get(o[0]) != v:
            print(f, hex(o[0]), 'type', o[5], 'val', v, [hex(x) for x in ai_state(e, o[0])])
            last[o[0]] = v
