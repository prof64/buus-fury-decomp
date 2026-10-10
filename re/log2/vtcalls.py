"""Break on every slot of a vtable for N frames; report call counts and callers."""
from common import *
import sys
from collections import Counter
e=new(sys.argv[1]); vt=int(sys.argv[2],16); nslots=int(sys.argv[3]); fr=int(sys.argv[4])
slots={}
for k in range(nslots):
    t=(vt+u32(vt+4*k))&0xfffffffe
    if 0x08000000<=t<0x08030000 and t not in slots.values():
        slots[k]=t; e.breakpoint(t)
e.run(fr)
c=Counter()
for h in e.hits():
    pc=h['regs'][15]&~1
    for k,t in slots.items():
        if pc in (t,t+2,t+4): c[(k,hex(t),hex(h['regs'][14]),h['regs'][1] if k==9 else '')]+=1
for k,v in sorted(c.items()): print(k,v)
