"""pl_hits.py STATE SLOT 'seq' [--noenemy] -- run a move against an enemy placed 40px right; log every damage calc (0x0800f910):
attacker, defender, attack-table entry (type,id,power), msg scale (msg+4), crit, result; plus EP per frame summary."""
from pl_common import *
from ents import enemies
from formula import RNG, damage
import sys
DMG=0x0800f910
TBL=0x086db2cc
def setup(e, slot, dist=40):
    R=rec(e); P=player(e)
    e.w8(R+9,slot); e.w16(R+4,999); e.w16(R+6,999)
    en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
    T=en[-1]
    for b in en[:-1]:   # park other enemies far away
        e.w32(b+0x158,10*256); e.w32(b+0x15c,10*256)
    e.w32(T+0x158,e.r32(P+0x158)+dist*256); e.w32(T+0x15c,e.r32(P+0x15c))
    e.w32(T+0x188,999999)
    e.w8(P+0x10,3)
    # make player immune (0 damage -> no hurt state, see 0x08008806); form controllers may rewrite these on form change
    e.w8(P+0x163,0); e.w8(P+0x164,0)
    return R,P,T
def read_hit(e):
    this=e.reg(0); msg=e.reg(1)
    A=e.r32(msg+8); att=e.r32(msg+0xc); scale=e.r32(msg+4)
    atype=e.r8(A); power=e.r32(A+4)
    S=e.r8(att+0x160+(0 if atype==0 else 1))
    dd=e.r8(this+0x162); dm=e.r8(this+0x163+(0 if atype==0 else 1))
    rng=RNG(e.r32(0x3003890),e.r32(0x3003894))
    raw,pred,crit=damage(atype,power,S,dd,dm,scale,rng)
    return dict(defender=this,attacker=att,A=A,tid=(A-TBL)//8 if TBL<=A<TBL+0x80 else None,atype=atype,power=power,scale=scale,S=S,pred=pred,crit=crit)
def run(e, keys, log):
    out=[]
    for f,k in enumerate(keys):
        e.set_keys(k)
        s=e.save()
        bp=e.breakpoint(DMG); e.run(1); hs=e.hits(); e.clear(bp)
        if hs:
            e.load(s); e.set_keys(k)
            fr=e.frame; tgt=hs[0]['regs'][15]
            while e.frame==fr:
                e.step()
                if e.reg(15)==tgt:
                    h=read_hit(e); h['frame']=f; out.append(h)
                    if log=='v': print('  HIT',{x:(hex(y) if x in('defender','attacker','A') else y) for x,y in h.items()})
                    e.step()
        if callable(log): log(f,e)
    return out
if __name__=='__main__':
    a=sys.argv[1:]
    e=new(a[0]); e.run(150)
    R,P,T=setup(e,int(a[1]))
    keys=parse(a[2])
    def lg(f,e):
        print(f, 'st',pstate(e),'ep',e.r16(R+4),'enemyhp',e.r32(T+0x188))
    run(e,keys,lg)
