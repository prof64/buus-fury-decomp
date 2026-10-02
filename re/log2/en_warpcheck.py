from common import *
import sys
sys.argv=[sys.argv[0]]+sys.argv[1:]
from warp import warp
from ents import enemies
from en_verify_aggro import stack, center
mi=int(sys.argv[1]); var=int(sys.argv[2]) if len(sys.argv)>2 else 0
e=warp(mi,var,240)
P=e.r32(0x3001fa8)
print('player',hex(P),center(e,P) if P else None)
for b,c in enemies(e):
    print(hex(b),'class',c,'hp',e.r32(b+0x188),'pos',center(e,b),'stack',stack(e,b))
e.screenshot(OUT+'warp.png')
if len(sys.argv)>3: e.save_file(ST+sys.argv[3])
