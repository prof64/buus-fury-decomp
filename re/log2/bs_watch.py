"""bs_watch.py CLASS GFX DIST FRAMES [axis] [quiet] -- force-spawn an enemy class (bs_arena) and log its AI states,
projectiles (spawn frame, per-frame speed, lifetime, rect) and player damage while the player stands still at DIST px
(re-placed every frame on the given axis relative to the enemy centre)."""
from bs_arena import *
import sys, math
def watch(cls,gfx,dist,N,axis='L',verbose=True,setup=None,ai_push=None,follow=False):
    e=arena(cls,gfx)
    T=boss(e,cls); P=e.r32(0x3001fa8)
    if not T: print('no entity'); return None
    godmode(e)
    if setup: setup(e,T,P)
    ox,oy={'L':(-dist,0),'R':(dist,0),'U':(0,-dist),'D':(0,dist)}[axis]
    prev=None; live={}; done=[]; hits=[]; states=[]
    hp0=e.r16(player_rec(e))
    for f in range(N):
        if dist is not None and (follow or f==0): place(e,P,T,ox,oy)
        e.run(1)
        st=tuple(stack(e,T)); hp=e.r16(player_rec(e))
        cur={p[0]:p for p in projectiles(e)}
        for a,p in cur.items():
            if a not in live: live[a]=dict(kind=p[1],spawn=f,traj=[(p[2],p[3])],rect=p[4])
            else: live[a]['traj'].append((p[2],p[3]))
        for a in list(live):
            if a not in cur:
                r=live.pop(a); r['life']=f-r['spawn']; done.append(r)
                if verbose:
                    tr=r['traj']; sp=[round(math.hypot(tr[i+1][0]-tr[i][0],tr[i+1][1]-tr[i][1]),3) for i in range(len(tr)-1)]
                    print(f"  proj {r['kind']} spawned f{r['spawn']} at ({tr[0][0]:.1f},{tr[0][1]:.1f}) rect0={r['rect']} life={r['life']}f speeds={sorted(set(sp))}")
        if hp!=hp0: hits.append((f,hp-hp0))
        if st!=prev or hp!=hp0:
            if verbose: print(f'f{f} states={st} playerHP={hp} (d{hp-hp0}) boss=({center(e,T)[0]:.1f},{center(e,T)[1]:.1f}) bossHP={e.r32(T+0x188)} playerAnim={hex(e.r32(P+0x18))}')
            states.append((f,st)); prev=st; hp0=hp
    return dict(e=e,T=T,P=P,proj=done,hits=hits,states=states)
if __name__=='__main__':
    cls=int(sys.argv[1]); gfx=int(sys.argv[2]); dist=int(sys.argv[3]); N=int(sys.argv[4]); axis=sys.argv[5] if len(sys.argv)>5 else 'L'
    r=watch(cls,gfx,dist,N,axis)
    if r: print('hits',r['hits'])
