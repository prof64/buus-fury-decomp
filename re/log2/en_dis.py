"""Robust thumb disassembler (skips undecodable halfwords). usage: en_dis.py START END"""
from common import *
from common import _ct
import sys
def disr(a,b):
    out=[]; pc=a
    while pc<b:
        off=pc&0x1ffffff
        got=False
        for i in _ct.disasm(ROM[off:off+4], pc):
            s=f'{i.address:08x}: {i.mnemonic} {i.op_str}'
            if i.mnemonic=='ldr' and 'pc' in i.op_str and '#' in i.op_str:
                try:
                    imm=int(i.op_str.split('#')[1].rstrip(']'),0)
                    la=((i.address+4)&~3)+imm; s+=f'   ; ={u32(la):#x}'
                except Exception: pass
            out.append(s); pc+=i.size; got=True
            break
        if not got:
            out.append(f'{pc:08x}: .hword {ROM[off]|ROM[off+1]<<8:#06x}'); pc+=2
    return '\n'.join(out)
if __name__=='__main__':
    print(disr(int(sys.argv[1],16),int(sys.argv[2],16)))
