"""Verify enemy aggro radius & speeds by logging the AI state stack (entity+0x19c) frame by frame.
State log2_en_map13.ss: debug-warped map 13 (zone2 area7), enemies idle (wolf class17 state74, snake class53 state8).
The player is placed at distance d (center-to-center, pure axis offset) and stepped closer 1px/frame; we record the
distance at which the enemy's state changes, then hold the player still and measure the enemy's per-frame speed."""
from common import *
from ents import enemies
import math
ST_TBL=0x086d5194
def center(e,ent):
    x=e.r32(ent+0x158)/256; y=e.r32(ent+0x15c)/256
    return x+e.r8(ent+0x154)/2, y+e.r8(ent+0x155)/2
def stack(e,ent):
    c=ent+0x19c; n=e.r32(c+4)
    return [ (e.r32(c+8+8*i+4)-ST_TBL)//8 for i in range(min(n,5))]
def place(e,P,T,ox,oy):
    tx,ty=center(e,T); px,py=center(e,P)
    e.w32(P+0x158, e.r32(P+0x158)+int(round((tx+ox-px)*256))); e.w32(P+0x15c, e.r32(P+0x15c)+int(round((ty+oy-py)*256)))
def run(ent_addr, axis, start=110):
    e=new('log2_en_map13.ss')
    P=e.r32(0x3001fa8); T=ent_addr
    e.w32(T+0x188,9999)
    st0=stack(e,T); face0=e.r8(T+0x10)
    res=None; d=start
    while d>8:
        ox,oy={'L':(-d,0),'R':(d,0),'U':(0,-d),'D':(0,d)}[axis]
        place(e,P,T,ox,oy); e.run(1)
        tx,ty=center(e,T); px,py=center(e,P)
        st=stack(e,T)
        if st!=st0:
            res=(round(math.hypot(px-tx,py-ty),1),st,e.r8(T+0x10)); break
        d-=1
    steps=[]; c0=center(e,T)
    for f in range(30):
        e.run(1); c1=center(e,T); steps.append(round(math.hypot(c1[0]-c0[0],c1[1]-c0[1]),3)); c0=c1
    return face0,st0,res,stack(e,T),steps
if __name__=="__main__":
    for T,name in ((0x202ed68,'wolf c17'),(0x2037c4c,'wolf c17 #2'),(0x202cf68,'snake c53'),(0x202e168,'snake c53 #2')):
        for axis in "LRUD":
            face0,st0,res,st,steps=run(T,axis)
            print(f'{name} facing0={face0} state0={st0} approach from {axis}: transition at {res}; then stack={st} speeds={steps[:12]}')
