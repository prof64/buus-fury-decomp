from collections import Counter
from common import *
from scriptdis import dis_script
T = 0x08711274
c = Counter()
for t in range(0x96):
    d = u32(T + 28 * t + 0x18)
    if not d:
        print(hex(t), 'no drop script')
        continue
    s = '; '.join(x for _, x in dis_script(d, 60))
    c[s.split(';')[0]] += 1
    print(hex(t), hex(d), s)
