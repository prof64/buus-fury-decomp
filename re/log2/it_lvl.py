"""Verify level-up (0x08003ca0) prediction against emulator."""
from common import *
from formula import RNG
from runto import run_to
from ents import enemies
import sys
EXP=[u32(0x81d9834+4*i) for i in range(50)]
G=0x81d95f0
e=new('log2_map12b.ss')
P=0x3000e90; idx=e.r8(P); R=P+0xc+28*idx
e.w32(R+0x10, EXP[40]-10)   # 10 exp short of level 41
Pl=e.r32(0x3001fa8)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0 and e.r32(b)==0x80252a4]
T=en[0]; e.w32(T+0x188,1)
e.w32(Pl+0x158, e.r32(T+0x158)-20*256); e.w32(Pl+0x15c, e.r32(T+0x15c))
def keys(f): return KEY.RIGHT if f%20==0 else (KEY.A if f%20<3 else 0)
assert run_to(e,0x08003ca0,800,keys)
old=dict(lv_args=(e.reg(2),e.reg(3)),hp=e.r16(R),mhp=e.r16(R+2),ep=e.r16(R+4),mep=e.r16(R+6),
         s=e.r16(R+0xa),p=e.r16(R+0xc),en=e.r16(R+0xe))
rng=RNG(e.r32(0x3003890),e.r32(0x3003894))
oldL,newL=e.reg(2),e.reg(3); n=newL-oldL
pred={}
for k,off in (('s',0),('p',4),('en',8)):
    mn=u16(G+12*idx+off); mx=u16(G+12*idx+off+2)
    v=old[k]+rng.rand((mx-mn+1)*n)+mn*n
    v&=0xffff; pred[k]=min(v,0x6400)
mhp=old['mhp']; mep=old['mep']
for l in range(oldL,newL):
    r=rng.rand(0x51f)+0xe14
    mhp=mhp+((r*mhp)>>16)
    mep=mep+rng.rand(3)+2
pred.update(mhp=mhp,hp=mhp,mep=mep,ep=mep)
run_to(e,0x08003d96,5)
got=dict(hp=e.r16(R),mhp=e.r16(R+2),ep=e.r16(R+4),mep=e.r16(R+6),s=e.r16(R+0xa),p=e.r16(R+0xc),en=e.r16(R+0xe))
print('levels',oldL,'->',newL)
print('old ',old); print('pred',pred); print('got ',got)
print('MATCH' if all(pred[k]==got[k] for k in got) else 'MISMATCH')
e.save_file(ST+'log2_it_levelup.ss')
