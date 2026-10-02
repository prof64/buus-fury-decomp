"""Disassemble the owned AUTO nodes: create fn, runtime vtable entries (enter/update/exit/..5), and the
controller vtables they switch to.  usage: bv_others_dis.py [fn ...]  -> stdout"""
import sys
import json
from common import *
from vtab import entries
from ai_decode import runtime_vtable, auto, emu_ram

NODES = json.loads((WORK / 'boss_groups.json').read_text())['others']['auto_nodes']
fns = [int(a, 16) for a in sys.argv[1:]] or [int(a, 16) for a in NODES]


def fn_end_dis(a, n=120):
    """disassemble until a 'pop {..pc}' or 'bx lr' followed by literal pool (rough)."""
    txt = dis(a & ~1, n)
    out = []
    for line in txt.split('\n'):
        out.append(line)
        if ('pop {' in line and 'pc}' in line) or 'bx lr' in line:
            # keep going a bit if more code follows a branch-around; simple heuristic: stop at first return
            # whose next line is not obviously code
            pass
    return '\n'.join(out)


seen = set()


def show(a, label, n=100):
    a &= ~1
    if a in seen or not (0x08000000 <= a < 0x08800000):
        return
    seen.add(a)
    print(f'---- {label} {a:#x}')
    print(dis(a, n))


for fn in fns:
    print('#' * 100)
    a = auto(fn)
    print(hex(fn), json.dumps(a)[:900])
    show(fn, 'create', 60)
    vt, how, single = runtime_vtable(fn)
    if vt and 0x08000000 <= vt < 0x08800000:
        e = entries(vt, 6)
        print('vtable', hex(vt), [hex(x) for x in e])
        for i, x in enumerate(e[:6]):
            if i in (0, 1, 5):
                show(x, f'vt[{i}]', 110)
    for c in a.get('controllers', []):
        if c.get('vtable'):
            ce = entries(int(c['vtable'], 16), 3)
            print('controller', c['controller'], c['vtable'], [hex(x) for x in ce])
            show(ce[0], 'ctrl enter', 60)
            show(ce[1], 'ctrl update', 140)
