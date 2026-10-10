"""Text banks: table 0x087B5B64 of webfoot-LZ blobs of UTF-16 text; dialog refs u32 = bank(6) | off_chars(15) << 6 | len(11) << 21."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/tools/extract')
from common import *
from gba import wf_decompress
T = 0x087B5B64
banks = []
for i in range(64):
    p = u32(T + 4 * i)
    if not 0x08000000 <= p < 0x09000000:
        print('end at', i, hex(p)); break
    d, e = wf_decompress(rom(), ra(p))
    banks.append(d)
    print(i, hex(p), len(d), repr(d[:80].decode('utf-16-le', 'replace')))
def ref(v):
    b = banks[v & 0x3f]
    off = 2 * ((v >> 6) & 0x7fff)
    n = v >> 21
    return b[off:off + 2 * n].decode('utf-16-le', 'replace')
for v in (0x03c6f500, 0x01e6fc80, 0x10070040, 0x00c72040, 0x07301580, 0x06f023c0):
    print(hex(v), repr(ref(v)))
