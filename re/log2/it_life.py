from common import *
from ents import enemies
from runto import run_to
e=new('log2_map12b.ss')
P=e.r32(0x3001fa8)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0 and e.r32(b)==0x80252a4]
T=en[0]; e.w32(T+0x188,1)
e.w32(P+0x158, e.r32(T+0x158)-20*256); e.w32(P+0x15c, e.r32(T+0x15c))
def keys(f): return KEY.RIGHT if f%20==0 else (KEY.A if f%20<3 else 0)
assert run_to(e,0x0801184a,800,keys)   # pickup ctor, after base init
obj=e.reg(4); f0=e.frame
print('pickup obj',hex(obj),'item',e.reg(6))
e.run(5)
# move player away
e.w32(P+0x158, e.r32(P+0x158)+0*256); e.w32(P+0x15c, e.r32(P+0x15c)+400*256)
alive=True; n=0
while n<3000:
    e.run(10); n+=10
    if e.r32(obj)!=0x8024c90:
        print('pickup gone after',e.frame-f0,'frames; vt now',hex(e.r32(obj))); break
else: print('still alive after 3000')
