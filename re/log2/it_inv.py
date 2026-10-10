"""Verify inventory location (party+0x12b+id) and item effects by using items through the pause menu."""
from common import *
import sys
from PIL import Image
P=0x3000e90; INV=P+0x12b
def rec(e):
    R=P+0xc+28*e.r8(P)
    return R,dict(hp=e.r16(R),mhp=e.r16(R+2),ep=e.r16(R+4),mep=e.r16(R+6),s=e.r16(R+0xa),p=e.r16(R+0xc),en=e.r16(R+0xe))
def keyseq(e,seq):
    KM={'R':KEY.RIGHT,'L':KEY.LEFT,'U':KEY.UP,'D':KEY.DOWN,'A':KEY.A,'B':KEY.B,'S':KEY.START,'l':KEY.L,'r':KEY.R,'W':0}
    for t in seq.split():
        k,n=t.split(':'); m=0
        for c in k: m|=KM[c]
        e.set_keys(m); e.run(int(n))
    e.set_keys(0)
item=int(sys.argv[1]); setup=sys.argv[2] if len(sys.argv)>2 else ''
e=new('log2_map12b.ss')
R,_=rec(e)
for i in range(50): e.w8(INV+i,0)
e.w8(INV+item,2)
if 'hurt' in setup: e.w16(R,100); e.w16(R+4,5)
before=rec(e)[1]
keyseq(e,"W:10 S:3 W:60 r:6 W:60")
e.screenshot(OUT+'inv1.png')
keyseq(e,"A:3 W:30")
e.screenshot(OUT+'inv2.png')
keyseq(e,"A:3 W:40")
e.screenshot(OUT+'inv3.png')
keyseq(e,"A:3 W:40 B:3 W:30 B:3 W:30 B:3 W:30")
after=rec(e)[1]
print('item',item,'count before 2 after',e.r8(INV+item))
print('before',before); print('after ',after)
