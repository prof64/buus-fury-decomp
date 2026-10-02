"""Test the invincibility code checked by 0x080048FA (sequence at 0x08011462: UP DOWN LEFT RIGHT B A)."""
from common import *

e = emu()
seq = [KEY.UP, KEY.DOWN, KEY.LEFT, KEY.RIGHT, KEY.B, KEY.A]
e.breakpoint(0x080048FA)
hit_frame = None
for f in range(3000):
    e.set_keys(KEY.START if (f // 10) % 20 == 0 and f > 1500 else 0)
    e.run(1)
    if any(h['kind'] == 'break' for h in e.hits()):
        hit_frame = e.frame
        break
print('scene callback 0x080048FA first runs at frame', hit_frame)
e.screenshot(SHOTS / 'cheat_scene.png', scale=1)
for k in seq:
    e.press(k, 2, 4)
e.run(10)
print('god-mode flag 0x03000C18 =', e.r32(0x03000C18))
e.screenshot(SHOTS / 'cheat_after.png', scale=1)
