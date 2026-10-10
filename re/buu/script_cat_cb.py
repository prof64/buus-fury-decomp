"""script_cat_cb.py -- list {handler, arg} callback pairs: 0x0800421F (raw bytecode pointer) and
0x0800424F (event header -> run_event). Prints address of pair, arg, and who references the pair."""
import struct
from common import *

R = rom()
SCRIPTS = 0x0879B704
hdr_index = {}
for i in range(522):
    hdr_index[u32(SCRIPTS + 4 * i)] = i


def refs_to(addr):
    out = []
    pat = struct.pack('<I', addr)
    i = R.find(pat)
    while i >= 0:
        if i % 4 == 0:
            out.append(0x08000000 + i)
        i = R.find(pat, i + 1)
    return out


for h in (0x0800421F, 0x0800424F):
    pat = struct.pack('<I', h)
    i = R.find(pat)
    while i >= 0:
        if i % 4 == 0:
            a = 0x08000000 + i
            arg = u32(a + 4)
            extra = ''
            if h == 0x0800424F:
                extra = f'event #{hdr_index.get(arg, "?")}'
            rs = refs_to(a)
            print(f'{h:08x} pair@{a:08x} arg={arg:08x} {extra} refs={[hex(r) for r in rs[:4]]}')
        i = R.find(pat, i + 1)
