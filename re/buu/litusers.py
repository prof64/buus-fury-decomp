"""litusers.py pool_addr [...] -- thumb `ldr rX, [pc, #imm]` instructions that load the literal at pool_addr."""
import sys
import struct
from common import *


def users(pool):
    out = []
    for a in range(max(0x08000000, pool - 1024), pool, 2):
        h = struct.unpack_from('<H', rom(), ra(a))[0]
        if (h & 0xF800) == 0x4800:
            lit = ((a + 4) & ~3) + (h & 0xFF) * 4
            if lit == pool:
                out.append(a)
    return out


if __name__ == '__main__':
    for s in sys.argv[1:]:
        p = int(s, 16)
        print(hex(p), hex(u32(p)), [hex(x) for x in users(p)])
