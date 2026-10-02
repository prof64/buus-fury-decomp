from common import *
e = emu('buu_fight_swforest.ss')
b1 = e.breakpoint(0x0800e9d8)
b2 = e.breakpoint(0x0800eabe)
for f in range(120):
    e.set_keys(KEY.A if f % 8 < 4 else 0)
    e.run(1)
    for h in e.hits():
        r = h['regs']
        print(h['frame'], hex(h['pc']), [hex(x) for x in r[:8]])
