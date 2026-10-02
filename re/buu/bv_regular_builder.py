"""bv_regular_builder.py site [...] -- for a spawn site (call to 0x0801191A), locate the builder called just
before it, print the builder disassembly and its vtable's update (slot 10) / message (slot 9) functions."""
import sys
import capstone
from common import *
from vtab import entries

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)


def full(f, n=120):
    out, seen = [], False
    for i, l in enumerate(dis(f & ~1, n).split('\n')):
        if i > 0 and seen and 'push' in l and 'lr' in l:
            break
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            seen = True
    return '\n'.join(out)


def builder_of(site):
    a = site - 4
    for back in range(2, 40, 2):
        x = site - back
        ins = list(md.disasm(rom()[x - 0x08000000:x - 0x08000000 + 4], x))
        if ins and ins[0].mnemonic == 'bl':
            t = int(ins[0].op_str.lstrip('#'), 16)
            if t not in (0x0801191A, 0x080116FC):
                return t
    return None


for s in sys.argv[1:]:
    site = int(s, 0)
    b = builder_of(site)
    print(f'======== site {site:#x} builder {b and hex(b)}')
    if not b:
        continue
    txt = full(b, 140)
    print(txt)
    vts = [int(l.split('=')[1], 16) for l in txt.split('\n') if '; =0x0804' in l]
    for vt in vts[-1:]:
        e = entries(vt, 12)
        print('vtable', hex(vt), [hex(x) for x in e])
        print('--- slot10 (update)')
        print(full(e[10], 200))
        print('--- slot9 (message)')
        print(full(e[9], 80))
