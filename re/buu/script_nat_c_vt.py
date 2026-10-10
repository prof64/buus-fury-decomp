"""script_nat_c_vt.py vtable [entry ...] -- resolve self-relative vtable entries and disassemble them
(default: entry 9 = world-message handler, called as handler(obj, msg, arg))."""
import sys
from common import *

vt = int(sys.argv[1], 16)
ents = [int(x) for x in sys.argv[2:]] or [9]
print('entries', [hex((vt + s32(vt + 4 * i)) & 0xffffffff) for i in range(14)])
for k in ents:
    a = (vt + s32(vt + 4 * k)) & 0xffffffff
    print(f'=== entry {k} -> {a:08x}')
    furthest = 0
    for l in dis(a & ~1, 90).split('\n'):
        print(l)
        try:
            ad = int(l.split(':')[0], 16)
        except ValueError:
            continue
        if ' b' in l and '#0x' in l and ' bl ' not in l:
            try:
                furthest = max(furthest, int(l.split('#0x')[-1].split()[0], 16))
            except ValueError:
                pass
        if (('pop' in l and 'pc' in l) or 'bx lr' in l) and ad >= furthest:
            break
