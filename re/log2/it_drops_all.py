from common import *
from maps import rec, N
from it_scr import drop_table, decode
from collections import Counter
fns=Counter(); tables=Counter(); ex={}
for i in range(N):
    ids,f=rec(i)
    lst=f[3]; cnt=ids[4]
    if not lst: continue
    for k in range(cnt):
        p=u32(lst+4*k)
        w=struct.unpack_from('<8I',ROM,p&0x1ffffff)
        if w[0]!=0x800e77f: continue
        s=w[5]
        fn=u32(s); scr=u32(s+4)
        fns[hex(fn)]+=1
        if fn==0x801069b:
            dt=drop_table(scr)
            key=str(dt); tables[key]+=1; ex.setdefault(key,(i,hex(scr),w[2]))
print(fns)
for k,v in tables.most_common(): print(v,ex[k],k)
