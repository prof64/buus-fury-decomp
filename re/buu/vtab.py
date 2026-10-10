"""vtab.py vtable [n] [dis_n] -- list self-relative vtable entries (entry = table_base + s32) and disassemble each"""
import sys
from common import *


def entries(vt, n):
    out = []
    for i in range(n):
        off = s32(vt + 4 * i)
        f = (vt + off) & 0xffffffff
        out.append(f)
    return out


if __name__ == '__main__':
    vt = int(sys.argv[1], 16)
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 16
    dn = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    for i, f in enumerate(entries(vt, n)):
        print(f'[{i:2d}] +{4*i:#04x} -> {f:08x}')
        if dn and 0x08000000 <= f < 0x08800000:
            print(dis(f & ~1, dn))
