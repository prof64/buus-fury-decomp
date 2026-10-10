"""Try to start the Raditz fight on map 9: talk to Raditz, advance dialogue, watch for the NPC turning
into an enemy (handler 0x08005605) and log the fight (hits both ways)."""
from game import *

e = emu('log1_raditz.ss')
pc = player_char(e)
rad_ent = [p for p in ents(e) if e.r8(p + 0x15) == 84][0]
e.hold(KEY.UP, 4); e.set_keys(0)
started = None
for i in range(400):
    e.press(KEY.A, 2, 12)
    if e.r32(rad_ent + 0x38) == 0x08005605:
        started = e.frame
        break
print('fight started at', started)
e.screenshot(SHOTS / 'raditz_start.png', scale=1)
e.save_file(STATES / 'log1_raditz_fight.ss')
rc = e.r32(rad_ent + 0x3c)
print('raditz char', hex(rc), char_info(e, rc) if rc else None)
