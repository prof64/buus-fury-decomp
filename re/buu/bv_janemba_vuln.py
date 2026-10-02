"""bv_janemba_vuln.py state.ss type -- can the player damage the boss right now?  Moves the player next to
the boss (left of it, facing right), punches 6 times, logs ComputeDamage calls on the boss and its HP."""
import sys
from common import *
from ents import enemies, PLAYER, TYPE_TAB

e = emu(WORK / sys.argv[1])
t = int(sys.argv[2], 0)
REC = 0x03001560
e.w32(REC + 0x24, 60000000)
b = [o for o in enemies(e) if (e.r32(o + 0x224) - TYPE_TAB) // 28 == t][0]
p = e.r32(PLAYER)
st = e.r32(b + 0x238 + 8 + 4)
print('boss state', hex((st - 0x0870F514) // 8), 'hp', e.r32(b + 0x28c) / 1024, 'z/+0x176', e.r16(b + 0x176))
bid = e.breakpoint(0x0800E9D8)
hp0 = e.r32(b + 0x28c)
for k in range(6):
    e.w32(p + 0x1a0, e.r32(b + 0x1a0) - (18 << 8)); e.w32(p + 0x1a4, e.r32(b + 0x1a4))
    e.w8(p + 0x10, 3)
    for f in range(24):
        e.set_keys(KEY.A if f < 2 else 0)
        e.run(1)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
hs = [h for h in e.hits() if h['regs'][0] == b]
print('ComputeDamage calls on boss', len(hs), 'hp', hp0 / 1024, '->', e.r32(b + 0x28c) / 1024)
