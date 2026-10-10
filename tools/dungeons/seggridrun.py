"""seggridrun.py SEGDIR NAME F0 F1 TY TX0 TX1 [ENEMY]: per frame F0..F1 the game's live enemy-layer grid row TY
(tiles TX0..TX1; '.' free) with enemy ENEMY's (default 0) 24.8 position, facing, AI stack and anim tick."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1, ty, x0, x1 = map(int, sys.argv[3:8])
k = int(sys.argv[8]) if len(sys.argv) > 8 else 0
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
prev = None
for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f < f0:
        continue
    g = e.r32(0x030019A4 + 0xB0)
    rows = []
    for y in (ty, ty + 1):
        row = e.read(g + y * 32, 32)
        rows.append(''.join('.' if row[tx >> 3] >> (tx & 7) & 1 else '#' for tx in range(x0, x1)))
    en = enemies(e)
    o = en[k] if k < len(en) else None
    info = None
    if o:
        sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        info = (sv(e.r32(o[0] + 0x1A0)) / 256, sv(e.r32(o[0] + 0x1A4)) / 256, o[3], [hex(x) for x in ai_state(e, o[0])], e.r16(o[0] + 0x12))
    cur = (rows, info and info[3])
    if cur != prev or '--all' in sys.argv:
        print(f, rows[0], rows[1], info)
    prev = cur
