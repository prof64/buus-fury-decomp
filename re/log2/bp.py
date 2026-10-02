"""bp.py STATE ADDR[,ADDR..] 'seq' [setup.py-snippet] -- breakpoint hits with regs, while playing a key sequence (same syntax as play.py)"""
from common import *
import sys
KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'E':KEY.SELECT,'l':KEY.L,'r':KEY.R,'W':0}
e=new(sys.argv[1])
if len(sys.argv)>4: exec(open(sys.argv[4]).read())
for a in sys.argv[2].split(','): e.breakpoint(int(a,16))
toks=[]
for grp in sys.argv[3].split():
    rep=1
    if '*' in grp: grp,rep=grp.split('*'); rep=int(rep)
    for _ in range(rep): toks+=grp.split(',')
for t in toks:
    k,n=t.split(':'); m=0
    for c in k: m|=KM[c]
    e.set_keys(m); e.run(int(n))
hs=e.hits()
lim=int(os.environ.get('LIM','40')) if 'os' in dir() else 40
for h in hs[:60]:
    r=h['regs']
    print(f"f{h['frame']} pc={r[15]:08x} lr={r[14]:08x} sp={r[13]:08x} r0={r[0]:x} r1={r[1]:x} r2={r[2]:x} r3={r[3]:x} r4={r[4]:x} r5={r[5]:x} r6={r[6]:x} r7={r[7]:x}")
print(len(hs),'hits')
