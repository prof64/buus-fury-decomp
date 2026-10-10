"""Verify the Android 19/20 energy-absorb on-hit logic (state 29 on-hit tree) in the emulator.
Method: in log2_map12b.ss (Piccolo Lv40, aggroed snake at 0x02037be4 in state 9/10 whose on-hit node is
ClassDefaultState -> states[class.ai_state].on_hit), repoint the snake's class row (entity+0x180) to class 5
(Android 19, ai_state 29). Then hit it with ki blasts (B) and melee (A) and log HP changes.
Expected: energy hits -> no damage, HP += raw_damage*0x3333>>16 (20%); melee hits -> normal damage."""
from common import *
from en_verify_aggro import stack, place
CT=0x086d56ac
def trial(btn, cls):
    e=new('log2_map12b.ss')
    P=e.r32(0x3001fa8); T=0x02037be4
    if cls is not None: e.w32(T+0x180, CT+24*cls)
    e.w32(T+0x188,500)
    place(e,P,T,-26,0)
    e.press(KEY.RIGHT,1,0)
    out=[]
    for k in range(6):
        hp0=e.r32(T+0x188)
        e.press(btn,2,28)
        out.append(e.r32(T+0x188)-hp0)
    return out, stack(e,T)
for cls,label in ((None,'snake (class 53, ai 8)'),(5,'snake repointed to class 5 (Android 19, ai 29)')):
    for btn,name in ((KEY.B,'ki blast'),(KEY.A,'melee')):
        d,st=trial(btn,cls)
        print(f'{label}: {name}: HP deltas {d} stack {st}')
