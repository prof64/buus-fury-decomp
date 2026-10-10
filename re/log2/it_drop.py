from common import *
from ents import enemies
import sys
VT={0x8024c90:'pickup',0x80252e8:'mapitem',0x8023f18:'?'}
def scan(e):
    d=e.ewram(); out=[]
    for v in VT:
        p=struct.pack('<I',v); i=d.find(p)
        while i!=-1:
            if i%4==0: out.append((VT[v],hex(0x02000000+i),e.r32(0x02000000+i+0x40)))
            i=d.find(p,i+1)
    return out
e=new(sys.argv[1])
P=e.r32(0x3001fa8)
kills=0
for rnd in range(int(sys.argv[2])):
    en=[b for b,c in enemies(e) if e.r32(b+0x188)>0 and e.r32(b)==0x80252a4]
    if not en: break
    T=en[0]
    e.w32(T+0x188,1)
    for k in range(10):
        e.w32(P+0x158, e.r32(T+0x158)-20*256); e.w32(P+0x15c, e.r32(T+0x15c))
        e.press(KEY.RIGHT,1,0); e.press(KEY.A,2,10)
        if e.r32(T+0x188)==0: kills+=1; break
    e.run(30)
    print('round',rnd,'kills',kills,scan(e))
e.screenshot(OUT+'drop.png')
e.save_file(ST+'log2_it_drop.ss')
