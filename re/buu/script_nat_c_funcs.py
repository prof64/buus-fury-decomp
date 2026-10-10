"""script_nat_c_funcs.py N addr... -- disassemble functions (first N instrs, stop at first return past all forward branches)."""
import sys
from common import *

n = int(sys.argv[1])
for s in sys.argv[2:]:
    a = int(s, 16) & ~1
    print(f'=== {a:08x}')
    furthest = 0
    for l in dis(a, n).split('\n'):
        print(l)
        try:
            ad = int(l.split(':')[0], 16)
        except ValueError:
            continue
        if ' b' in l and '#0x' in l and ' bl ' not in l:
            try:
                t = int(l.split('#0x')[-1].split()[0], 16)
                furthest = max(furthest, t)
            except ValueError:
                pass
        if (('pop' in l and 'pc' in l) or 'bx lr' in l) and ad >= furthest:
            break
