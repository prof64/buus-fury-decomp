import sys
from common import *
for st in sys.argv[1:]:
    e = emu(st if (STATES / st).exists() else WORK / st)
    print(st, 'IE', hex(e.r16(0x04000200)), 'IF', hex(e.r16(0x04000202)), 'IME', hex(e.r16(0x04000208)),
          'DISPSTAT', hex(e.r16(0x04000004)), 'DISPCNT', hex(e.r16(0x04000000)), 'cpsr', hex(e.reg(16)), 'pc', hex(e.reg(15)),
          'sp', hex(e.reg(13)), 'lr', hex(e.reg(14)))
    sp = e.reg(13)
    print('  stack', [hex(e.r32(sp + 4 * i)) for i in range(24)])
