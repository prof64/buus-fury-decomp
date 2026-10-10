"""xref.py ADDR [ADDR...]: find aligned 32-bit words in ROM equal to ADDR (literal pool refs),
and Thumb BL calls to ADDR."""
import sys, struct
from common import *

R = rom()


def lit_refs(target):
    out = []
    tb = struct.pack('<I', target)
    i = R.find(tb)
    while i != -1:
        if i % 4 == 0:
            out.append(0x08000000 + i)
        i = R.find(tb, i + 1)
    return out


def bl_refs(target, lo=0, hi=0x200000):
    out = []
    for i in range(lo, hi - 2, 2):
        h1 = R[i] | R[i + 1] << 8
        if h1 & 0xF800 != 0xF000:
            continue
        h2 = R[i + 2] | R[i + 3] << 8
        if h2 & 0xF800 not in (0xF800, 0xE800):
            continue
        off = ((h1 & 0x7FF) << 12) | ((h2 & 0x7FF) << 1)
        if off & 0x400000:
            off -= 0x800000
        dest = 0x08000000 + i + 4 + off
        if dest == target & ~1:
            out.append(0x08000000 + i)
    return out


if __name__ == '__main__':
    for a in sys.argv[1:]:
        t = int(a, 0)
        print(hex(t), 'lit:', ' '.join(hex(x) for x in lit_refs(t)), ' lit|1:', ' '.join(hex(x) for x in lit_refs(t | 1)) if not t & 1 else '')
        if t >= 0x08000000:
            print('   bl:', ' '.join(hex(x) for x in bl_refs(t)))
