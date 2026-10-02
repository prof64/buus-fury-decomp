"""wtrace.py STATE START END MODE 'seq' [setup] [BPADDR] -- watch range while playing; print hits only in frames where BPADDR is hit (if given)"""
from common import *
import sys
KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'E':KEY.SELECT,'l':KEY.L,'r':KEY.R,'W':0}
e=new(sys.argv[1])
a=int(sys.argv[2],16); b=int(sys.argv[3],16); mode=sys.argv[4]
if len(sys.argv)>6 and sys.argv[6]!='-': exec(open(sys.argv[6]).read())
bpa=int(sys.argv[7],16) if len(sys.argv)>7 else None
toks=[]
for grp in sys.argv[5].split():
    rep=1
    if '*' in grp: grp,rep=grp.split('*'); rep=int(rep)
    for _ in range(rep): toks+=grp.split(',')
e.watch(a,b,mode)
if bpa: e.breakpoint(bpa)
for t in toks:
    k,n=t.split(':'); m=0
    for c in k: m|=KM[c]
    e.set_keys(m); e.run(int(n))
hs=e.hits()
frames=set(h['frame'] for h in hs if h['kind']=='break') if bpa else None
for h in hs:
    if frames is not None and h['frame'] not in frames: continue
    r=h['regs']
    if h['kind']=='break': print(f"f{h['frame']} BREAK pc={r[15]:08x} lr={r[14]:08x}"); continue
    print(f"f{h['frame']} {h['access']} a={h['addr']:08x} w{h['width']} {h['old']:x}->{h['new']:x} pc={r[15]:08x} lr={r[14]:08x} r0={r[0]:x} r1={r[1]:x} r2={r[2]:x} r3={r[3]:x}")
