"""script_scene_dump.py idx [idx...] -- dump a scene (entry of the scene table 0x0879B704) as its step list:
header = u32[] indexed by state; entry < 0x10000 = 'exit with value', else pointer to a step record
{u16 next_state, u8 kind, u8 arg, payload}. Kinds: 0 script (code at +3), 1 text (UTF-16 at +4), 2 formatted text
(script at +8 computes printf args, format at +4), 3 narration text object (UTF-16 at +4), 4 compressed script
(LZ at +4), 5/6 dialog text from a compressed bank (u32 ref at +4), 7 goto scene (header ptr at +4)."""
import struct
import sys
from common import *
from script_nat_c_text import lz
from strtab import ustr

SCENES = 0x0879B704
BANKS = 0x087B5B64
_bank = {}


def bank_text(ref):
    b = ref & 0x3F
    if b not in _bank:
        _bank[b] = lz(u32(BANKS + 4 * b))
    d = _bank[b]
    start = ((ref >> 6) & 0x7FFF) * 2
    n = ref >> 21
    units = struct.unpack_from(f'<{n}H', d, start)
    return ''.join(chr(c) if 0x20 <= c < 0x7f else '{%04x}' % c for c in units)


def entries(hdr, maxn=64):
    """Header length is not stored; walk until a word is neither a small value nor a ROM pointer, or until another
    scene header starts (heuristic)."""
    out = []
    for i in range(maxn):
        w = u32(hdr + 4 * i)
        if w < 0x10000 or 0x08000000 <= w < 0x08800000:
            out.append(w)
        else:
            break
    return out


def step(w):
    if w < 0x10000:
        return f'exit({w})'
    nxt, kind, arg = u16(w), u8(w + 2), u8(w + 3)
    s = f'next={nxt} kind={kind} arg={arg}'
    if kind in (1, 3):
        s += ' text=' + repr(ustr(w + 4, 200))
    elif kind in (5, 6):
        s += f' ref={u32(w + 4):#x} text=' + repr(bank_text(u32(w + 4))[:160])
    elif kind == 2:
        s += f' fmt=' + repr(ustr(u32(w + 4), 120) if u32(w + 4) > 0x08000000 else u32(w + 4))
    elif kind == 7:
        s += f' goto={u32(w + 4):#x}'
    return s


if __name__ == '__main__':
    for a in sys.argv[1:]:
        i = int(a, 0)
        hdr = u32(SCENES + 4 * i)
        es = entries(hdr, 12)
        print(f'scene #{i} hdr {hdr:08x}')
        for k, w in enumerate(es):
            print(f'  [{k}] {w:08x} ' + step(w))
