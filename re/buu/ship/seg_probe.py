"""seg_probe.py SEGDIR NAME a b -- replay a recorded segment's keys from its start state (SEGDIR/states/NAME.ss) and
print, for video frames a..b, the player and every enemy (x, y px, hp, type, sprite def, AI state stack, anim)"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import status, enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name, a, b = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(min(b + 1, len(keys))):
    e.set_keys(keys[f])
    e.run(1)
    if f >= a:
        s = status(e)
        print(f, f'k{keys[f]:03x}', s['p'], 'rng', [hex(e.r32(0x03004A5C)), hex(e.r32(0x03004A60))],
              [(o[1], o[2], o[4] >> 10, o[5], e.r32(o[0] + 0x58) and hex(e.r32(o[0] + 0x58)),
                [hex(x) for x in ai_state(e, o[0])], e.r16(o[0] + 0x1E4)) for o in enemies(e)])
