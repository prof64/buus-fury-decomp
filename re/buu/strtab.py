"""String table: id -> UTF-16 string. Table of pointers at STRTAB."""
import struct
from common import rom, u32, ra

STRTAB = 0x0806bce8


def ustr(addr, maxlen=2000):
    r = rom()
    o = ra(addr)
    out = []
    for i in range(maxlen):
        c = struct.unpack_from('<H', r, o + 2 * i)[0]
        if c == 0:
            break
        if 0x20 <= c < 0x7f or c == 10:
            out.append(chr(c))
        elif c >= 0xa0 and c < 0x100:
            out.append(chr(c))
        else:
            out.append('{%04x}' % c)
    return ''.join(out)


def table():
    out = []
    i = 0
    while True:
        p = u32(STRTAB + 4 * i)
        if not (0x08000000 <= p < 0x08800000):
            break
        out.append(p)
        i += 1
    return out


_cache = None


def S(i):
    global _cache
    if _cache is None:
        _cache = [ustr(p) for p in table()]
    if 0 <= i < len(_cache):
        return _cache[i]
    return None


if __name__ == '__main__':
    import sys
    t = table()
    print(len(t), 'strings, ends at', hex(STRTAB + 4 * len(t)))
    for i, p in enumerate(t):
        s = ustr(p, 200)
        print(f'{i:4d} 0x{i:03x} {p:08x} {s[:100]!r}')
