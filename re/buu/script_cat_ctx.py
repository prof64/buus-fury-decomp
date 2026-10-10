"""script_cat_ctx.py site [before] [after] -- disassemble the function containing a call site:
walk back to the nearest 'push {..lr}' (max `before` bytes), print until site+after."""
import sys
from common import *

site = int(sys.argv[1], 16)
before = int(sys.argv[2]) if len(sys.argv) > 2 else 0x100
after = int(sys.argv[3]) if len(sys.argv) > 3 else 0x30
start = site
R = rom()
for a in range(site, site - before, -2):
    h = R[ra(a)] | (R[ra(a) + 1] << 8)
    if (h & 0xFF00) == 0xB500:  # push {.., lr}
        start = a
        break
n = (site + after - start) // 2
for l in dis(start, n).split('\n'):
    print(l)
