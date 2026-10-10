"""wm_probe1: from the capsule state, press A, follow into the world map 0-50; dump player object and save a state."""
from common_field import *  # noqa
from common import WORK

e = emu(WORK / 'obj_capsule.ss')
P = e.r32(PLAYER)
print('area', area(e), 'player', hex(P), 'vt', hex(e.r32(P)), 'fc', e.r32(FC))
e.set_keys(KEY.A); e.run(1); e.set_keys(0)
last = None
for f in range(400):
    e.run(1)
    a = area(e)
    p = e.r32(PLAYER)
    vt = e.r32(p) if p else 0
    key = (tuple(a), vt)
    if key != last:
        print(f, a, hex(p), hex(vt), 'fc', e.r32(FC))
        last = key
p = e.r32(PLAYER)
print('player words:')
for o in range(0, 0x260, 0x10):
    print(hex(o), ' '.join('%08x' % e.r32(p + o + k) for k in range(0, 16, 4)))
e.save_file(WORK / 'wm_start.ss')
e.screenshot(str(WORK / 'wm_start.png'))
