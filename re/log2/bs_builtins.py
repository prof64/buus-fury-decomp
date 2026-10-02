"""Catalog script-VM builtins (table 0x083b5d28; opcode 0x4a+i or '02 i'): first ~60 instructions, bl targets, literals."""
from en_lib import *
from en_dis import disr
import re, sys
T=0x083b5d28
lo=int(sys.argv[1],16) if len(sys.argv)>1 else 0; hi=int(sys.argv[2],16) if len(sys.argv)>2 else 0xa3
for i in range(lo,hi):
    f=u32(T+4*i)
    if not (f>>24==8 and f&1): continue
    txt=disr(f&~1,(f&~1)+0x70)
    lines=txt.split('\n')
    cut=[]
    for l in lines:
        cut.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l: break
    bls=re.findall(r'bl #(0x[0-9a-f]+)','\n'.join(cut))
    lits=re.findall(r'=(0x[0-9a-f]+)','\n'.join(cut))
    reads_pc=sum(1 for l in cut if re.search(r'ldrb r\d, \[r\d\]',l))
    print(f'b{i:#04x} op{0x4a+i:#04x} {f:#x} n={len(cut)} bl={bls} lit={lits}')
