from common import *
from ents import enemies
from runto import run_to
import sys
e=new(sys.argv[1])
P=e.r32(0x3001fa8)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0 and e.r32(b)==0x80252a4]
T=en[0]; e.w32(T+0x188,1)
e.w32(P+0x158, e.r32(T+0x158)-20*256); e.w32(P+0x15c, e.r32(T+0x15c))
def keys(f): return KEY.RIGHT if f%20==0 else (KEY.A if f%20<3 else 0)
if run_to(e,0x08011802,800,keys):
    sp=e.reg(13)
    print('r0-3',[hex(e.reg(i)) for i in range(4)],'lr',hex(e.reg(14)))
    print('stack',[hex(e.r32(sp+4*i)) for i in range(24)])
