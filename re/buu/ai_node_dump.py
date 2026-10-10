"""ai_node_dump.py create_fn [create_fn...] -- full disassembly of a behaviour node type (create + vtable fns)."""
import sys
from common import *
from vtab import entries
from ai_vts import create_vtable

COMMON = {0x080342a9: 'cond-decorator enter', 0x080342af: 'cond-decorator update (runs child while vt[5] true)',
          0x08034307: 'cond-decorator exit', 0x0804e501: 'nop', 0x0804f571: 'nop', 0x0804feb5: 'return 0'}


def full(f, n=90):
    """Disassemble until the next function prologue (push {...lr}) that follows a return."""
    out = []
    seen_ret = False
    for i, l in enumerate(dis(f & ~1, n).split('\n')):
        if i > 0 and seen_ret and 'push' in l and 'lr' in l:
            break
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            seen_ret = True
    return '\n'.join(out)


for s in sys.argv[1:]:
    f = int(s, 16)
    print(f'################ node create {f:08x}')
    print(full(f, 40))
    vt = create_vtable(f)
    print('vtable', vt and hex(vt))
    if vt:
        for slot, fn in enumerate(entries(vt, 8)):
            if fn in COMMON:
                print(f'-- vt[{slot}] {fn:08x} = {COMMON[fn]}')
                continue
            if slot in (0, 1, 2, 5):
                print(f'-- vt[{slot}] {fn:08x}')
                print(full(fn, 120))
