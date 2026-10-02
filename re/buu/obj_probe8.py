from common import *
from emutools import call
import obj_list
e = emu('buu_snakeway_start.ss')
P = e.r32(0x03001c34)
S = 0x0203FF00
e.w32(S, 230); e.w32(S + 4, 160); e.w32(S + 8, 0); e.w32(S + 12, 0)
print('scratch before', e.read(S, 16).hex())
obj = call(e, 0x0800D30E, (0, 0, S, S + 8))
print('obj', hex(obj))
for f in range(5):
    e.set_keys(KEY.RIGHT); e.run(1)
    print(f, e.r32(P + 0x1a0) >> 8, hex(e.r32(e.r32(P + 0x14))))
call(e, 0x0801191A, (0x03001C4C, obj))
for f in range(5):
    e.set_keys(KEY.RIGHT); e.run(1)
    print(f, e.r32(P + 0x1a0) >> 8, hex(e.r32(e.r32(P + 0x14))))
obj_list.dump(e)
