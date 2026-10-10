"""Verify damage formula (0x0800f910) against emulator, hit by hit.
usage: verify_dmg.py STATE NHITS [setup.py] [keys: A|B]"""
from common import *
from formula import RNG, damage
from runto import run_to
import sys
e=new(sys.argv[1]); N=int(sys.argv[2])
if len(sys.argv)>3 and sys.argv[3]!='-': exec(open(sys.argv[3]).read())
btn=KEY.B if len(sys.argv)>4 and sys.argv[4]=='B' else KEY.A
def keys(f):
    return btn if (f%24)<2 else 0
ok=0
for i in range(N):
    if not run_to(e,0x0800f910,600,keys): print('no more hits'); break
    this=e.reg(0); msg=e.reg(1)
    A=e.r32(msg+8); att=e.r32(msg+0xc); scale=e.r32(msg+4)
    atype=e.r8(A); power=e.r32(A+4)
    S=e.r8(att+0x160+(0 if atype==0 else 1))
    dd=e.r8(this+0x162); dm=e.r8(this+0x163+(0 if atype==0 else 1))
    rng=RNG(e.r32(0x3003890),e.r32(0x3003894))
    raw,pred,crit=damage(atype,power,S,dd,dm,scale,rng)
    # run to function end
    run_to(e,0x0800f988,5)
    got=e.r32(msg+0x18)
    good=(got==pred and e.r32(msg+0x14)==raw)
    ok+=good
    print(f"hit{i}: A={A:#x} type={atype} id={e.r8(A+1)} power={power} S={S} def={dd} mult={dm} scale={scale} crit={crit} -> pred {raw}/{pred} got {e.r32(msg+0x14)}/{got} {'OK' if good else 'MISMATCH'}")
print(f'{ok}/{i+1} matched')
