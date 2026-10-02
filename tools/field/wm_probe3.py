"""wm_probe3: who writes the camera and the player's position on the world map (watchpoints)."""
from wm_lib import *  # noqa

e = emu(WORK / 'obj_capsule.ss')
c = e.r32(CAM_PTR)
p = e.r32(PLAYER)
print('cam struct', hex(c), 'player', hex(p))
e.set_keys(KEY.UP | KEY.RIGHT)
e.run(20)
e.watch(c, c + 8, 'w')
e.watch(p + 0x1A0, p + 0x1B0, 'w')
e.run(1)
seen = set()
for h in e.hits():
    k = (h['addr'], h['pc'], h['lr'])
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['addr']), 'pc', hex(h['pc']), 'lr', hex(h['lr']), 'new', hex(h['new']))
print('cam words')
for o in range(0, 0xA0, 0x10):
    print(hex(o), ' '.join('%08x' % e.r32(c + o + k) for k in range(0, 16, 4)))
print('mapobj')
for o in range(0x80, 0xC0, 0x10):
    print(hex(o), ' '.join('%08x' % e.r32(MAPOBJ + o + k) for k in range(0, 16, 4)))
