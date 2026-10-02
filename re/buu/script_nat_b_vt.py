"""script_nat_b_vt.py -- for natives 0x58..0xA7 dump the command-object vtables they build (methods 1..4,
self-relative entries) to work/buu/script/nat_b_vtables.txt, plus disassembly of unique BL helpers."""
import json
from common import *
from vtab import entries

st = json.loads((WORK / 'script' / 'nat_b_stack.json').read_text())


def body(a, maxn=120):
    lines = dis(a & ~1, maxn).split('\n')
    res = []
    furthest = 0
    for l in lines:
        res.append(l)
        try:
            addr = int(l.split(':')[0], 16)
        except ValueError:
            continue
        if ' b' in l and '#0x' in l and ' bl ' not in l:
            try:
                t = int(l.split('#0x')[-1].split()[0], 16)
                if t > furthest and t - addr < 0x200:
                    furthest = t
            except ValueError:
                pass
        if (('pop' in l and 'pc' in l) or 'bx lr' in l) and addr >= furthest:
            break
    return res


out = []
done = set()
for k, v in st.items():
    for L in v['lits']:
        x = int(L, 16)
        if 0x08049000 <= x < 0x08050000 and x != 0x08054fbc and x not in done:
            done.add(x)
            out.append(f'##### vtable {x:08x} (native 0x{int(k):02x})')
            es = entries(x, 6)
            for i, f in enumerate(es):
                out.append(f'--- [{i}] {f:08x}')
                if i in (1, 2, 3, 4) and 0x08000000 <= f < 0x08800000:
                    out += body(f)
(WORK / 'script' / 'nat_b_vtables.txt').write_text('\n'.join(out) + '\n')
print(len(out), sorted(hex(x) for x in done))
