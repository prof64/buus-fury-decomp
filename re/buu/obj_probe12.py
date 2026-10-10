"""8-57: map item pickup; who sends message 3"""
from common import *
from warp import warp
import obj_list
e = emu('buu_snakeway_start.ss')
ok = warp(e, 8, 57, 112, 170)
P = e.r32(0x03001c34)
print('warp', ok, hex(e.r32(0x03001980)), e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
e.save_file(WORK / 'obj_8_57.ss')
obj_list.dump(e)
it = [o for o in obj_list.objects(e) if e.r32(o) == 0x0804C508]
print('items', [hex(o) for o in it])
if it:
    print(e.read(it[0], 0x84).hex())
bid = e.breakpoint(0x0800d78a)
for f in range(120):
    e.set_keys(KEY.UP)
    e.run(1)
    for h in e.hits():
        if h['regs'][1] == 3:
            print('f', f, 'msg3 lr', hex(h['lr']), 'msg', [e.r32(h['regs'][2] + 4 * k) for k in range(5)], 'player', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
e.clear(bid)
e.set_keys(0)
print('cnt37', e.r8(0x03001068 + 2 + 37), 'player', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
