"""bv_regular_dis2.py site [...] -- disassemble the function containing a spawn site (from its prologue to
the site + 16 bytes)."""
import sys
from common import *

for s in sys.argv[1:]:
    a = int(s, 0)
    st = a
    while not (u16(st) & 0xff00 == 0xb500) or st == a:
        st -= 2
    n = (a - st) // 2 + 10
    print(f'===== site {a:#x} fn {st:#x}')
    print(dis(st, n))
