"""bv_janemba_death.py type [frames] -- kill the force-spawned boss through the normal enemy damage path
(0x0800FA2A with a lethal hit) and log what follows: enemy creations (0x0800F68C, type from r2), script
runs (0x08004098, r1 = script header -> index in 0x0879B704), story-flag writes, and the screen."""
import sys
from common import *
from emutools import call
from bv_janemba_spawn import jspawn, REC
from ents import enemies, PLAYER, TYPE_TAB

t = int(sys.argv[1], 0)
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 900
e, o, ok = jspawn(t)
HIT = 0x0203FE00
SCRIPTS = {u32(0x0879B704 + 4 * i): i for i in range(522)}
for i in range(0, 0x20, 4):
    e.w32(HIT + i, 0)
e.w32(HIT + 4, 0x400); e.w32(HIT + 8, 0x086B8098); e.w32(HIT + 0xC, e.r32(PLAYER)); e.w32(HIT + 0x18, 0x7FFFFFF)
e.breakpoint(0x0800F68C); e.breakpoint(0x08004098); e.breakpoint(0x0800AA4A)
call(e, 0x0800FA2A, (o, HIT, 0, 0))
S = Shots(e, f'jdeath_{t:02x}')
for f in range(frames):
    e.run(1)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    for h in e.hits():
        a, r = h['addr'], h['regs']
        if a == 0x0800F68C:
            print(f'{f:5d} create enemy type {(r[2] - TYPE_TAB) // 28:#x} lr {h["lr"]:#x}')
        elif a == 0x08004098:
            print(f'{f:5d} run script {SCRIPTS.get(r[1], hex(r[1]))} lr {h["lr"]:#x}')
        elif a == 0x0800AA4A:
            ty = (e.r32(r[0] - 0x238 + 0x224) - TYPE_TAB) // 28
            if ty != 0x44:
                print(f'{f:5d} goto type {ty:#x} state {r[1]:#x}')
    if f % 60 == 0 and f < 16 * 60:
        S.snap()
S.write()
print('roster', [hex((e.r32(x + 0x224) - TYPE_TAB) // 28) for x in enemies(e)])
