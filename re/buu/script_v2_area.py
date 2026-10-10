"""script_v2_area.py IDX... -- print an area record's object lists with conditions (for choosing test targets)."""
import sys
from common import *

for s in sys.argv[1:]:
    i = int(s)
    a = 0x0808E2E0 + 0x38 * i
    n0, n1, n2, n3 = u8(a + 3), u8(a + 4), u8(a + 5), u8(a + 6)
    print(f'area {i} @{a:08x} counts list0 {n0} actors {n1} breakables {n2} items {n3}')
    l0 = u32(a + 0x10)
    for k in range(n0):
        c, d = u32(l0 + 8 * k), u32(l0 + 8 * k + 4)
        print(f'  list0[{k}] cond {c:08x} def {d:08x} ctor {u32(d):08x}')
    l1 = u32(a + 0x14)
    for k in range(n1):
        d = u32(l1 + 4 * k)
        print(f'  actor[{k}] def {d:08x} ctor {u32(d):08x} cond {u32(d + 4):08x}')
    l2 = u32(a + 0x18)
    for k in range(n2):
        d = u32(l2 + 4 * k)
        print(f'  break[{k}] def {d:08x} cond {u32(d):08x}')
    l3 = u32(a + 0x1C)
    for k in range(n3):
        d = u32(l3 + 4 * k)
        print(f'  item[{k}] def {d:08x} cond {u32(d):08x} item {s32(d + 4)}')
