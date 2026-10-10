"""script_cat_nodes.py [kind] -- sample event-graph nodes of the event table 0x0879B704.

Event = array of u32 entries; entry < 0x10000 = terminal (result value), else pointer to a node:
  u16 next | u8 kind | u8 arg | payload...   kinds dispatched by table 0x080728AC."""
import sys
from collections import Counter
from common import *
from strtab import S

SCRIPTS = 0x0879B704
want = int(sys.argv[1]) if len(sys.argv) > 1 else None


def u16le(a):
    return u16(a)


def ustr(a, n=60):
    out = []
    for i in range(n):
        c = u16(a + 2 * i)
        if c == 0:
            break
        out.append(chr(c) if 32 <= c < 127 else '{%04x}' % c)
    return ''.join(out)


cnt = Counter()
shown = Counter()
for ev in range(522):
    h = u32(SCRIPTS + 4 * ev)
    for k in range(64):
        v = u32(h + 4 * k)
        if v < 0x10000:
            continue
        if not (0x08000000 <= v < 0x08800000):
            break
        kind = u8(v + 2)
        if kind > 7:
            break
        cnt[kind] += 1
        if (want is None or kind == want) and shown[kind] < 6:
            shown[kind] += 1
            nxt, arg = u16(v), u8(v + 3)
            extra = ''
            if kind in (1,):
                extra = ustr(v + 4)
            elif kind == 3:
                extra = ustr(v + 4)
            elif kind in (2,):
                extra = f'str@{u32(v+4):08x}: ' + ustr(u32(v + 4))
            elif kind in (5, 6):
                extra = f'packed={u32(v+4):08x}'
            elif kind == 7:
                extra = f'goto event hdr {u32(v+4):08x}'
            print(f'ev#{ev} node{k} @{v:08x} next={nxt} kind={kind} arg={arg} {extra}')
print(cnt)
