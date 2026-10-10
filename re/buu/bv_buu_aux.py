"""On-hit reaction ("aux") trees.  State table entry = {root def, aux def}; enemy table +0x14 = default aux.
When an enemy is hit, the game runs word +4 (f1, 'onHit') of the aux def; composite defs' f1 iterate their
children's f1.  Defs used only as reactions may have create fn = 0.
This prints aux trees as [def] create/f1/f2 + words, recursing into words that look like defs.
usage: bv_buu_aux.py state|0xdef [...]"""
import sys
from common import *
from ai_decode import STATE_TABLE, REGISTRY

F1 = {0x0801900B: 'Sequence.onHit', 0x08018921: 'GotoState.onHit', 0x080188CB: 'Chance.onHit',
      0x08029A41: 'IfHitAttackByte(param)', 0x08019043: 'composite.f2'}


def looks_def(d):
    if not (0x086f0000 <= d < 0x08800000) or d & 3:
        return False
    w0, w1 = u32(d), u32(d + 4)
    ok = lambda w: w == 0 or (0x08000000 <= w < 0x08060000 and w & 1)
    return ok(w0) and ok(w1) and (w0 or w1)


def show(d, ind=0, seen=None, maxw=8):
    seen = set() if seen is None else seen
    if d in seen or ind > 12:
        return
    seen.add(d)
    c, f1, f2 = u32(d), u32(d + 4), u32(d + 8)
    name = REGISTRY.get(c, {}).get('name', f'{c:#x}' if c else 'NOCREATE')
    words = [u32(d + 4 * k) for k in range(3, maxw)]
    print('  ' * ind + f'@{d:#x} {name} f1={F1.get(f1, hex(f1))} f2={F1.get(f2, hex(f2))} w=' +
          ' '.join(hex(w) for w in words))
    n = u32(d + 0xC) if c in (0x08018F91, 0x080191C7, 0x080191E9, 0x0801920F) else None
    kids = [u32(d + 0x10 + 4 * k) for k in range(n)] if n is not None and n < 20 else [w for w in words if looks_def(w)]
    for k in kids:
        if looks_def(k):
            show(k, ind + 1, seen)


for a in sys.argv[1:]:
    v = int(a, 16)
    if v < 0x1000:
        print(f'## state {v:#x} aux {u32(STATE_TABLE + 8 * v + 4):#x}')
        show(u32(STATE_TABLE + 8 * v + 4))
    else:
        show(v)
