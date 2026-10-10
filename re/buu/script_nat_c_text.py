"""Decode kind-5/6 dialog-node text: blob+4 = u32 ref; bank = ref & 0x3F -> container at u32[0x087B5B64 + 4*bank]
(u32 type 0/1/2, u32 size, stream; 1/2 inflated by 0x08049120); start = ((ref >> 6) & 0x7FFF) * 2 bytes,
length = (ref >> 21) UTF-16 units. usage: script_nat_c_text.py [table_idx ...]"""
import struct
import sys
from common import *
BANKS = 0x087B5B64
_cache = {}
_e = None


def lz(addr):
    """Port of engine/crates/wf-rom/src/lz.rs (Webfoot LZ container at addr: u32 type, u32 size, stream)."""
    d = rom()
    off = ra(addr)
    typ, size = struct.unpack_from('<II', d, off)
    if typ == 0:
        return d[off + 8:off + 8 + size]
    st = {'p': off + 8, 'buf': 0, 'n': 0}

    def bit():
        if st['n'] == 0:
            st['buf'] = struct.unpack_from('<I', d, st['p'])[0]
            st['p'] += 4
            st['n'] = 32
        st['n'] -= 1
        return (st['buf'] >> st['n']) & 1

    def bits(k):
        v = 0
        for _ in range(k):
            v = (v << 1) | bit()
        return v

    def gamma():
        v = 1
        while True:
            v = ((v << 1) | bit()) & 0xFFFFFFFF
            if bit() == 0:
                return v

    out = bytearray()

    def back(dist, n):
        for _ in range(n):
            out.append(out[len(out) - dist])

    litbits, litoff, idxbits, last = typ, 0, 8, 1
    while True:
        if bit():
            out.append((bits(litbits) + litoff) & 0xFF)
            continue
        if bit():
            hi = gamma()
            if hi == 2:
                ln = gamma()
            else:
                last = bits(idxbits) + ((hi - 3) << idxbits)
                ln = gamma()
                if last >= 0x10000:
                    ln += 3
                elif last >= 0x37FF:
                    ln += 2
                elif last >= 0x27F:
                    ln += 1
                elif last <= 0x7F:
                    ln += 4
            back(last, ln)
            continue
        if bit() == 0:
            v = bits(7)
            if v:
                last = v
                back(last, bits(2) + 2)
                continue
            v = bits(2)
            if v == 0:
                break
            idxbits = bits(v + 3)
            continue
        v = bits(4) - 1
        if v == 0:
            out.append(0)
        elif v > 0:
            back(v, 1)
        elif bit():
            while True:
                for _ in range(256):
                    out.append(bits(8))
                if bit() == 0:
                    break
        else:
            litoff = 0
            litbits = 7 + bit()
            if litbits != 8:
                litoff = bits(8)
    return bytes(out)


def bank(b):
    if b not in _cache:
        _cache[b] = lz(u32(BANKS + 4 * b))
    return _cache[b]


def text(ref):
    b = ref & 0x3F
    off = ((ref >> 6) & 0x7FFF) * 2
    n = ref >> 21
    d = bank(b)
    s = []
    for i in range(n):
        c = struct.unpack_from('<H', d, off + 2 * i)[0]
        s.append(chr(c) if 0x20 <= c < 0x7f or c == 10 or 0xa0 <= c < 0x100 else '{%04x}' % c)
    return ''.join(s)


if __name__ == '__main__':
    from script_nat_c_graph import walk, TABLE
    for a in sys.argv[1:]:
        i = int(a)
        h = u32(TABLE + 4 * i)
        for s, w in sorted(walk(h).items(), key=lambda x: abs(x[0] + 0.5)):
            if w >= 0x10000 and u8(w + 2) in (5, 6):
                print(i, s, 'next', u16(w), 'kind', u8(w + 2), 'arg', u8(w + 3), repr(text(u32(w + 4)))[:150])
