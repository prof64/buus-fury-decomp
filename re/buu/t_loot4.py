"""Order of RNG calls vs drop natives during one kill's loot script."""
from common import *
from emutools import catch
from ents import enemies
REC = 0x03001560
EV = {0x0803FAAC: 'rng', 0x0800253C: 'zenie', 0x08002556: 'supply', 0x080025A4: 'equip', 0x08000726: 'nat19', 0x08000738: 'nat1a',
      0x08004098: 'runner'}
e = emu('buu_fight_swforest.ss')
en = enemies(e)
o = en[0]
p = e.r32(0x03001c34)
e.w32(o + 0x28c, 1 << 10)
e.w32(REC + 0x24, 60000000)
e.w8(p + 0x10, 3)
e.w32(o + 0x1a0, e.r32(p + 0x1a0) + (18 << 8)); e.w32(o + 0x1a4, e.r32(p + 0x1a4))
seq = []
active = [False]


def cb(e, pc):
    name = EV[pc]
    if name == 'runner':
        active[0] = e.reg(1) == u32(0x0879B704 + 4 * 497) or active[0]
    if active[0]:
        seq.append((e.frame, name, hex(e.reg(14))))


catch(e, list(EV), 200, lambda f: KEY.A if f < 2 else 0, cb)
for s in seq[:80]:
    print(s)
