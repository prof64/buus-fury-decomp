"""Scan Thumb code for read-modify-write of record flags: ldr rA,[rB,#OFF]; ... orrs ...; str rA,[rB,#OFF]  (OFF in argv, default 0x20)."""
from common import *
import sys
from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB
md=Cs(CS_ARCH_ARM, CS_MODE_THUMB)
OFF=sys.argv[1] if len(sys.argv)>1 else '0x20'
def at(off):
    for i in md.disasm(ROM[off:off+4],0x08000000+off,count=1): return i
    return None
seq=[at(o) for o in range(0,0x30000,2)]
for k,i in enumerate(seq):
    if i and i.mnemonic=='str' and f'#{OFF}]' in i.op_str:
        win=[w for w in seq[max(0,k-12):k:1] if w]
        if any(w.mnemonic in('orrs','bics','ands') for w in win) and any(w.mnemonic=='ldr' and f'#{OFF}]' in w.op_str for w in win):
            print(hex(i.address))
