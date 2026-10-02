"""Player-side helpers (fork-owned)."""
from common import *
KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'E':KEY.SELECT,'l':KEY.L,'r':KEY.R,'W':0}
STATE_TBL=0x083b5278
PARTY=0x03000e90
def state_map():
    m={}
    for s in range(0x34):
        p=u32(STATE_TBL+4*s)
        m.setdefault(p,s)
    return m
SMAP=state_map()
def player(e): return e.r32(0x3001fa8)
def rec(e):
    idx=e.r8(PARTY); return PARTY+0xc+28*idx
def parse(seq):
    toks=[]
    for grp in seq.split():
        rep=1
        if '*' in grp: grp,rep=grp.split('*'); rep=int(rep)
        for _ in range(rep): toks+=grp.split(',')
    out=[]
    for t in toks:
        k,n=t.split(':'); m=0
        for c in k: m|=KM[c]
        out+= [m]*int(n)
    return out
def pstate(e):
    P=player(e); p=e.r32(P+0x18)
    return SMAP.get(p, hex(p))
