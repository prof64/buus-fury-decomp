"""Dump player state table (ROM 0x083b5278: pointers to IWRAM state singletons) and their relative vtables."""
from common import *
import sys
e=new(sys.argv[1] if len(sys.argv)>1 else 'log2_map12b.ss')
T=0x083b5278
for s in range(0x60):
    p=u32(T+4*s)
    if p>>24!=3: print('end at',hex(s),hex(p)); break
    vt=e.r32(p)
    meths=[hex((vt+u32(vt+4*k))&0xffffffff) for k in range(6)] if vt>>24==8 else []
    extra=[hex(e.r32(p+4*k)) for k in range(1,4)]
    print(hex(s), hex(p), 'vt',hex(vt), meths, extra)
