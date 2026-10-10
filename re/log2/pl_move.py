"""pl_move.py STATE SLOT 'seq' [--enemy] [--frames N]
Select move slot (record+9), max EP, play key seq; per frame print player state, EP and new/changed world objects
(world list 0x03001120: +4 count, +8 ptrs). Objects shown: vtable, pos(+0x20,+0x24 >>8), vel(+0x40,+0x44)."""
from pl_common import *
from ents import enemies
import sys
args=sys.argv[1:]
e=new(args[0]); e.run(150); slot=int(args[1]); keys=parse(args[2])
R=rec(e)
e.w8(R+9,slot)
e.w16(R+4,999); e.w16(R+6,999)
P=player(e)
if '--enemy' in args:
    en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
    T=en[-1]
    # put enemy 48px right of player, high hp
    e.w32(T+0x158,e.r32(P+0x158)+48*256); e.w32(T+0x15c,e.r32(P+0x15c))
    e.w32(T+0x188,99999)
# face right
e.w8(P+0x10,3)
known=set()
def objs():
    n=e.r32(0x3001124); return [e.r32(0x3001128+4*i) for i in range(min(n,64))]
for o in objs(): known.add(o)
for i,k in enumerate(keys):
    e.set_keys(k); e.run(1)
    line=f'{i:3d} k={k:03x} st={pstate(e)} ep={e.r16(R+4)} px={e.r32(P+0x158)/256:.1f},{e.r32(P+0x15c)/256:.1f}'
    cur=objs()
    for o in cur:
        if o in known and '--all' not in args: continue
        vt=e.r32(o)
        line+=f' | {o:x} vt={vt:x} p=({e.r32(o+0x20)/256:.1f},{e.r32(o+0x24)/256:.1f}) v=({e.r32(o+0x40)-(1<<32) if e.r32(o+0x40)>>31 else e.r32(o+0x40)},{e.r32(o+0x44)-(1<<32) if e.r32(o+0x44)>>31 else e.r32(o+0x44)})'
    print(line)
    known=set(x for x in known if x in cur) if False else known
