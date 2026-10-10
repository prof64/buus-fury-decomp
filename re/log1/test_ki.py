"""Verify player ki move damage: Ki Blast = maxHP/3, Kamehameha = ki spent while charging.
Usage: test_ki.py MOVE HOLD_FRAMES"""
import sys
from game import *

move, hold = int(sys.argv[1]), int(sys.argv[2])
e = emu('log1_wolf.ss')
pc = player_char(e)
e.w8(PLAYER_CTL + 5, move)          # selected ki move
e.w32(pc + 0x50, 600); e.w32(pc + 0x4c, 600)   # maxHP 600 -> expect ki blast dmg 200
e.w32(pc + 0x58, 400); e.w32(pc + 0x54, 400)
wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]
e.w32(wolf['char'] + 0x60, 0)   # wolf defense 0 so damage applies exactly
e.w32(wolf['char'] + 0x4c, 5000)
e.hold(KEY.UP, 1); e.set_keys(0)   # face up toward the wolf
ki0 = e.r32(pc + 0x54)
e.breakpoint(0x08007416)
e.hold(KEY.B, hold)
ki1 = e.r32(pc + 0x54)
e.set_keys(0)
for i in range(120):
    e.run(1)
hs = [h for h in e.hits() if h['kind'] == 'break']
print('ki before', ki0, 'after charge', ki1, 'spent', ki0 - ki1, 'ki now', e.r32(pc + 0x54))
for h in hs:
    print('hit frame', h['frame'], 'att', hex(h['regs'][0]), 'def', hex(h['regs'][1]), 'flag', h['regs'][2], 'dmg', h['regs'][3])
print('wolf hp', e.r32(wolf['char'] + 0x4c))
