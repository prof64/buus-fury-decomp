"""Linear Thumb disassembly of the code region into work/log1_dis.txt (for grepping).
Functions are detected as starting at 'push {..., lr}'. BL targets annotated."""
import capstone, struct
from common import *

R = rom()
START, END = 0x08000000, 0x08000000 + 0x10400
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.skipdata = True
out = open(ROOT / 'work' / 'log1_dis.txt', 'w')
code = R[START - 0x08000000:END - 0x08000000]
for ins in md.disasm(code, START):
    extra = ''
    if ins.mnemonic == 'ldr' and 'pc' in ins.op_str and '#' in ins.op_str:
        try:
            imm = int(ins.op_str.split('#')[-1].rstrip(']'), 0)
            lit = ((ins.address + 4) & ~3) + imm
            extra = f' ; [{lit:08x}]={u32(lit):#010x}'
        except Exception:
            pass
    if ins.mnemonic == 'push' and 'lr' in ins.op_str:
        out.write(f'\n;---- func {ins.address:08x}\n')
    out.write(f'{ins.address:08x}: {ins.mnemonic} {ins.op_str}{extra}\n')
out.close()
