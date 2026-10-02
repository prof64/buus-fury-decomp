import sys
from collections import Counter
from common import *
from emutools import cur_pc
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
c = Counter()
for i in range(200000):
    c[cur_pc(e)] += 1
    e.step()
print('frame', e.frame)
for p, n in c.most_common(25):
    print(hex(p), n)
