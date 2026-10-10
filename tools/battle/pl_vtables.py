"""Print the vtable (enter/update/... methods) of each player state singleton (table 0x083B5278) from a save state."""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're/log2'))
from common import new, u32  # noqa: E402

e = new('log2_pl_goku.ss')
e.run(2)
want = [int(x, 16) for x in sys.argv[1:]] if len(sys.argv) > 1 else range(0x34)
for i in want:
    so = u32(0x83B5278 + 4 * i)
    vt = e.r32(so)
    meths = []
    for s in range(8):
        ent = u32(vt + 4 * s)
        meths.append(hex((vt + ent) & 0xFFFFFFFF))
    print(hex(i), 'obj', hex(so), 'vt', hex(vt), ' '.join(meths))
