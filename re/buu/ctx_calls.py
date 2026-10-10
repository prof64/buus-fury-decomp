"""ctx_calls.py target [back] -- for every BL to target, print the `back` instructions before it (literal values shown)."""
import sys
from common import *
from blxref import bl_targets

t = int(sys.argv[1], 16) & ~1
back = int(sys.argv[2]) if len(sys.argv) > 2 else 24
src, tgt = bl_targets()
for s in src[tgt == t]:
    s = int(s)
    print(f'===== call at {s:08x}')
    txt = dis(s - back * 2, back + 2)
    print('\n'.join(l for l in txt.split('\n') if 'ldr' in l and '=' in l or 'movs' in l or 'bl ' in l or 'str' in l and 'sp' in l))
