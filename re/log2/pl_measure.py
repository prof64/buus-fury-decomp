"""Detailed per-attack measurement harness (player side).

pl_measure.py NAME STATE SLOT 'seq' FACING ENEMY_OFFSETS [--free]
  FACING: 0 down 1 up 2 left 3 right.  ENEMY_OFFSETS: 'dx,dy;dx,dy' relative to player (px); up to 3 enemies.
  --free: do not pin enemies in place each frame (used to measure knockback / hitstun).
Records per frame: player state, EP, player vulnerability bytes (+0x163/+0x164, +0x168), every new world
object with its rect (+0x28..+0x34), pos (+0x20/+0x24 24.8); and every damage calculation (0x0800F910)
with the hit message rect (msg base = r1-0x14, rect at base+4), attack id, scale, defender.
Writes work/log2/pl_meas_NAME.json and prints a summary.
"""
from pl_hits import *
import sys, json
a=sys.argv[1:]
name,st,slot,seq,facing,offs=a[0],a[1],int(a[2]),a[3],int(a[4]),a[5]
free='--free' in a
e=new(st); e.run(150)
R=rec(e); P=player(e)
e.w8(R+9,slot); e.w16(R+4,999); e.w16(R+6,999)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
tg=[]
px,py=e.r32(P+0x158),e.r32(P+0x15c)
pairs=[tuple(int(v) for v in s.split(',')) for s in offs.split(';') if s]
for b in en: e.w32(b+0x158,10*256); e.w32(b+0x15c,10*256)   # park all
for (dx,dy),b in zip(pairs,en):
    e.w32(b+0x188,999999); tg.append((b,px+dx*256,py+dy*256))
e.w8(P+0x10,facing)
if '--vuln' not in a: e.w8(P+0x163,0); e.w8(P+0x164,0)
def S32(v): return v-(1<<32) if v>>31 else v
known=set(e.r32(0x3001128+4*i) for i in range(e.r32(0x3001124)))
frames=[]; objs={}
def pin():
    if free: return
    for b,x,y in tg: e.w32(b+0x158,x); e.w32(b+0x15c,y)
for b,x,y in tg: e.w32(b+0x158,x); e.w32(b+0x15c,y)
def lg(f,e):
    pin()
    P=player(e)
    fr=dict(f=f,state=pstate(e),ep=e.r16(R+4),mult=[e.r8(P+0x163),e.r8(P+0x164)],vuln=e.r8(P+0x168),
            ppos=[e.r32(P+0x158)/256,e.r32(P+0x15c)/256],enemies=[[e.r32(b+0x158)/256,e.r32(b+0x15c)/256,hex(e.r32(b+0x18)),hex(e.r32(b+0x1c)),e.r32(b+0x188)] for b,_,_ in tg],objs=[])
    n=e.r32(0x3001124)
    for o in [e.r32(0x3001128+4*i) for i in range(min(n,80))]:
        if o in known: continue
        vt=e.r32(o)
        if vt in (0x080245c0,0x080255a4,0x08025a18,0x080240fc,0x08023f24): continue   # cosmetic sparks/debris/damage numbers
        rect=[S32(e.r32(o+0x28+4*k)) for k in range(4)]
        fr['objs'].append(dict(o=hex(o),vt=hex(vt),pos=[S32(e.r32(o+0x20))/256,S32(e.r32(o+0x24))/256],rect=rect))
        objs.setdefault(o,[f,vt])
    frames.append(fr)
