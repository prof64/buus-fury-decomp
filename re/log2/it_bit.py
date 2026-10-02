from common import *
import re

lines=[]
from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB
md=Cs(CS_ARCH_ARM, CS_MODE_THUMB); md.skipdata=True
ins=list(md.disasm(ROM[0x200:0x28000],0x08000200))
for k in range(len(ins)-3):
    a=ins[k]
    if a.mnemonic=='ldr' and re.search(r'#0x(20|14)\]$',a.op_str):
        rd=a.op_str.split(',')[0]
        for b in ins[k+1:k+3]:
            if b.mnemonic=='lsls' and b.op_str.startswith(rd) and b.op_str.endswith(('#0x1e','#0x1f','#0x1d')):
                print(hex(a.address),a.op_str,'|',b.op_str)
