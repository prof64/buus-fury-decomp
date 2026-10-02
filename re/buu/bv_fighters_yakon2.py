"""Yakon state 0x326 (downward mouth beam): player kept below the beam origin; log ComputeDamage hit structs."""
from bv_fighters_lib import *

e, o, ok = spawn2(0x91)
p = e.r32(PLAYER)
hits = []


def cb(e, pc):
    h = e.reg(1)
    at = e.r32(h + 8)
    hits.append((e.frame, e.r32(h + 4), hex(at), e.r8(at + 1), hex(e.r32(h + 0xc))))


for f in range(560):
    e.w16(REC + 0x16, 9999)
    if f >= 270:
        bx, by = pos(e, o)
        setpos(e, p, bx + 32, by + 60)
    catch(e, [DMG], 1, lambda _: 0, cb)
print('yakon obj', hex(o), len(hits))
for h in hits[:12]:
    print(h)
print('...', hits[-1] if hits else None)
