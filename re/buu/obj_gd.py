"""obj_gd.py [state] -- dump the persistent GameData block 0x03001068 (size 0x8A8)"""
import sys
from common import *
GD = 0x03001068


def dump(e):
    b = e.read(GD, 0x8A8)
    print('byte0 char', b[0], 'byte1 bits', bin(b[1]))
    sup = {i: b[2 + i] for i in range(83) if b[2 + i]}
    print('supplies', sup)
    flags = [i for i in range(143 * 8) if b[0x55 + (i >> 3)] >> (i & 7) & 1]
    print('flags', len(flags), flags[:80])
    n = int.from_bytes(b[0xE4:0xE8], 'little')
    eq = [int.from_bytes(b[0xE8 + 4 * i:0xEC + 4 * i], 'little') for i in range(n)]
    print('equip n', n, [hex(x) for x in eq])
    for off in range(0x4D0, 0x4F8, 4):
        print(hex(off), hex(int.from_bytes(b[off:off + 4], 'little')))
    print('framectr', hex(e.r32(0x03000F94)), hex(e.r32(0x03000F90)))
    for c in range(9):
        r = b[0x4F8 + 0x68 * c:0x4F8 + 0x68 * (c + 1)]
        print(c, r.hex())
    print('after recs', b[0x4F8 + 0x68 * 9:].hex())


if __name__ == '__main__':
    e = emu(sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss')
    e.run(1)
    dump(e)
