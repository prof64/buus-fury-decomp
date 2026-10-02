from common import *
import numpy as np, sys
e=new(sys.argv[1] if len(sys.argv)>1 else 'log2_prologue_trunks.ss')
def snap(): return np.frombuffer(e.ewram()+e.iwram(),dtype=np.uint8).copy()
seq=[snap()]
for k in [KEY.RIGHT,KEY.RIGHT,KEY.LEFT,KEY.LEFT]:
    e.hold(k,12); seq.append(snap())
for dt,name in ((np.int16,'16'),(np.int32,'32')):
    A=[s.view(dt).astype(np.int64) for s in seq]
    d=[A[i+1]-A[i] for i in range(4)]
    ok=(d[0]>0)&(d[1]>0)&(d[2]<0)&(d[3]<0)
    sz=int(name)//8
    for i in np.nonzero(ok)[0][:40]:
        o=i*sz
        addr=0x02000000+o if o<0x40000 else 0x03000000+o-0x40000
        print(name,hex(addr),[int(a[i]) for a in A])
