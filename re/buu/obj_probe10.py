from common import *
import obj_list
e = emu(WORK / 'obj_8_1.ss')
P = e.r32(0x03001c34)
e.w32(P + 0x1a4, 275 << 8)
e.run(2)
e.set_keys(KEY.A); e.run(1); e.set_keys(0)
for f in range(400):
    e.run(1)
    if f % 50 == 0:
        P = e.r32(0x03001c34)
        print(f, 'area', hex(e.r32(0x03001980)), 'player', hex(P), hex(e.r32(P)) if P else None)
e.screenshot(str(WORK / 'obj_capsule.png'))
for k in range(8):
    e.set_keys(KEY.A); e.run(2); e.set_keys(0); e.run(60)
    P = e.r32(0x03001c34)
    print('A', k, 'area', hex(e.r32(0x03001980)), 'player', hex(P), hex(e.r32(P)) if P else None)
e.screenshot(str(WORK / 'obj_capsule2.png'))
e.save_file(WORK / 'obj_capsule.ss')
