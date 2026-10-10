"""Find code that compares an equipment slot (record +0x3c/+0x40/+0x44/+0x48) against an item id."""
import capstone
from common import *
from strtab import S
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.skipdata = True
code = rom()[:0x58000]
ins = list(md.disasm(code, 0x08000000))
names = {}
T = 0x086f82ec
for i in range(162):
    names[i] = S(u16(T + 0x2c * i))
slot = {0x3c: 'body', 0x40: 'hands', 0x44: 'feet', 0x48: 'acc'}
for k, a in enumerate(ins):
    if a.mnemonic == 'ldr' and any(f'#0x{o:x}]' in a.op_str for o in slot):
        reg = a.op_str.split(',')[0]
        off = int(a.op_str.split('#')[1].rstrip(']'), 16)
        for b in ins[k + 1:k + 5]:
            if b.mnemonic == 'cmp' and b.op_str.startswith(reg + ',') and '#' in b.op_str:
                v = int(b.op_str.split('#')[1], 16)
                print(f'{a.address:08x} {slot[off]:5} == {v} ({names.get(v)})  cmp@{b.address:08x}')
