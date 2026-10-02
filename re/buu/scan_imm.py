"""scan_imm.py imm1 [imm2] [window] -- find thumb 'movs/adds/cmp #imm1' with #imm2 nearby (code region)"""
import sys
import capstone
from common import *
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.skipdata = True
a1 = int(sys.argv[1], 0)
a2 = int(sys.argv[2], 0) if len(sys.argv) > 2 else None
win = int(sys.argv[3]) if len(sys.argv) > 3 else 8
ins = list(md.disasm(rom()[:0x58000], 0x08000000))


def imm(i):
    if '#' in i.op_str and i.mnemonic in ('movs', 'adds', 'subs', 'cmp', 'mov'):
        try:
            return int(i.op_str.split('#')[-1], 0)
        except ValueError:
            return None
    return None


for k, i in enumerate(ins):
    if imm(i) == a1:
        if a2 is None or any(imm(j) == a2 for j in ins[max(0, k - win):k + win]):
            print(f'{i.address:08x}: {i.mnemonic} {i.op_str}')
