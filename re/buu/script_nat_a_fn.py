"""script_nat_a_fn.py addr [addr...] [-n N] -- disassemble functions (following forward branches to the real
return) and annotate BL targets. Also: `-vt ADDR [count]` dumps a self-relative vtable's methods."""
import sys
from common import *


def fn(a, maxn=160):
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
                if t > furthest and t < addr + 0x400:
                    furthest = t
            except ValueError:
                pass
        if (('pop' in l and 'pc' in l) or 'bx lr' in l or ('bx r' in l and 'bx r' in l)) and addr >= furthest:
            break
    return res


def vtable(v, n=12):
    out = []
    for k in range(n):
        ent = v + 4 * k
        rel = s32(ent)
        out.append((k * 4, ent, (v + rel) & 0xFFFFFFFF))
    return out


args = sys.argv[1:]
n = 160
if '-n' in args:
    i = args.index('-n'); n = int(args[i + 1]); del args[i:i + 2]
if args and args[0] == '-vt':
    v = int(args[1], 16)
    cnt = int(args[2]) if len(args) > 2 else 12
    for off, ent, t in vtable(v, cnt):
        print(f'+{off:02x} @{ent:08x} -> {t:08x}')
    sys.exit()
for s in args:
    a = int(s, 16)
    print(f'=== {a & ~1:08x}')
    print('\n'.join(fn(a, n)))
