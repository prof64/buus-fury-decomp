"""npc_probe.py W M X Y [frames]: warp, then print NPC objects / their behaviour step state and RNG draws."""
import sys
from npc_lib import *

w, m, x, y = (int(a, 0) for a in sys.argv[1:5])
frames = int(sys.argv[5]) if len(sys.argv) > 5 else 120
e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, w, m, x, y)
area0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
loaded = None
for f in range(600):
    fix()
    fc = e.r32(FRAME_COUNTER)
    r0 = rng(e)
    e.run(1)
    if e.r32(AREA_PTR) != area0:
        e.set_keys(0)
        if loaded is None:
            loaded = f
            print('area changed at', f, 'rng before', [hex(v) for v in r0], 'after', [hex(v) for v in rng(e)])
    if loaded is not None and f > loaded + 3:
        break
e.set_keys(0)
for f in range(frames):
    fc = e.r32(FRAME_COUNTER)
    r0 = rng(e)
    e.run(1)
    npcs = npc_objects(e)
    if f < 10 or f % 20 == 0:
        print(f, 'fc+', e.r32(FRAME_COUNTER) - fc, 'rng', hex(e.r32(RNG)), 'P', player_row(e), 'npcs', len(npcs))
for o in npc_objects(e):
    print(npc_row(e, o), step_state(e, o))
