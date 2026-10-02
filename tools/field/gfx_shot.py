"""gfx_shot.py W A X Y [FRAMES] [OUT]: warp to area W-A with the player at (X, Y), wait, screenshot
work/field_gfx/shot_<W>_<A>.png and dump the field objects + OAM (gfx_probe helpers)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rec_objects as R  # noqa: E402
from gfx_probe import oam, cam, OUT  # noqa: E402
from warp import warp  # noqa: E402

w, a, x, y = (int(v) for v in sys.argv[1:5])
n = int(sys.argv[5]) if len(sys.argv) > 5 else 30
e = R.emu('buu_snakeway_start.ss')
assert warp(e, w, a, x, y)
R.place(e, x, y, 0, settle=n)
os.makedirs(OUT, exist_ok=True)
path = sys.argv[6] if len(sys.argv) > 6 else os.path.join(OUT, f'shot_{w}_{a}.png')
e.screenshot(path, scale=1)
print('cam', cam(e), '->', path)
for o in R.objects(e):
    print('  obj', o)
for o in oam(e):
    print('  oam', o)
