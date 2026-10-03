"""gridat.py MAP TX0 TY0 TX1 TY1: the content walk grid of a map (collision.json) over a tile window, '.' walkable."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from grid import Grid  # noqa: E402

g = Grid(sys.argv[1])
x0, y0, x1, y1 = map(int, sys.argv[2:6])
for ty in range(y0, y1 + 1):
    print('%3d ' % ty + ''.join('.' if g.ok(tx, ty) else '#' for tx in range(x0, x1 + 1)))
