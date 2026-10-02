"""Verify enemy wake radius: an enemy in the dormant list wakes by proximity (0x0800819C) when
dist^2(player pos, enemy char+0x20) < char+0x0E (= 96*aggro_tiles^2) and it is near the screen centre.
Warp next to the map-2 wolf at (256,56) (char slot 0x0201523C) at several vertical distances."""
from game import *
from warp import run_until_loadmap, SCRATCH

WOLF_CHAR = 0x0201523C
base = emu('log1_menus.ss')
assert run_until_loadmap(base, KEY.A, 3000)
st = base.save()
for dy in (60, 80, 90, 94, 96, 98, 100, 104, 110):
    e = base
    e.load(st)
    e.w16(SCRATCH, 256); e.w16(SCRATCH + 2, 56 + dy)
    e.set_reg(0, 2); e.set_reg(1, SCRATCH)
    e.set_keys(0)
    b1 = e.breakpoint(0x0800819C)
    b2 = e.breakpoint(0x080081DE)
    reasons = []
    first = None
    for f in range(300):
        e.run(1)
        for hh in e.hits():
            if hh['kind'] == 'break' and hh['regs'][4] == WOLF_CHAR:
                reasons.append(('proximity' if hh['pc'] - 2 == 0x0800819C else 'roam', e.frame))
        if first is None and e.r16(WOLF_CHAR + 0xE):
            px, py = player_xy(e)
            wx, wy = s16(e.r16(WOLF_CHAR + 0x20)), s16(e.r16(WOLF_CHAR + 0x22))
            first = (px, py, wx, wy, (px - wx) ** 2 + (py - wy) ** 2, e.r16(WOLF_CHAR + 0xE))
    e.clear(b1); e.clear(b2)
    print('dy', dy, 'player/wolf/dist2/radius2', first, 'wake events', reasons[:2])
