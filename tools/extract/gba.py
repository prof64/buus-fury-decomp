"""Shared helpers for graphics extraction: ROM access, disassembly, GBA image codecs."""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GAMES = ('log1', 'log2', 'buu')
WORK = ROOT / 'work' / 'gfx'
ASSETS = ROOT / 'site' / 'public' / 'assets'
DATA = ROOT / 'data'

_roms = {}


def rom(game):
    if game not in _roms:
        _roms[game] = (ROOT / 'roms' / f'{game}.gba').read_bytes()
    return _roms[game]


class Rom:
    def __init__(self, game):
        self.game = game
        self.d = rom(game)

    def off(self, a):
        return a & 0x1FFFFFF

    def u8(self, a): return self.d[a & 0x1FFFFFF]
    def s8(self, a): return struct.unpack_from('<b', self.d, a & 0x1FFFFFF)[0]
    def u16(self, a): return struct.unpack_from('<H', self.d, a & 0x1FFFFFF)[0]
    def s16(self, a): return struct.unpack_from('<h', self.d, a & 0x1FFFFFF)[0]
    def u32(self, a): return struct.unpack_from('<I', self.d, a & 0x1FFFFFF)[0]
    def s32(self, a): return struct.unpack_from('<i', self.d, a & 0x1FFFFFF)[0]
    def bytes(self, a, n): return self.d[a & 0x1FFFFFF:(a & 0x1FFFFFF) + n]

    def isptr(self, v):
        return 0x08000000 <= v < 0x08000000 + len(self.d)

    def dis(self, addr, n=40, arm=False):
        from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_ARM
        md = Cs(CS_ARCH_ARM, CS_MODE_ARM if arm else CS_MODE_THUMB)
        off = addr & 0x1FFFFFF
        out = []
        for i in md.disasm(self.d[off:off + n * 4], addr):
            s = f'{i.address:08x}: {i.mnemonic:6s} {i.op_str}'
            if i.mnemonic == 'ldr' and '[pc' in i.op_str and '#' in i.op_str:
                try:
                    imm = int(i.op_str.split('#')[1].rstrip(']'), 0)
                    la = ((i.address + 4) & ~3) + imm if not arm else i.address + 8 + imm
                    s += f'   ; ={self.u32(la):#x}'
                except Exception:
                    pass
            out.append(s)
            if len(out) >= n:
                break
        return '\n'.join(out)


def bgr555(c):
    r = (c & 31) << 3
    g = ((c >> 5) & 31) << 3
    b = ((c >> 10) & 31) << 3
    return (r | r >> 5, g | g >> 5, b | b >> 5)


def palette(data, n=16):
    return [bgr555(struct.unpack_from('<H', data, i * 2)[0]) for i in range(n)]


def lz77(data, off=0):
    """GBA BIOS LZ77 (type 0x10). Returns (bytes, consumed)."""
    hdr = struct.unpack_from('<I', data, off)[0]
    assert hdr & 0xF0 == 0x10, hex(hdr)
    size = hdr >> 8
    out = bytearray()
    p = off + 4
    while len(out) < size:
        flags = data[p]; p += 1
        for b in range(8):
            if len(out) >= size:
                break
            if flags & (0x80 >> b):
                v = data[p] << 8 | data[p + 1]; p += 2
                ln = (v >> 12) + 3
                disp = (v & 0xFFF) + 1
                for _ in range(ln):
                    out.append(out[-disp])
            else:
                out.append(data[p]); p += 1
    return bytes(out[:size]), p - off


def tiles4(data, ntiles=None):
    """Decode 4bpp tiles to list of 8x8 index arrays (lists of 64)."""
    n = len(data) // 32 if ntiles is None else ntiles
    out = []
    for t in range(n):
        px = []
        for b in data[t * 32:t * 32 + 32]:
            px.append(b & 15); px.append(b >> 4)
        out.append(px)
    return out


class _Bits:
    """MSB-first bit reader over little-endian 32-bit words (Webfoot decompressor)."""

    def __init__(self, data, off):
        self.d = data
        self.p = off
        self.buf = 0
        self.n = 0

    def _word(self):
        v = struct.unpack_from('<I', self.d, self.p)[0]
        self.p += 4
        return v

    def bit(self):
        if self.n == 0:
            self.buf = self._word()
            self.n = 32
        self.n -= 1
        return (self.buf >> self.n) & 1

    def bits(self, k):
        v = 0
        for _ in range(k):
            v = (v << 1) | self.bit()
        return v

    def gamma(self):
        v = 1
        while True:
            v = (v << 1) | self.bit()
            if not self.bit():
                return v


