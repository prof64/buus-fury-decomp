from common import *
from en_dis import disr
import sys
for a in sys.argv[1:]:
    n=int(a,16)
    ws=[u32(n+4*k) for k in range(8)]
    print('NODE',hex(n),' '.join(hex(w) for w in ws))
    for k in range(3):
        f=ws[k]
        if f and (f>>24)==8 and f&1:
            print(' fn%d'%k); print(disr(f&~1,(f&~1)+44))
