"""wm_probe2: per-frame flight state from obj_capsule.ss (already on the world map)."""
import sys
from wm_lib import *  # noqa

e = emu(WORK / 'obj_capsule.ss')
print('map size', e.r32(MAPOBJ + 0x94), e.r32(MAPOBJ + 0x98), 'cam', camera(e), 'fc', e.r32(FC))
seq = [(0, 10), (KEY.UP, 40), (KEY.UP | KEY.RIGHT, 30), (KEY.UP, 10), (0, 30), (KEY.DOWN, 30), (KEY.LEFT, 20), (KEY.A, 40), (KEY.B, 30)]
f = 0
for k, n in seq:
    e.set_keys(k)
    for _ in range(n):
        fc = e.r32(FC)
        e.run(1)
        r = flyer(e)
        print(f, k, e.r32(FC) - fc, r and [r['x'], r['y'], r['z'], r['h'], r['facing'], r['raw'], r['aframe'], r['rev'], r['spd'], r['turn']], camera(e), rng(e), area(e))
        f += 1
