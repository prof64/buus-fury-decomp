"""Measure boss/enemy behaviour in the force-spawn arena (bs_arena) and write work/log2/boss_measure.json.
For each (class, gfx) and start distance (player stands still after initial placement; Lv40 party, active char as in
the debug Map Test state, godmode HP): hits on the player (frame, damage), projectiles (kind, life, speeds), state trace."""
from bs_watch import watch
import json, math
OUT='/Users/prof/source/buus-fury-decomp/re/log2/bs_measure_results.json'
RUNS=[('android17',3,9),('android18',4,11),('android19',5,12),('android20',6,14),('android20_b',7,14),('cooler',29,34),
      ('cell_imperfect',60,28),('cell_nonperfect',76,29),('perfect_cell_78',78,30),('perfect_cell_21',21,30),('perfect_cell_25',25,30),
      ('cell_jr',26,31),('general_tao',44,50),('triceratops_king',101,127),('yamcha',106,156),('krillin',64,76),('tien',98,121),
      ('destroyer',31,37),('frieza_dream_41',41,48),('newman_74',74,146)]
res={}
for name,cls,gfx in RUNS:
    for dist in (30,100):
        r=watch(cls,gfx,dist,900,verbose=False)
        if not r: continue
        hits=r['hits']; iv=[hits[i+1][0]-hits[i][0] for i in range(len(hits)-1)]
        pr={}
        for p in r['proj']:
            tr=p['traj']; sp=[round(math.hypot(tr[i+1][0]-tr[i][0],tr[i+1][1]-tr[i][1]),3) for i in range(len(tr)-1)]
            d=pr.setdefault(p['kind'],dict(count=0,lives=[],speeds=set()))
            d['count']+=1; d['lives'].append(p['life']); d['speeds'].update(x for x in sp if x>0)
        for k in pr: pr[k]['speeds']=sorted(pr[k]['speeds'])[:6]
        st=sorted(set(s for f,t in r['states'] for s in t))
        res[f'{name}@{dist}']=dict(cls=cls,gfx=gfx,start_dist=dist,frames=900,hits=len(hits),damage=sorted(set(-d for f,d in hits)),
             mean_interval=round(sum(iv)/len(iv),1) if iv else None,projectiles=pr,states_seen=st)
        print(name,dist,res[f'{name}@{dist}'])
json.dump(res,open(OUT,'w'),indent=1)
