from common import *
from strtab import S
T = 0x0871daf8
for i in range(0x18):
    a = T + 24 * i
    mx, ic1, ic2, nm, ds, f1, f2 = struct.unpack_from('<IIIHHII', rom(), ra(a))
    print(f'==== skill {i:#x} {S(nm)!r} max={mx} canUse={f1:08x} activate={f2:08x}')
    print(dis(f1 & ~1, 22))
    print('  -- activate')
    print(dis(f2 & ~1, 40))
