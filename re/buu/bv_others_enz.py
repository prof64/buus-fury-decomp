"""Enzyme latch: which object is [0x03001b94], what is its vfunc +0x5c, and what happens to the player
(position, state, input lock) while latched."""
from common import *
from vtab import entries
from bv_buu_lib import setup, pos, REC
from forcespawn import setpos
from ents import PLAYER

e, o = setup(0x18, dx=4, dy=0)
p = e.r32(PLAYER)
print('player', hex(p), '[0x3001b94]', hex(e.r32(0x03001b94)), 'player vt', hex(e.r32(p)))
vt = e.r32(e.r32(0x03001b94))
ent = entries(vt, 0x30)
print('vfunc +0x5c ->', hex(ent[0x5c // 4]), ' +0x80 ->', hex(ent[0x80 // 4]), '+0x24 ->', hex(ent[0x24 // 4]))
print(dis(ent[0x5c // 4] & ~1, 40))
# run unpinned, hold RIGHT, watch player pos / ctrl / enemy state
log = []
for f in range(300):
    e.set_keys(KEY.RIGHT if f < 200 else 0)
    e.run(1)
    log.append((f, pos(e, p), pos(e, o), hex(e.r32(p + 0x14)), e.r16(REC + 0x16), e.r16(REC + 0x1a),
                hex(e.r32(e.r32(o + 0x238 + 4)) if e.r32(o + 0x238 + 4) else 0)))
for l in log[::15]:
    print(l)
