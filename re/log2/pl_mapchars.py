from warp import warp
from pl_common import *
from maps import rec as mrec
import sys
for mi in [int(x) for x in sys.argv[1:]]:
    for var in (0,1):
        e=warp(mi,var,300)
        ids,_=mrec(mi)
        print(mi,var,ids[:2].hex(),'char',e.r8(PARTY),'form',e.r8(rec(e)+8),'player',hex(player(e)))
