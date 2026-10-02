"""ents.py STATE : list active entities (0x40-byte objects in pool at 0x02003840)."""
import sys
from common import *


def list_ents(e):
    n = e.r32(0x03000610)
    out = []
    for i in range(n):
        p = e.r32(0x03000614 + 4 * i)
        d = e.read(p, 0x40)
        out.append((p, d))
    return out


if __name__ == '__main__':
    e = emu(sys.argv[1])
    for p, d in list_ents(e):
        w = [int.from_bytes(d[i:i + 4], 'little') for i in range(0, 0x40, 4)]
        print(hex(p), 'xy=(%d,%d)' % (int.from_bytes(d[0x20:0x22], 'little', signed=True), int.from_bytes(d[0x22:0x24], 'little', signed=True)),
              'upd=%08x data=%08x' % (w[14], w[15]), d[:0x20].hex(' '), d[0x24:0x38].hex(' '))
