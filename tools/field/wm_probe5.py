"""wm_probe5: the map screen (SELECT on a 2D map) - screenshots, which object runs, what it reads."""
from wm_lib import *  # noqa

e = emu('buu_swforest3.ss')
e.run(10)
print('area', area(e))
bp = e.breakpoint(0x080256F0)
e.set_keys(KEY.SELECT); e.run(1); e.set_keys(0)
e.run(1)
print('map screen ctor hit', any((h['addr'] & ~1) == 0x080256F0 for h in e.hits()))
e.clear(bp)
for f in range(120):
    e.run(1)
e.screenshot(str(WORK / 'wm_mapscreen0.png'))
# read the visited flags region while the screen is up
e.watch(0x03001068 + 0x8D, 0x03001068 + 0x8D + 0x40, 'r')
e.run(2)
reads = {(h['pc'] & ~1) for h in e.hits()}
print('visited-bit readers', [hex(x) for x in sorted(reads)][:10])
for k in (KEY.RIGHT, KEY.DOWN, KEY.A):
    e.set_keys(k); e.run(2); e.set_keys(0); e.run(40)
    e.screenshot(str(WORK / f'wm_mapscreen_{k}.png'))
print('area after', area(e))
