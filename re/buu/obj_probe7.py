"""spawn a supply drop next to the player in swforest and find who sends message 3 (collect)"""
from common import *
from emutools import call
import obj_list
e = emu('buu_snakeway_start.ss')
P = e.r32(0x03001c34)
px, py = e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8
print('player', px, py)
S = 0x0203FF00
e.w32(S, px + 40); e.w32(S + 4, py); e.w32(S + 8, 0); e.w32(S + 12, 0)
item = 0  # supply 0
obj = call(e, 0x0800D30E, (0, item, S, S + 8))
print('item obj', hex(obj))
call(e, 0x0801191A, (0x03001C4C, obj))
bid = e.breakpoint(0x0800d78a)
seq = [KEY.RIGHT] * 40 + [0] * 5 + [KEY.A] + [0] * 30 + [KEY.A] + [0] * 30
for f, k in enumerate(seq):
    e.set_keys(k)
    e.run(1)
    hs = [h for h in e.hits() if h['regs'][1] == 3]
    for h in hs:
        print('f', f, 'msg3 lr', hex(h['lr']), 'msg', [e.r32(h['regs'][2] + 4 * k) for k in range(5)], 'player', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8, 'cnt', e.r8(0x03001068 + 2 + item))
e.clear(bid)
e.set_keys(0)
e.run(80)
print('supply count ->', e.r8(0x03001068 + 2 + item), 'player', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
