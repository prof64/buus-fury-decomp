"""Warp into every map from the intro and screenshot it; save states log1_mapNN.ss (in work/states/maps)."""
import sys
from common import *
from warp import warp, run_until_loadmap

out = STATES / 'log1_maps'
out.mkdir(exist_ok=True)
e = emu('log1_menus.ss')
assert run_until_loadmap(e, KEY.A, 3000)
base = e.save()
maps = [int(x) for x in sys.argv[1:]] or range(42)
for m in maps:
    e.load(base)
    from warp import default_entrance
    e.set_reg(0, m)
    e.set_reg(1, default_entrance(m))
    e.set_keys(0)
    e.run(150)
    e.save_file(out / f'log1_map{m:02d}.ss')
    e.screenshot(SHOTS / f'map{m:02d}.png', scale=1)
print('done')
