"""OBJ tile pixels of an OAM entry at a scenario frame: c10.py SCENARIO FRAME TILE W H"""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu')
from cap import Cap, program
import capture
name, frame, tile, w, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
state, setup, pre, keys, *rest = capture.SCENARIOS[name]
extra = rest[0] if rest else {}
c = Cap(state)
if setup:
    setup(c.e)
for k in program(pre):
    c.step(k, keep=False)
if extra.get('trigger'):
    extra['trigger'](c.e)
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    if i == frame:
        break
e = c.e
# 8bpp, 1D mapping: tile number counts 32-byte units
base = 0x06010000 + tile * 32
tw = w // 8
for y in range(h):
    row = []
    for x in range(w):
        t = (y // 8) * tw + (x // 8)
        a = base + t * 64 + (y % 8) * 8 + (x % 8)
        row.append(e.r8(a))
    print(' '.join(f'{v:3d}' if v else '  .' for v in row))
