"""cut.py HDR...: print the sections of a script/cutscene header (list of section pointers)."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/tools/extract')
from common import *
from gba import wf_decompress
T = 0x087B5B64
banks = [wf_decompress(rom(), ra(u32(T + 4 * i)))[0] for i in range(7)]


def ref(v):
    b = banks[v & 0x3f]
    off = 2 * ((v >> 6) & 0x7fff)
    n = v >> 21
    return b[off:off + 2 * n].decode('utf-16-le', 'replace')


def show(h, depth=0, seen=None):
    seen = seen if seen is not None else set()
    if h in seen:
        return
    seen.add(h)
    i = 0
    while True:
        s = u32(h + 4 * i)
        pad = '  ' * depth
        if s < 0x10000:
            print(f'{pad}[{i}] END/ret {s}')
            if i > 0 and s == 0:
                # header lists end at the first 0 after entries? continue a little
                pass
            if s == 0 and i > 0:
                break
            i += 1
            continue
        if not 0x08000000 <= s < 0x08800000:
            break
        nxt, kind, b3 = u16(s), u8(s + 2), u8(s + 3)
        if kind in (5, 6):
            print(f'{pad}[{i}] next={nxt} kind{kind} portrait={b3} {ref(u32(s + 4))!r}')
        elif kind == 7:
            print(f'{pad}[{i}] next={nxt} kind7 -> {u32(s + 4):08x}')
            show(u32(s + 4), depth + 1, seen)
        else:
            print(f'{pad}[{i}] next={nxt} kind{kind} b3={b3} @{s:08x}')
        i += 1
        if i > 200:
            break


for a in sys.argv[1:]:
    print('===', a)
    show(int(a, 16))
