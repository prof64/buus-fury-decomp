"""List projectile constructors (callers of projectile base init 0x08013bc0) with their vtable and attack refs."""
from en_lib import *
from en_dis import disr
import re
ATK=0x086db2cc
calls=[0x0800e8d2,0x08011bec,0x080137ea,0x080139e0,0x08013e6c,0x080141d8,0x08015f34,0x080160e4,0x0801660e,0x0801a0d2,0x0801ccec,0x0801dd1c,0x0801eab4]
def fstart(a):
    # walk back to push
    p=a
    while p>a-0x200:
        h=u16(p)
        if (h&0xff00)==0xb500 or (h&0xfe00)==0xb400: return p
        p-=2
    return None
for c in calls:
    s=fstart(c)
    txt=disr(s, c+0x60)
    vts=re.findall(r'=0x(80[23][0-9a-f]{4})',txt)
    vt=[v for v in vts if 0x08023000<=int(v,16)<0x08026000]
    print(hex(s), 'vt', vt)
    for v in vt:
        v=int(v,16)
        fns=[(v+u32(v+4*k))&0xffffffff for k in range(12)]
        print('   slots', [hex(f) for f in fns])
