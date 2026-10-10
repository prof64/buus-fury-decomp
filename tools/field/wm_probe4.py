"""wm_probe4: video registers / DMA on the world map; screenshots; z -> pitch table check."""
from wm_lib import *  # noqa

e = emu(WORK / 'obj_capsule.ss')
e.run(2)
io = 0x04000000
print('DISPCNT', hex(e.r16(io)), 'BG0..3CNT', [hex(e.r16(io + 8 + 2 * k)) for k in range(4)])
for ch in range(4):
    b = io + 0xB0 + 12 * ch
    print('DMA', ch, hex(e.r32(b)), hex(e.r32(b + 4)), hex(e.r16(b + 8)), hex(e.r16(b + 10)))
print('BG2 PA..PD', [hex(e.r16(io + 0x20 + 2 * k)) for k in range(4)], 'X/Y', hex(e.r32(io + 0x28)), hex(e.r32(io + 0x2C)))
print('WIN/BLD', hex(e.r16(io + 0x50)), hex(e.r16(io + 0x52)), hex(e.r16(io + 0x40)), hex(e.r16(io + 0x44)), hex(e.r16(io + 0x48)), hex(e.r16(io + 0x4A)))
e.screenshot(str(WORK / 'wm_shot0.png'))
e.set_keys(KEY.UP | KEY.RIGHT)
e.run(40)
e.screenshot(str(WORK / 'wm_shot1.png'))
e.set_keys(KEY.B)
e.run(60)
e.screenshot(str(WORK / 'wm_shot2_high.png'))
