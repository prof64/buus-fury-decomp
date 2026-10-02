"""For each skill activate fn, find state vtables it installs, and dump vt entries [0..3]."""
import capstone
from common import *
from strtab import S
from vtab import entries
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
SK = 0x0871DAF8


def lits(fn, n=60):
    out = []
    code = rom()[ra(fn & ~1):ra(fn & ~1) + n * 2]
    for ins in md.disasm(code, fn & ~1):
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
            off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
            out.append(u32(((ins.address + 4) & ~3) + off))
        if ins.mnemonic == 'pop' and 'pc' in ins.op_str:
            break
    return out


for i in range(1, 0x18):
    a = SK + 24 * i
    f2 = u32(a + 0x14)
    L = lits(f2)
    vts = [x for x in L if 0x08049000 <= x < 0x0804f000]
    print(i, S(u16(a + 12)), 'act', hex(f2), 'vts', [hex(v) for v in vts],
          [[hex(x) for x in entries(v, 4)] for v in vts])
