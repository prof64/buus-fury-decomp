"""For each AI node type: scan create fn + runtime vtable fns (+callees depth 2) for attack-table refs and projectile spawners."""
from common import *
from common import _ct
import re, json
ATK=0x086db2cc
def func_insns(a, maxn=400):
    out=[]; pc=a&~1; n=0
    while n<maxn:
        off=pc&0x1ffffff
        ins=list(_ct.disasm(ROM[off:off+4],pc,1))
        if not ins: pc+=2; n+=1; continue
        i=ins[0]; out.append(i); pc+=i.size; n+=1
        if i.mnemonic=='pop' and 'pc' in i.op_str:
            # continue only if a branch target beyond; simple heuristic: stop
            break
        if i.mnemonic=='bx' and i.op_str=='lr': break
    return out
def lits(insns):
    res=[];calls=[]
    for i in insns:
        if i.mnemonic=='ldr' and 'pc' in i.op_str and '#' in i.op_str:
            imm=int(i.op_str.split('#')[1].rstrip(']'),0)
            res.append(u32(((i.address+4)&~3)+imm))
        if i.mnemonic=='bl':
            calls.append(int(i.op_str.lstrip('#'),0))
    return res,calls
def vt_fns(v,n=4): return [(v+u32(v+4*k))&0xffffffff for k in range(n)]
def scan_fn(f, depth=2, seen=None):
    seen=seen if seen is not None else set()
    if f in seen or not (0x08000000<=f<0x08030000): return []
    seen.add(f)
    ins=func_insns(f); L,C=lits(ins); out=list(L)
    for v in L:
        if 0x08023000<=v<0x08026000:
            for g in vt_fns(v,4): out+=scan_fn(g,depth-1,seen) if depth>0 else []
    if depth>0:
        for c in C: out+=scan_fn(c,depth-1,seen)
    return out
def attack_ids(vals):
    ids=set()
    for v in vals:
        if ATK<=v<ATK+16*8 and (v-ATK)%8==0: ids.add((v-ATK)//8)
    return sorted(ids)
if __name__=='__main__':
    import sys
    types=[int(x,16) for x in sys.argv[1:]]
    for t in types:
        vals=scan_fn(t,3)
        print(hex(t),'attacks',attack_ids(vals),'fns',sorted(set(hex(v) for v in vals if 0x08013000<=v<0x08017000 or 0x86e0000<=v<0x86f0000))[:12])
