"""play.py IN_STATE OUT_STATE 'seq' -- seq tokens: KEYS:frames  (KEYS like R,L,U,D,A,B,S(start),E(select),l,r combos e.g. RA:10) or W:frames (wait).
Tokens may be repeated with *N e.g. A:3,W:20*10. Writes OUT png grid of a few frames."""
from common import *
import sys
from PIL import Image
KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'E':KEY.SELECT,'l':KEY.L,'r':KEY.R,'W':0}
src,dst,seq=sys.argv[1],sys.argv[2],sys.argv[3]
e=new(src)
toks=[]
for grp in seq.split():
    rep=1
    if '*' in grp: grp,rep=grp.split('*'); rep=int(rep)
    for _ in range(rep): toks+=grp.split(',')
ims=[]
total=sum(int(t.split(':')[1]) for t in toks)
every=max(1,total//7)
f=0;nxt=0
for t in toks:
    k,n=t.split(':'); n=int(n)
    m=0
    for c in k: m|=KM[c]
    e.set_keys(m)
    for i in range(n):
        e.run(1); f+=1
        if f>=nxt: ims.append(Image.frombytes('RGB',(240,160),e.pixels())); nxt+=every
e.set_keys(0)
ims=ims[-7:]+[Image.frombytes('RGB',(240,160),e.pixels())]
G=Image.new('RGB',(240*4,160*2))
for k,im in enumerate(ims): G.paste(im,((k%4)*240,(k//4)*160))
G.save(OUT+'grid.png')
if dst!='-': e.save_file(dst if '/' in dst else ST+dst)
