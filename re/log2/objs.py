from common import *
from maps import rec, N, MAPS
from collections import Counter, defaultdict
fac=Counter(); ex=defaultdict(list)
for i in range(N):
    ids,f=rec(i)
    lst=f[3]; cnt=ids[4]
    if not lst: continue
    for k in range(cnt):
        p=u32(lst+4*k)
        if p>>24!=8: print('bad',i,k,hex(p)); continue
        w=struct.unpack_from('<8I',ROM,p&0x1ffffff)
        fac[w[0]]+=1
        ex[w[0]].append((i,ids[:2].hex(),hex(p),w[2],w[3]&0xffff,w[3]>>16,w[4],hex(w[5]),w[6]))
for k,v in fac.most_common():
    print(hex(k),v)
    for x in ex[k][:6]: print('   ',x)
