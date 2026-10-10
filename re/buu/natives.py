"""natives.py lo hi [n] -- disassemble bytecode native functions (table 0x08055098)."""
import sys
from common import *
NAT = 0x08055098
lo, hi = int(sys.argv[1], 16), int(sys.argv[2], 16)
n = int(sys.argv[3]) if len(sys.argv) > 3 else 24
for i in range(lo, hi + 1):
    f = u32(NAT + 4 * i)
    print(f'--- native 0x{i:02x} -> {f:08x}')
    print(dis(f & ~1, n))
