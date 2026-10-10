"""Verify exp award + level-up: player exp 340, kill a wolf (exp reward 70) -> level 2."""
from game import *

e = emu('log1_wolf.ss')
pc = player_char(e)
e.w32(pc + 0x84, 340)
e.w32(pc + 0x4c, 80)        # current HP 80 / 105 to see HP delta behaviour
e.w32(pc + 0x54, 30)        # ki 30 / 100
wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]['char']
e.w32(wolf + 0x60, 0); e.w32(wolf + 0x4c, 1)
print('before', char_info(e, pc), 'lvl idx', e.r32(0x03000988), 'exp glob', e.r32(0x03000984))
e.hold(KEY.UP, 20); e.set_keys(0)
for f in range(200):
    e.set_keys(KEY.A if f % 8 < 2 else 0)
    e.run(1)
    if not e.r16(wolf + 0x9c):
        break
e.set_keys(0); e.run(5)
print('after ', char_info(e, pc), 'lvl idx', e.r32(0x03000988), 'exp glob', e.r32(0x03000984))
print('ctl', e.read(PLAYER_CTL, 0x40).hex(' '))
e.run(120)
e.screenshot(SHOTS / 'levelup.png', scale=1)
