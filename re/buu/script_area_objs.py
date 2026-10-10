"""script_area_objs.py world map -- dump an area's object lists (area table 0x0808E2E0): per list the records
(first words) and, where a record word points at bytecode (condition / handler scripts), its disassembly."""
import sys
from common import *
from scriptdis import dis_script

AREAS, NAREA = 0x0808E2E0, 0x1C4
w, m = int(sys.argv[1]), int(sys.argv[2])
for i in range(NAREA):
    a = AREAS + 0x38 * i
    if u8(a) != w or u8(a + 1) != m:
        continue
    print(f'area #{i} @ {a:08x}: ' + rom()[ra(a):ra(a) + 0x38].hex())
    counts = [u8(a + 3 + k) for k in range(5)]
    for k in range(5):
        p = u32(a + 0x10 + 4 * k)
        print(f' list {k} count {counts[k]} @ {p:08x}')
        if not (0x08000000 <= p < 0x09000000):
            continue
        for j in range(counts[k]):
            r = u32(p + 4 * j)
            words = [u32(r + 4 * q) for q in range(8)] if 0x08000000 <= r < 0x09000000 else []
            print(f'   [{j}] rec {r:08x}: ' + ' '.join(f'{x:08x}' for x in words))