keys=parse(seq)
hits=[]
for f,k in enumerate(keys):
    e.set_keys(k); s=e.save()
    bp=e.breakpoint(DMG); e.run(1); hs=e.hits(); e.clear(bp)
    if hs:
        e.load(s); e.set_keys(k); fr0=e.frame; t=hs[0]['regs'][15]
        while e.frame==fr0:
            e.step()
            if e.reg(15)==t:
                h=read_hit(e); base=e.reg(1)-0x14
                h['rect']=[S32(e.r32(base+4+4*i)) for i in range(4)]
                h['frame']=f; h['defender_idx']=next((i for i,(b,_,_) in enumerate(tg) if b==h['defender']),None)
                h['player_pos']=[e.r32(player(e)+0x158)/256,e.r32(player(e)+0x15c)/256]
                hits.append({k2:(hex(v) if k2 in ('defender','attacker','A') else v) for k2,v in h.items()})
                e.step()
    lg(f,e)
out=dict(name=name,state=st,slot=slot,seq=seq,facing=facing,enemy_offsets=pairs,free=free,
         player_start=[px/256,py/256],frames=frames,hits=hits)
json.dump(out,open(OUT+f'pl_meas_{name}.json','w'))
# summary
ss=[(fr['f'],fr['state']) for fr in frames]
chg=[ss[0]]+[ss[i] for i in range(1,len(ss)) if ss[i][1]!=ss[i-1][1]]
print(name,'states',chg)
for i,(b,_,_) in enumerate(tg):
    ec=[(fr['f'],fr['enemies'][i][2],fr['enemies'][i][3]) for fr in frames]
    ech=[ec[0]]+[ec[j] for j in range(1,len(ec)) if ec[j][1:]!=ec[j-1][1:]]
    pos=[(fr['f'],fr['enemies'][i][0],fr['enemies'][i][1]) for fr in frames]
    pch=[pos[j] for j in range(1,len(pos)) if pos[j][1:]!=pos[j-1][1:]]
    print(f' enemy{i} state/overlay changes',ech[:10]); 
    if free: print(f'  enemy{i} moves',pch[:6],'...',pch[-2:] if pch else '')
vul=[(fr['f'],fr['mult'],fr['vuln']) for fr in frames]
vchg=[vul[0]]+[vul[i] for i in range(1,len(vul)) if vul[i][1:]!=vul[i-1][1:]]
print(' vuln',vchg)
epc=[(fr['f'],fr['ep']) for i,fr in enumerate(frames) if i and fr['ep']!=frames[i-1]['ep']]
print(' ep',epc[:12],'...' if len(epc)>12 else '')
seen={}
for fr in frames:
    for ob in fr['objs']:
        r=seen.setdefault(ob['o']+ob['vt'],dict(first=fr['f'],last=fr['f'],p0=ob['pos'],r0=ob['rect'],p1=ob['pos'],r1=ob['rect'],vt=ob['vt'],traj=[]))
        r['last']=fr['f']; r['p1']=ob['pos']; r['r1']=ob['rect']; r['traj'].append(ob['pos'])
for k,r in seen.items():
    life=r['last']-r['first']+1
    t=r['traj']; v0=[round(t[1][0]-t[0][0],3),round(t[1][1]-t[0][1],3)] if len(t)>1 else None
    v5=[round(t[min(5,len(t)-1)][0]-t[min(4,len(t)-2)][0],3),round(t[min(5,len(t)-1)][1]-t[min(4,len(t)-2)][1],3)] if len(t)>2 else None
    moving=[i for i in range(1,len(t)) if t[i]!=t[i-1]]
    print(f"   v_first={v0} v_f5={v5} moving_frames={len(moving)} last_move_rel={moving[-1] if moving else None}")
    print(f" obj {r['vt']} f{r['first']}..f{r['last']} life={life} pos {r['p0']}->{r['p1']} v~({(r['p1'][0]-r['p0'][0])/max(1,life-1):.3f},{(r['p1'][1]-r['p0'][1])/max(1,life-1):.3f}) rect0 {r['r0']} rect1 {r['r1']}")
for h in hits:
    if h['defender_idx'] is None: continue
    print(f" hit f{h['frame']} def{h['defender_idx']} tid={h['tid']} pow={h['power']} scale={h['scale']} dmg={h['pred']} rect={h['rect']} ppos={h['player_pos']}")
