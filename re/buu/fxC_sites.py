"""fxC_sites.py target -- for every BL to target: enclosing function start (last `push {..., lr}` before it) and any
boss_nodes_* / notes mention of that function or of a node address within 0x200 before it."""
import sys
import re
import struct
from pathlib import Path
from common import *
from blxref import bl_targets

t = int(sys.argv[1], 16) & ~1
src, tgt = bl_targets()
texts = ''.join(p.read_text() for p in Path('.').glob('boss_nodes_*.py')) + (ROOT / 're/buu/notes.md').read_text()
names = {}
for m in re.finditer(r"0x([0-9A-Fa-f]{8}): dict\(\s*name='([^']+)'", texts):
    names[int(m.group(1), 16) & ~1] = m.group(2)
for s in src[tgt == t]:
    s = int(s)
    a = s
    while a > s - 0x800:
        h = struct.unpack_from('<H', rom(), ra(a))[0]
        if (h & 0xFF00) == 0xB500:
            break
        a -= 2
    near = [f'{k:08x}:{v}' for k, v in names.items() if a - 0x40 <= k <= s]
    print(f'call {s:08x} fn {a:08x}', near)
