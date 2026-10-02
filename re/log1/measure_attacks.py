"""Measure enemy attack cadence and damage vs. formula. Usage: measure_attacks.py STATE ENEMYNAME FRAMES"""
import sys
from game import *

st, name, frames = sys.argv[1], sys.argv[2], int(sys.argv[3])
e = emu(st)
pc = player_char(e)
e.w32(pc + 0x50, 99999); e.w32(pc + 0x4c, 99999); e.w32(pc + 0x60, 0)
e.w32(0x03000C18 + 4, 999999)
e.breakpoint(0x08007416)
e.hold(KEY.UP, 20); e.set_keys(0)
rows = []
for f in range(frames):
    e.run(1)
    for h in e.hits():
        if h['kind'] == 'break' and h['regs'][1] == pc:
            a = h['regs'][0]
            rows.append((h['frame'], hex(a), h['regs'][2], h['regs'][3], s32v(e.r32(a + 0x4c)), s32v(e.r32(a + 0x54))))
prev = None
for r in rows:
    print(r, 'dt', r[0] - prev if prev else '')
    prev = r[0]
