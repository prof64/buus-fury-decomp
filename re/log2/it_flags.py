from common import *
import sys
e=new(sys.argv[1]); seq=sys.argv[2]
KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'E':KEY.SELECT,'l':KEY.L,'r':KEY.R,'W':0}
for i in range(6):
    R=0x3000e9c+28*i
    e.watch(R+0x14,R+0x18,'r')
for t in seq.split():
    k,n=t.split(':'); m=0
    for c in k: m|=KM[c]
    e.set_keys(m); e.run(int(n))
seen={}
for h in e.hits():
    seen.setdefault(h['pc'],(h['addr'],h['lr'],0))
    a,l,c=seen[h['pc']]; seen[h['pc']]=(a,l,c+1)
for pc,(a,l,c) in sorted(seen.items()): print(hex(pc),'char',(a-0x3000eb0)//28,'lr',hex(l),'n',c)