def wf_decompress(data, off, size=None):
    """Webfoot JCALG1-style decompressor (IWRAM routine at 0x3000000 in all three games).

    Header at off: u32 type (0 raw, 1/2 compressed), u32 decompressed size. Stream follows.
    Returns (bytes, end_offset_of_stream)."""
    typ, usize = struct.unpack_from('<II', data, off)
    if typ == 0:
        return bytes(data[off + 8:off + 8 + usize]), off + 8 + usize
    assert typ in (1, 2), f'bad type {typ} at {off:#x}'
    B = _Bits(data, off + 8)
    out = bytearray()
    litbits = typ      # r3 at entry holds the type word
    litoff = 0         # r4 (garbage at entry; streams reset it first)
    idxbits = 8        # r5
    last = 1           # ip
    while True:
        if B.bit():
            out.append((B.bits(litbits) + litoff) & 0xFF)
            continue
        if B.bit():
            # normal match: gamma-coded high part
            hi = B.gamma()
            if hi == 2:
                ln = B.gamma()
                src = last
            else:
                hi -= 3
                last = B.bits(idxbits) + (hi << idxbits)
                ln = B.gamma()
                if last >= 0x10000:
                    ln += 3
                elif last >= 0x37FF:
                    ln += 2
                elif last >= 0x27F:
                    ln += 1
                elif last <= 0x7F:
                    ln += 4
                src = last
            for _ in range(ln):
                out.append(out[-src])
            continue
        if not B.bit():
            # short match
            v = B.bits(7)
            if v:
                last = v
                ln = B.bits(2) + 2
                for _ in range(ln):
                    out.append(out[-last])
                continue
            v = B.bits(2)
            if v == 0:
                break
            idxbits = B.bits(v + 3)
            continue
        v = B.bits(4) - 1
        if v == 0:
            out.append(0)
        elif v > 0:
            out.append(out[-v])
        else:
            if B.bit():
                while True:
                    for _ in range(256):
                        out.append(B.bits(8))
                    if not B.bit():
                        break
            else:
                litoff = 0
                litbits = 7 + B.bit()
                if litbits != 8:
                    litoff = B.bits(8)
    return bytes(out), B.p


def huffman(data, off=0):
    """GBA BIOS Huffman (type 0x2X, 4 or 8 bit symbols). Returns (bytes, consumed)."""
    hdr = struct.unpack_from('<I', data, off)[0]
    assert hdr & 0xF0 == 0x20, hex(hdr)
    bits = hdr & 0xF
    size = hdr >> 8
    tree_size = data[off + 4]
    tree = off + 5          # root node offset
    p = off + 4 + (tree_size + 1) * 2
    out = bytearray()
    acc = 0
    nacc = 0
    node = tree
    word = 0
    nbits = 0
    while len(out) < size:
        if nbits == 0:
            word = struct.unpack_from('<I', data, p)[0]
            p += 4
            nbits = 32
        bit = (word >> 31) & 1
        word = (word << 1) & 0xFFFFFFFF
        nbits -= 1
        nd = data[node]
        child = (node & ~1) + (nd & 0x3F) * 2 + 2 + bit
        is_leaf = nd & (0x40 if bit else 0x80)
        if is_leaf:
            sym = data[child]
            acc |= sym << nacc
            nacc += bits
            if nacc == 8:
                out.append(acc & 0xFF)
                acc = 0
                nacc = 0
            node = tree
        else:
            node = child
    return bytes(out[:size]), p - off


def rle(data, off=0):
    """GBA BIOS RLE (type 0x30). Returns (bytes, consumed)."""
    hdr = struct.unpack_from('<I', data, off)[0]
    assert hdr & 0xF0 == 0x30, hex(hdr)
    size = hdr >> 8
    p = off + 4
    out = bytearray()
    while len(out) < size:
        f = data[p]
        p += 1
        if f & 0x80:
            n = (f & 0x7F) + 3
            out += bytes([data[p]]) * n
            p += 1
        else:
            n = (f & 0x7F) + 1
            out += data[p:p + n]
            p += n
    return bytes(out[:size]), p - off


def bios_decompress(data, off=0):
    """Chain BIOS codecs by header type until a non-compressed payload remains."""
    t = data[off] & 0xF0
    if t == 0x10:
        d, _ = lz77(data, off)
    elif t == 0x20:
        d, _ = huffman(data, off)
    elif t == 0x30:
        d, _ = rle(data, off)
    else:
        raise ValueError(hex(data[off]))
    if d and (d[0] & 0xF0) in (0x10, 0x20, 0x30) and (struct.unpack_from('<I', d, 0)[0] >> 8) >= len(d) // 2:
        try:
            return bios_decompress(d, 0)
        except (AssertionError, IndexError, struct.error, ValueError):
            pass
    return d
