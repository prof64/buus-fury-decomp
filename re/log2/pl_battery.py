"""pl_battery.py STATE SLOT 'seq' [dist] -- compact summary: state changes, EP changes, hits, projectile objects (new world objs)."""
from pl_hits import *
import sys, os
a=sys.argv[1:]
e=new(a[0]); e.run(150)
dist=int(a[3]) if len(a)>3 else 40
R,P,T=setup(e,int(a[1]),dist)
keys=parse(a[2])
known=set(e.r32(0x3001128+4*i) for i in range(e.r32(0x3001124)))
st=[None]; ep=[e.r16(R+4)]; objs={}
def lg(f,e):
    s=pstate(e)
    if s!=st[0]: print(f'f{f} state {s}'); st[0]=s
    v=e.r16(R+4)
    if v!=ep[0]: print(f'f{f} ep {ep[0]}->{v} ({v-ep[0]})'); ep[0]=v
    n=e.r32(0x3001124)
    cur=[e.r32(0x3001128+4*i) for i in range(min(n,80))]
    for o in cur:
        if o in known: continue
        vt=e.r32(o); x=e.r32(o+0x20); y=e.r32(o+0x24)
        if o not in objs:
            objs[o]=[f,vt,x,y,x,y,f]; print(f'f{f} NEW obj {o:x} vt={vt:x} pos=({x/256:.1f},{y/256:.1f}) player=({e.r32(P+0x158)/256:.1f},{e.r32(P+0x15c)/256:.1f})')
        else:
            objs[o][4:7]=[x,y,f]
    for o in list(objs):
        if o not in cur and objs[o][6]!=-1:
            r=objs[o]; print(f'f{f} GONE obj {o:x} vt={r[1]:x} life={r[6]-r[0]+1} start=({r[2]/256:.1f},{r[3]/256:.1f}) end=({r[4]/256:.1f},{r[5]/256:.1f})'); r[6]=-1; known.add(o)
hits=run(e,keys,lg)
from collections import OrderedDict
g=OrderedDict()
for h in hits:
    k=(h['defender'],h['tid'],h['atype'],h['power'],h['scale'])
    g.setdefault(k,[]).append(h)
for k,v in g.items():
    fr=[h['frame'] for h in v]
    print(f"HITS def={k[0]:x} tid={k[1]} type={k[2]} power={k[3]} scale={k[4]} n={len(v)} frames={fr[0]}..{fr[-1]} distinct_frames={len(set(fr))} dmg={[h['pred'] for h in v][:12]} crits={sum(h['crit'] for h in v)}")
QUIET=set(int(x,16) for x in os.environ.get('QVT','').split(',') if x)
for o,r in objs.items():
    if r[6]!=-1: print(f'ALIVE obj {o:x} vt={r[1]:x} from f{r[0]} start=({r[2]/256:.1f},{r[3]/256:.1f}) last=({r[4]/256:.1f},{r[5]/256:.1f}) at f{r[6]}')
