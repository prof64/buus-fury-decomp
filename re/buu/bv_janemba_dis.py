"""bv_janemba_dis.py [vt:ADDR | fn:ADDR[:N] | node:CREATEFN] ... -- disassembly helper.
vt:ADDR  -> list 8 self-relative vtable slots and disassemble slots 0,1,2,5 (100 lines each)
fn:ADDR[:N] -> disassemble N (default 120) lines
node:FN -> create fn + its runtime vtable (via ai_vts.create_vtable)"""
import sys
from common import *
from vtab import entries
from ai_vts import create_vtable

NOP = {0x0804e501, 0x0804f571, 0x0804feb5, 0x080342a9, 0x080342af, 0x08034307}


def show_vt(vt, n=100):
    es = entries(vt, 8)
    print(f'vtable {vt:08x}:', ' '.join(f'{x:08x}' for x in es))
    for slot in (0, 1, 2, 5):
        fn = es[slot]
        if fn in NOP or not (0x08000000 <= fn < 0x08800000):
            continue
        print(f'-- vt[{slot}] {fn:08x}')
        print(dis(fn & ~1, n))


for a in sys.argv[1:]:
    kind, _, rest = a.partition(':')
    parts = rest.split(':')
    addr = int(parts[0], 16)
    if kind == 'vt':
        show_vt(addr, int(parts[1]) if len(parts) > 1 else 100)
    elif kind == 'fn':
        print(f'######## fn {addr:08x}')
        print(dis(addr & ~1, int(parts[1]) if len(parts) > 1 else 120))
    elif kind == 'node':
        print(f'######## node {addr:08x}')
        print(dis(addr & ~1, 16))
        vt = create_vtable(addr)
        if vt:
            show_vt(vt, int(parts[1]) if len(parts) > 1 else 100)
