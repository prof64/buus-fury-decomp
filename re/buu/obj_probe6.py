from common import *
import obj_list
e = emu('buu_swforest3.ss')
P = e.r32(0x03001c34)
B = 0x203debc
print('player', hex(P), e.r32(P + 0x1a0) / 256, e.r32(P + 0x1a4) / 256, 'facing byte', e.r8(P + 0x10))
e.w32(P + 0x1a0, 344 << 8)
e.w32(P + 0x1a4, 226 << 8)
e.run(2)
e.set_keys(KEY.UP)
e.run(1)
e.set_keys(0)
e.run(10)
print('pos', e.r32(P + 0x1a0) / 256, e.r32(P + 0x1a4) / 256, 'facing', e.r8(P + 0x10))
for k in range(12):
    e.set_keys(KEY.A)
    e.run(1)
    e.set_keys(0)
    for f in range(30):
        e.run(1)
        hp = e.r8(B + 0xC)
    print(k, 'hp', e.r8(B + 0xC), 'shake', e.r32(B + 0x24), 'vt', hex(e.r32(B)), 'pos', e.r32(P + 0x1a0) / 256, e.r32(P + 0x1a4) / 256)
e.run(30)
obj_list.dump(e)
