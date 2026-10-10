from common import *
from collections import Counter, defaultdict
T=0x086d5194
def isfn(v): return (v>>24)==8 and (v&1) and (v&0x1ffffff)<0x40000
def isdata(v): return (v>>24)==8 and (v&3)==0 and 0x6c0000<=(v&0x1ffffff)<0x720000
roots=[]
for i in range(152):
    roots+=list(struct.unpack_from('<2I',ROM,(T&0x1ffffff)+8*i))
seen={}; q=list(roots)
types=Counter(); ex=defaultdict(list)
while q:
    n=q.pop()
    if n in seen: continue
    if not isdata(n) or not isfn(u32(n)): continue
    seen[n]=1
    f=u32(n); types[f]+=1; ex[f].append(n)
    for k in range(1,16):
        w=u32(n+4*k)
        if isdata(w) and isfn(u32(w)): q.append(w)
for f,c in types.most_common():
    n=ex[f][0]
    print(hex(f),c,'eg',hex(n),' '.join(hex(u32(n+4*k)) for k in range(1,10)))
