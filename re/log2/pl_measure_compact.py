"""Compact work/log2/pl_meas_*.json (from pl_measure_all.py) into re/log2/pl_measurements.json.
All rects/positions are made RELATIVE to the player's integer position at that frame (player faces `facing`)."""
from common import *
import json, glob, os
COSMETIC={'0x80245c0','0x80255a4','0x8025a18','0x80240fc','0x8023f24','0x8025808','0x8024acc','0x80259f0'}  # last two = beam controllers (no pos/rect at +0x20/+0x28)
out={}
for fn in sorted(glob.glob(OUT+'pl_meas_*.json')):
    d=json.load(open(fn)); name=d['name']; fr=d['frames']
    px,py=d['player_start']
    st=[(f['f'],f['state']) for f in fr]
    states=[st[0]]+[st[i] for i in range(1,len(st)) if st[i][1]!=st[i-1][1]]
    vul=[(f['f'],f['mult'],f['vuln']) for f in fr]
    vuln=[vul[0]]+[vul[i] for i in range(1,len(vul)) if vul[i][1:]!=vul[i-1][1:]]
    ep=[(f['f'],f['ep']) for i,f in enumerate(fr) if i and f['ep']!=fr[i-1]['ep']]
    objs={}
    for f in fr:
        ppx,ppy=f['ppos']
        for o in f['objs']:
            if o['vt'] in COSMETIC: continue
            k=o['o']+o['vt']
            r=objs.setdefault(k,dict(vt=o['vt'],first_frame=f['f'],traj=[],rects=[]))
            r['traj'].append([round(o['pos'][0]-ppx,3),round(o['pos'][1]-ppy,3)])
            r['rects'].append([o['rect'][0]-int(ppx),o['rect'][1]-int(ppy),o['rect'][2]-int(ppx),o['rect'][3]-int(ppy)])
    projs=[]
    for k,r in objs.items():
        t=r['traj']; moving=[i for i in range(1,len(t)) if t[i]!=t[i-1]]
        vel=[round(t[1][0]-t[0][0],3),round(t[1][1]-t[0][1],3)] if len(t)>1 else None
        vel_steady=[round(t[min(len(t)-1,6)][0]-t[min(len(t)-1,6)-1][0],3),round(t[min(len(t)-1,6)][1]-t[min(len(t)-1,6)-1][1],3)] if len(t)>2 else None
        sizes=sorted(set((rc[2]-rc[0],rc[3]-rc[1]) for rc in r['rects']))
        projs.append(dict(vtable=r['vt'],spawn_frame=r['first_frame'],frames_alive=len(t),frames_moving=(moving[-1] if moving else 0),
                          spawn_pos_rel=t[0],end_pos_rel=t[-1],first_step=vel,steady_step=vel_steady,
                          rect_sizes=[list(s) for s in sizes],spawn_rect_rel=r['rects'][0],last_rect_rel=r['rects'][-1]))
    hits=[]
    for h in d['hits']:
        if h.get('defender_idx') is None: continue
        hx,hy=h['player_pos']
        rc=h['rect']; big=rc[2]-rc[0]>=1000
        hits.append(dict(frame=h['frame'],enemy=h['defender_idx'],attack_id=h['tid'],power=h['power'],scale=h['scale'],dmg=h['pred'],crit=h['crit'],
                         rect_rel=(rc if big else [rc[0]-int(hx),rc[1]-int(hy),rc[2]-int(hx),rc[3]-int(hy)]),rect_size=[rc[2]-rc[0],rc[3]-rc[1]]))
    ens=[]
    for i in range(len(fr[0]['enemies'])):
        s=[(f['f'],f['enemies'][i][2]) for f in fr]
        ch=[s[j] for j in range(1,len(s)) if s[j][1]!=s[j-1][1]]
        pos=[(f['f'],f['enemies'][i][0],f['enemies'][i][1]) for f in fr]
        ens.append(dict(offset=d['enemy_offsets'][i],state_changes=ch[:12],
                        moved_px=[round(pos[-1][1]-pos[0][1],2),round(pos[-1][2]-pos[0][2],2)] if d['free'] else None))
    out[name]=dict(state_file=os.path.basename(d['state']),slot=d['slot'],input=d['seq'],facing=d['facing'],enemies_pinned=not d['free'],
                   player_states=states,player_mults_vuln=vuln,ep_changes=ep[:40],projectiles=projs,hits=hits,enemies=ens)
json.dump(out,open(ROOT+'re/log2/pl_measurements.json','w'),indent=0)
print(len(out),'runs ->',ROOT+'re/log2/pl_measurements.json')
