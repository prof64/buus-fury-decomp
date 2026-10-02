"""Measure hurt state (0x20): duration, knockback per frame, invulnerability (entity+0x163/0x164)."""
from pl_common import *
from ents import enemies
e=new('log2_pl_goku.ss'); e.run(150)
P=player(e); R=rec(e)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]; T=en[-1]
e.w32(T+0x158,e.r32(P+0x158)+20*256); e.w32(T+0x15c,e.r32(P+0x15c))
e.w8(P+0x10,3)  # face right (toward enemy)
prev=None
for f in range(400):
    e.run(1)
    s=pstate(e)
    if s==32 or prev==32:
        print(f,s,f'x={e.r32(P+0x158)/256:.2f} y={e.r32(P+0x15c)/256:.2f}','mult',e.r8(P+0x163),e.r8(P+0x164),'hp',e.r16(R),'facing',e.r8(P+0x10))
    prev=s
