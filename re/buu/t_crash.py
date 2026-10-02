from collections import deque
from common import *
from emutools import cur_pc
import tp3

e = emu('buu_snakeway_start.ss')
trace = deque(maxlen=80)


def stepping_run(n=1):
    start = e.frame
    while e.frame < start + n:
        p = cur_pc(e)
        cpsr = e.reg(16)
        trace.append((p, e.reg(14), e.reg(13), cpsr, [e.reg(i) for i in range(8)]))
        if (cpsr & 0x1f) == 0x1b:
            print('UNDEF; last pcs:')
            for t in list(trace)[-60:]:
                print('  ', hex(t[0]), 'lr', hex(t[1]), 'sp', hex(t[2]), 'cpsr', hex(t[3]), [hex(x) for x in t[4][:4]])
            raise SystemExit
        e.step()


e.run = stepping_run
tp3.teleport(e, 8, 3, 0x80, 0x80)
for i in range(300):
    stepping_run(1)
print('no crash; frame', e.frame)
