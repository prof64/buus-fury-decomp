from common import *
from tp3 import teleport
e = emu('buu_snakeway_start.ss')
b = e.breakpoint(0x0800fd54)
b2 = e.breakpoint(0x0800f68c)
teleport(e, 8, 3, 0)
e.run(120)
for h in e.hits()[:40]:
    r = h['regs']
    print(hex(h['pc']), ' '.join(f'r{i}={r[i]:08x}' for i in (0, 1, 2, 3, 13, 14)))
