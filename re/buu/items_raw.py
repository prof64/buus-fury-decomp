from common import *
from strtab import S
from scriptdis import dis_script
T = 0x083bd2f8
for i in range(120):
    a = T + 0x34 * i
    f0, sn, nm, pl = struct.unpack_from('<HHHH', rom(), ra(a))
    desc = u32(a + 8)
    if S(nm) is None or nm < 0x200 or nm > 0x600:
        print('end', i, hex(a)); break
    price = s32(a + 0x2c)
    eff = u32(a + 0x30)
    code = '; '.join(s for _, s in dis_script(eff, 200)) if 0x08000000 <= eff < 0x08800000 else hex(eff)
    print(f'{i:3d} f0={f0} {S(nm)!r:24} price={price} eff={eff:08x}: {code}')
